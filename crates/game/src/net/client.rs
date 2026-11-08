//! 客户端联机：加入重试、输入上行、预测自己的角色、应用权威快照（`05` §5.15.3）。
//!
//! 客户端**只预测自己的角色**：其余玩家与全部玩法结果（碰撞、死亡、房间进度）仍以主机为准。

use std::collections::{BTreeSet, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use bevy::prelude::*;

use super::protocol::{self, InputSample, NetMessage, Snapshot};
use crate::anim_data::ActiveAnim;
use crate::checkpoint::{ActiveCheckpoint, CheckpointPoint};
use crate::config::net::{
    CLIENT_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS, INPUT_REDUNDANCY, INTERP_DELAY_STEPS,
    INTERP_MAX_LAG_STEPS, JOIN_RETRY_SECS, MAX_PACKET_BYTES, MAX_PACKETS_PER_FRAME,
    MAX_PENDING_INPUTS,
};
use crate::config::physics::{COYOTE_STEPS, DEFAULT_PHYSICS, FIXED_DT};
use crate::config::render::VIEWPORT_W;
use crate::physics::{Aabb, Body, StepInput, StepOutcome, step_with_players};
use crate::player::spawn::{PlayerSheet, spawn_player};
use crate::player::{
    PlayerFacing, PlayerId, PlayerInput, PlayerInputs, PlayerMotion, PlayerStepOutcome, next_facing,
};
use crate::render::snap::RenderCenter;
use crate::room::{PickedZones, ZoneId};
use crate::state::{GameState, SessionMode};
use crate::tiles::LevelTiles;
use crate::world::load::{LoadNotice, LoadedLevel, Notice};
use crate::world::progress::LevelProgress;

/// 客户端运行时状态（进入 `Loading` 且会话模式为 `Client` 时存在）。
#[derive(Resource)]
pub struct ClientNet {
    pub socket: UdpSocket,
    pub server: SocketAddr,
    pub player_id: Option<u8>,
    pub welcomed: bool,
    pub last_seq: u32,
    pub last_rx: Instant,
    last_join: Instant,
    started: Instant,
    input_seq: u32,
    client_id: u64,
    /// 已发送、尚未被主机确认的输入（`seq`, 输入）。
    ///
    /// 快照回带 `last_input_seq` 后，丢弃 ≤ 该值的项，再用剩下的输入从权威状态重放，
    /// 让本地预测收敛到主机结果。
    pending_inputs: VecDeque<(u32, PlayerInput)>,
    pending: Option<Snapshot>,
    /// 用于渲染插值的最近快照，按 `seq` 升序，容量 `INTERP_DELAY_STEPS + 1`。
    recent: VecDeque<Snapshot>,
    /// 渲染时钟：快照 `seq` 轴上的浮点位置（`INTERP_DELAY_STEPS` 步落后于最新快照）。
    render_seq: f32,
    bye_sent: bool,
    logged_snapshot: bool,
    logged_join: bool,
}

/// 快照插值的窗口：包住 `render_seq` 的两帧快照与插值系数。
///
/// 客户端每 60 Hz 收到一次快照；渲染帧与快照到达时刻不同步，直接投影权威 `Aabb`
/// 会让**远端角色**按包到达节奏跳动。渲染时统一在 `seq` 轴上回看
/// [`INTERP_DELAY_STEPS`] 步，并在相邻快照之间线性插值。
/// 本地玩家不插值：它在本地按固定步预测（见 [`send_input_and_predict`]）。
fn interpolation_pair(recent: &VecDeque<Snapshot>, render_seq: f32) -> (&Snapshot, &Snapshot, f32) {
    let first = recent.front().expect("recent 非空");
    let last = recent.back().expect("recent 非空");
    if recent.len() == 1 || render_seq <= first.seq as f32 {
        return (first, first, 0.0);
    }
    for (from, to) in recent.iter().zip(recent.iter().skip(1)) {
        let from_seq = from.seq as f32;
        let to_seq = to.seq as f32;
        if render_seq < to_seq {
            let alpha = (render_seq - from_seq) / (to_seq - from_seq);
            return (from, to, alpha.clamp(0.0, 1.0));
        }
    }
    (last, last, 1.0)
}

/// 推进渲染时钟（`seq` 轴）。
///
/// 正常节奏下每帧前进 `dt / FIXED_DT` 步；落后超过
/// [`INTERP_MAX_LAG_STEPS`]（暂停、长卡顿）就直接对齐到「最新 − 延迟」，
/// 但永远不越过最新快照（收包中断时冻结在最后一帧，由超时处理）。
fn advance_render_seq(render_seq: f32, dt_secs: f32, newest_seq: u32) -> f32 {
    let newest = newest_seq as f32;
    let delay = INTERP_DELAY_STEPS as f32;
    let mut next = render_seq + dt_secs / FIXED_DT;
    if next < newest - delay - INTERP_MAX_LAG_STEPS as f32 {
        next = newest - delay;
    }
    next.min(newest)
}

/// 丢弃主机已确认的输入：序号 ≤ `ack` 的输入效果已经包含在权威快照里。
fn drop_acked_inputs(pending: &mut VecDeque<(u32, PlayerInput)>, ack: u32) {
    pending.retain(|(seq, _)| *seq > ack);
}

/// 某玩家在快照里的 AABB 中心（`PlayerSnap` 已按 id 升序存放）。
fn player_center(snapshot: &Snapshot, id: u8) -> Option<Vec2> {
    snapshot
        .players
        .iter()
        .find(|player| player.id == id)
        .map(|player| Vec2::new(player.x, player.y))
}

/// 本安装的稳定客户端标识（写在与存档同目录的 `client_id`）。
///
/// 主机靠它把「关掉窗口后重新加入」识别为同一玩家的重连，而不是新玩家。
/// `ANIMJ_CLIENT_ID` 可覆盖（同机多开测试时给每个实例不同值）。
fn client_id_path() -> std::path::PathBuf {
    crate::save::save_path().with_file_name("client_id")
}

fn load_or_create_client_id() -> u64 {
    if let Ok(raw) = std::env::var("ANIMJ_CLIENT_ID")
        && let Ok(id) = raw.trim().parse::<u64>()
        && id != 0
    {
        return id;
    }
    let path = client_id_path();
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(id) = text.trim().parse::<u64>()
        && id != 0
    {
        return id;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1);
    let id = (nanos ^ ((std::process::id() as u64) << 32)).max(1);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, id.to_string());
    id
}

pub fn start_client(mut commands: Commands, mode: Res<SessionMode>) {
    let SessionMode::Client { server } = &*mode else {
        return;
    };
    let server = *server;
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            if let Err(error) = socket.set_nonblocking(true) {
                warn!("联机：无法设置非阻塞 socket（{error}）");
            }
            let now = Instant::now();
            info!("联机：正在连接主机 {server} …");
            commands.insert_resource(ClientNet {
                socket,
                server,
                player_id: None,
                welcomed: false,
                last_seq: 0,
                last_rx: now,
                last_join: now - std::time::Duration::from_secs(60),
                started: now,
                input_seq: 0,
                client_id: load_or_create_client_id(),
                pending_inputs: VecDeque::with_capacity(MAX_PENDING_INPUTS),
                pending: None,
                recent: VecDeque::with_capacity(INTERP_DELAY_STEPS as usize + 1),
                render_seq: 0.0,
                bye_sent: false,
                logged_snapshot: false,
                logged_join: false,
            });
        }
        Err(error) => warn!("联机：无法创建客户端 socket（{error}）"),
    }
}

/// 加入重试（`JOIN_RETRY_SECS`），同时负责「连不上」的超时提示。
pub fn send_join(
    mut client: Option<ResMut<ClientNet>>,
    mut notice: ResMut<LoadNotice>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    let now = Instant::now();
    if client.welcomed {
        return;
    }
    if now.duration_since(client.started).as_secs_f32() > CONNECT_TIMEOUT_SECS {
        warn!("联机：连接超时");
        notice.0 = Notice::NetTimeout;
        next.set(GameState::MainMenu);
        return;
    }
    if now.duration_since(client.last_join).as_secs_f32() < JOIN_RETRY_SECS {
        return;
    }
    client.last_join = now;
    let join = NetMessage::Join {
        name: "player".to_string(),
        // 空串 = 不校验关卡：客户端已经按房间列表里的 level_id 加载。
        level_id: String::new(),
        client_id: client.client_id,
    };
    if let Some(bytes) = protocol::encode(&join) {
        let _ = client.socket.send_to(&bytes, client.server);
    }
    if !client.logged_join {
        client.logged_join = true;
        info!("联机：已发送加入请求（{}）", client.server);
    }
}

/// 收包：Welcome / Reject / Snapshot / 断线超时。
pub fn poll_client(
    mut client: Option<ResMut<ClientNet>>,
    loaded: Option<Res<LoadedLevel>>,
    mut notice: ResMut<LoadNotice>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    let now = Instant::now();
    let mut buffer = [0u8; MAX_PACKET_BYTES];
    for _ in 0..MAX_PACKETS_PER_FRAME {
        match client.socket.recv_from(&mut buffer) {
            Ok((len, _addr)) => {
                let Some(message) = protocol::decode(&buffer[..len]) else {
                    continue;
                };
                match message {
                    NetMessage::Welcome {
                        player_id,
                        level_id,
                        grid,
                    } => {
                        // 关卡不符直接断开，避免把客户端放进错误地图。
                        if let Some(loaded) = loaded.as_ref()
                            && (loaded.level.name != level_id
                                || (loaded.level.grid.cols, loaded.level.grid.rows) != grid)
                        {
                            warn!(
                                "联机：主机关卡 {level_id}({grid:?}) 与本地 {}({:?}) 不一致",
                                loaded.level.name,
                                (loaded.level.grid.cols, loaded.level.grid.rows)
                            );
                            notice.0 = Notice::NetLevelMismatch;
                            next.set(GameState::MainMenu);
                            return;
                        }
                        client.player_id = Some(player_id);
                        client.welcomed = true;
                        client.last_rx = now;
                        info!("联机：已连接 —— 玩家 {player_id}（{}）", client.server);
                    }
                    NetMessage::Reject { reason } => {
                        warn!("联机：主机拒绝加入（{reason}）");
                        notice.0 = Notice::NetRejected;
                        next.set(GameState::MainMenu);
                        return;
                    }
                    NetMessage::Snapshot(snapshot) => {
                        client.last_rx = now;
                        if client.welcomed && snapshot.seq > client.last_seq {
                            client.last_seq = snapshot.seq;
                            // 首帧把渲染时钟对齐到「最新 − 插值延迟」；此后只按帧时间推进，
                            // 不因到达抖动重置，否则插值系数会每包跳一次。
                            if client.recent.is_empty() {
                                client.render_seq = snapshot.seq as f32 - INTERP_DELAY_STEPS as f32;
                            }
                            let capacity = INTERP_DELAY_STEPS as usize + 1;
                            while client.recent.len() >= capacity {
                                client.recent.pop_front();
                            }
                            client.recent.push_back(snapshot.clone());
                            client.pending = Some(snapshot);
                        }
                    }
                    NetMessage::Bye => {
                        warn!("联机：主机关闭了房间");
                        notice.0 = Notice::NetDisconnected;
                        next.set(GameState::MainMenu);
                        return;
                    }
                    _ => {}
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                // Windows 在主机未监听时用 ICMP 端口不可达把下一次 recv 变成 10054；
                // 这不是协议错误，静默等待重试即可。
                break;
            }
            Err(error) => {
                warn!("联机：客户端收包失败（{error}）");
                break;
            }
        }
    }

    if client.welcomed && now.duration_since(client.last_rx).as_secs_f32() > CLIENT_TIMEOUT_SECS {
        warn!("联机：与主机的连接超时");
        notice.0 = Notice::NetDisconnected;
        next.set(GameState::MainMenu);
    }
}

/// 应用最新快照（`Update`，早于相机）。
///
/// 远端玩家直接采用权威位置；本地玩家先采用权威位置，再重放未确认输入，
/// 把本地预测收敛到主机结果（`05` §5.15.3）。
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn apply_snapshot(
    mut commands: Commands,
    mut client: Option<ResMut<ClientNet>>,
    state: Res<State<GameState>>,
    sheet: Option<Res<PlayerSheet>>,
    anim: Option<Res<ActiveAnim>>,
    tiles: Option<Res<LevelTiles>>,
    collision: Res<crate::player::collision::PlayerCollision>,
    mut progress: Option<ResMut<LevelProgress>>,
    mut active: Option<ResMut<ActiveCheckpoint>>,
    mut picked: Option<ResMut<PickedZones>>,
    run: Option<ResMut<crate::run::RunState>>,
    mut spawners: Query<(
        &ZoneId,
        &mut crate::player::items::SpawnerItem,
        &mut Visibility,
    )>,
    mut players: Query<(
        Entity,
        &PlayerId,
        &mut PlayerMotion,
        &mut PlayerStepOutcome,
        &mut PlayerFacing,
        &mut crate::player::items::CarriedItem,
    )>,
    zones: Query<(Entity, &ZoneId)>,
) {
    if *state.get() != GameState::Playing {
        return; // Loading 期间先攒着最新一份
    }
    let Some(client) = client.as_mut() else {
        return;
    };
    if !client.welcomed {
        return;
    }
    let Some(snapshot) = client.pending.take() else {
        return;
    };

    let alive: BTreeSet<u8> = snapshot.players.iter().map(|player| player.id).collect();
    let existing: Vec<(u8, Entity)> = players
        .iter()
        .map(|(entity, id, _, _, _, _)| (id.0, entity))
        .collect();

    for snap in &snapshot.players {
        let mut body = Body::at_center_with(Vec2::new(snap.x, snap.y), collision.size_vec());
        body.vel = Vec2::new(snap.vx, snap.vy);
        body.grounded = snap.grounded;
        body.airborne_since = if snap.grounded { 0 } else { COYOTE_STEPS + 1 };
        body.buffer_age = u32::MAX;
        body.air_jumps_used = snap.air_jumps_used;
        if let Some((_, entity)) = existing.iter().find(|(id, _)| *id == snap.id)
            && let Ok((_, _, mut motion, mut outcome, mut facing, mut item)) =
                players.get_mut(*entity)
        {
            motion.0 = body;
            outcome.0 = crate::physics::StepOutcome::default();
            facing.0 = snap.facing_right;
            item.kind = snap.item;
            item.remaining = snap.item_remaining;
            continue;
        }
        // 新加入的玩家：生成实体；速度在下一份快照补齐（位置已经正确）。
        if let (Some(sheet), Some(anim)) = (sheet.as_ref(), anim.as_ref()) {
            let entity = spawn_player(
                &mut commands,
                sheet,
                &anim.set,
                PlayerId(snap.id),
                Vec2::new(snap.x, snap.y),
                &collision,
            );
            // 只有远端玩家走渲染插值；本地玩家由预测位置直接投影。
            if client.player_id != Some(snap.id) {
                commands
                    .entity(entity)
                    .insert(RenderCenter(Vec2::new(snap.x, snap.y)));
            }
        }
    }

    // 主机快照里没有的玩家 = 已离开。
    for (id, entity) in existing {
        if !alive.contains(&id) {
            commands.entity(entity).despawn();
        }
    }

    // 本地玩家：丢弃已确认的输入，再从权威状态重放剩余输入（客户端预测的收敛步骤）。
    if let Some(local_id) = client.player_id
        && let Some(local_snap) = snapshot.players.iter().find(|player| player.id == local_id)
    {
        drop_acked_inputs(&mut client.pending_inputs, local_snap.last_input_seq);
        if let (Some(tiles), Some((_, _, mut motion, mut outcome, _, _))) = (
            tiles.as_ref(),
            players
                .iter_mut()
                .find(|(_, id, _, _, _, _)| id.0 == local_id),
        ) {
            // 与主机同一物理函数；其他玩家用本快照的 AABB 当实心障碍。
            let mut cfg = DEFAULT_PHYSICS;
            cfg.player_size = collision.size_vec();
            crate::player::items::apply_item_to_physics(&mut cfg, local_snap.item);
            let others: Vec<Aabb> = snapshot
                .players
                .iter()
                .filter(|player| player.id != local_id)
                .map(|player| {
                    Aabb::from_center_size(Vec2::new(player.x, player.y), cfg.player_size)
                })
                .collect();
            for (_, input) in &client.pending_inputs {
                let mut out = StepOutcome::default();
                step_with_players(
                    &cfg,
                    tiles.as_ref(),
                    &mut motion.0,
                    StepInput {
                        move_x: input.move_x,
                        jump_pressed: input.jump_pressed,
                        jump_held: input.jump_held,
                    },
                    &others,
                    &mut out,
                );
                outcome.0 = out;
            }
        }
    }

    if let Some(progress) = progress.as_mut() {
        progress.apply_net(&snapshot.fruits);
    }
    if let Some(mut run) = run {
        run.elapsed = snapshot.run_elapsed;
        // 快照是完整状态：先清空，再按快照重建，避免离开/重连的旧成绩残留。
        run.finished = [None; crate::config::net::MAX_PLAYERS];
        for snap in &snapshot.players {
            if let (Some(time), Some(slot)) =
                (snap.finish, run.finished.get_mut(PlayerId(snap.id).index()))
            {
                *slot = Some(time);
            }
        }
    }
    if let Some(picked) = picked.as_mut() {
        for zone in &snapshot.picked {
            picked.0.insert(*zone);
        }
        for (entity, zone) in &zones {
            if picked.0.contains(&zone.0) {
                commands.entity(entity).despawn();
            }
        }
    }
    // 生成器状态：快照只列**未激活**的，其余恢复为激活/可见。
    let inactive: BTreeSet<u32> = snapshot.inactive_item_spawners.iter().copied().collect();
    for (id, mut spawner, mut visibility) in &mut spawners {
        let active = !inactive.contains(&id.0);
        spawner.active = active;
        spawner.respawn_left = 0.0;
        *visibility = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(active) = active.as_mut() {
        active.0 = snapshot.checkpoint.map(|(col, row, x, y)| CheckpointPoint {
            col,
            row,
            at: (x, y),
        });
    }

    if !client.logged_snapshot {
        client.logged_snapshot = true;
        info!(
            "联机：已接收快照（seq {}，{} 名玩家）",
            snapshot.seq,
            snapshot.players.len()
        );
    }
}

/// 点击窗口关闭按钮时立刻 `Bye`。
///
/// 不能只依赖 `AppExit`：窗口关闭流程是「请求 → 下一帧标记 ClosingWindow → 再下一帧 despawn →
/// `exit_on_all_closed` 才写 AppExit」，而应用可能在写 AppExit 的同一帧结束；先发 Bye 最稳。
pub fn bye_on_window_close(
    mut requested: MessageReader<bevy::window::WindowCloseRequested>,
    mut client: Option<ResMut<ClientNet>>,
) {
    if requested.read().count() == 0 {
        return;
    }
    if let Some(client) = client.as_mut() {
        send_bye(client, "窗口关闭");
    }
}

/// 进程正常退出时兜底再发一次 `Bye`（窗口关闭路径可能已经发过）。
pub fn bye_on_exit(mut exit: MessageReader<AppExit>, mut client: Option<ResMut<ClientNet>>) {
    if exit.read().count() == 0 {
        return;
    }
    if let Some(client) = client.as_mut() {
        send_bye(client, "退出");
    }
}

/// 只发一次 `Bye`：窗口关闭与 AppExit 可能在同一帧先后触发。
fn send_bye(client: &mut ClientNet, reason: &str) {
    if client.bye_sent {
        return;
    }
    client.bye_sent = true;
    if let Some(bytes) = protocol::encode(&NetMessage::Bye) {
        let _ = client.socket.send_to(&bytes, client.server);
        info!("联机：{reason}，已通知主机断开（{}）", client.server);
    }
}

/// 按快照 `seq` 轴推进渲染时钟，把所有玩家（含本地）插值到渲染时刻。
///
/// 快照每固定步递增一个 `seq`，因此 `seq` 就是无抖动的时间轴：渲染时刻固定回看
/// [`INTERP_DELAY_STEPS`] 步，相邻快照之间线性插值。丢一个包时两帧间隔变大，
/// 插值会跨过该间隔继续以平均速度前进；到达时刻的抖动也被这道缓冲吸收。
pub fn advance_net_visuals(
    time: Res<Time>,
    mut client: Option<ResMut<ClientNet>>,
    mut visuals: Query<(&PlayerId, &mut RenderCenter)>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    let Some(newest_seq) = client.recent.back().map(|snapshot| snapshot.seq) else {
        return;
    };
    client.render_seq = advance_render_seq(client.render_seq, time.delta_secs(), newest_seq);

    let (from, to, alpha) = interpolation_pair(&client.recent, client.render_seq);
    for (id, mut center) in &mut visuals {
        let from = player_center(from, id.0);
        let to = player_center(to, id.0);
        center.0 = match (from, to) {
            // 重生/传送会让两帧相距整屏以上：直接取目标，避免跨房间拖影。
            (Some(from), Some(to)) if from.distance(to) > VIEWPORT_W as f32 => to,
            (Some(from), Some(to)) => from.lerp(to, alpha),
            (Some(from), None) => from,
            (None, Some(to)) => to,
            (None, None) => continue,
        };
    }
}

/// 每个固定步：上行一次输入，并用同一份输入在本地预测自己的角色。
///
/// 预测让「按键 → 自己角色开始动」不再依赖快照往返；快照到达后
/// [`apply_snapshot`] 用 `last_input_seq` 丢弃已确认输入并重放剩余输入，纠正偏差。
pub fn send_input_and_predict(
    mut client: Option<ResMut<ClientNet>>,
    state: Res<State<GameState>>,
    tiles: Option<Res<LevelTiles>>,
    collision: Res<crate::player::collision::PlayerCollision>,
    mut inputs: ResMut<PlayerInputs>,
    mut players: Query<(
        &PlayerId,
        &mut PlayerMotion,
        &mut PlayerStepOutcome,
        &mut PlayerFacing,
        &crate::player::items::CarriedItem,
    )>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    if !client.welcomed || *state.get() != GameState::Playing {
        return;
    }
    let Some(local_id) = client.player_id else {
        return;
    };

    let input: PlayerInput = inputs.slots[0];
    client.input_seq = client.input_seq.wrapping_add(1);
    let seq = client.input_seq;
    if client.pending_inputs.len() >= MAX_PENDING_INPUTS {
        client.pending_inputs.pop_front();
    }
    client.pending_inputs.push_back((seq, input));

    // 冗余重发最近几条未确认输入：单个 UDP 包丢失不会让某条输入永远消失。
    let samples: Vec<InputSample> = client
        .pending_inputs
        .iter()
        .rev()
        .take(INPUT_REDUNDANCY)
        .rev()
        .map(|(seq, input)| InputSample {
            seq: *seq,
            move_x: input.move_x,
            jump_pressed: input.jump_pressed,
            jump_held: input.jump_held,
            interact_pressed: input.interact_pressed,
        })
        .collect();
    let message = NetMessage::Input { samples };
    if let Some(bytes) = protocol::encode(&message) {
        let _ = client.socket.send_to(&bytes, client.server);
    }
    // 一个按下只发一次；`move_x` / `jump_held` 是持续量。
    inputs.slots[0].jump_pressed = false;
    inputs.slots[0].interact_pressed = false;

    // ---- 本地预测（与主机同一个物理函数，确定性）----
    let Some(tiles) = tiles.as_ref() else {
        return;
    };
    let others: Vec<Aabb> = players
        .iter()
        .filter(|(id, _, _, _, _)| id.0 != local_id)
        .map(|(_, motion, _, _, _)| motion.0.aabb)
        .collect();
    let Some((_, mut motion, mut outcome, mut facing, carried)) =
        players.iter_mut().find(|(id, _, _, _, _)| id.0 == local_id)
    else {
        return; // 本地玩家实体还没由第一份快照生成
    };
    let mut out = StepOutcome::default();
    let mut cfg = DEFAULT_PHYSICS;
    cfg.player_size = collision.size_vec();
    crate::player::items::apply_item_to_physics(&mut cfg, carried.kind);
    step_with_players(
        &cfg,
        tiles.as_ref(),
        &mut motion.0,
        StepInput {
            move_x: input.move_x,
            jump_pressed: input.jump_pressed,
            jump_held: input.jump_held,
        },
        &others,
        &mut out,
    );
    outcome.0 = out;
    if input.move_x != 0 {
        facing.0 = next_facing(facing.0, input.move_x);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::PlayerSnap;

    fn snapshot(seq: u32, x: f32) -> Snapshot {
        Snapshot {
            seq,
            run_elapsed: seq as f32 / 60.0,
            players: vec![PlayerSnap {
                id: 1,
                x,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                grounded: true,
                facing_right: true,
                item: None,
                item_remaining: 0.0,
                air_jumps_used: 0,
                finish: None,
                last_input_seq: 0,
            }],
            fruits: Vec::new(),
            picked: Vec::new(),
            inactive_item_spawners: Vec::new(),
            checkpoint: None,
        }
    }

    /// 相邻快照之间：端点精确命中，中点线性插值。
    #[test]
    fn interpolates_between_adjacent_snapshots() {
        let recent = VecDeque::from(vec![snapshot(10, 0.0), snapshot(11, 10.0)]);

        let (from, to, alpha) = interpolation_pair(&recent, 10.5);
        assert_eq!(alpha, 0.5);
        assert_eq!(player_center(from, 1), Some(Vec2::ZERO));
        assert_eq!(player_center(to, 1), Some(Vec2::new(10.0, 0.0)));

        // 下端点渲染上一帧；上端点渲染最新帧。
        let (from, _, alpha) = interpolation_pair(&recent, 10.0);
        assert_eq!((alpha, player_center(from, 1)), (0.0, Some(Vec2::ZERO)));
        let (_, to, alpha) = interpolation_pair(&recent, 11.0);
        assert_eq!(
            (alpha, player_center(to, 1)),
            (1.0, Some(Vec2::new(10.0, 0.0)))
        );
    }

    /// 丢一个包时两帧相隔 2 个 `seq`：插值跨过缺口，仍以平均速度前进。
    #[test]
    fn interpolates_across_a_lost_snapshot() {
        let recent = VecDeque::from(vec![snapshot(10, 0.0), snapshot(12, 10.0)]);
        let (from, to, alpha) = interpolation_pair(&recent, 11.0);

        assert_eq!(alpha, 0.5);
        assert_eq!(
            player_center(from, 1)
                .unwrap()
                .lerp(player_center(to, 1).unwrap(), alpha),
            Vec2::new(5.0, 0.0)
        );
    }

    /// 单个快照丢失时，渲染位置仍然每步前进——不会「冻结一帧再跳」。
    #[test]
    fn a_lost_snapshot_never_freezes_the_visual() {
        let mut recent: VecDeque<Snapshot> = VecDeque::new();
        let mut render_seq = 0.0;
        let mut xs = Vec::new();
        for step in 0..20u32 {
            if step != 7 {
                let capacity = INTERP_DELAY_STEPS as usize + 1;
                while recent.len() >= capacity {
                    recent.pop_front();
                }
                if recent.is_empty() {
                    render_seq = step as f32 - INTERP_DELAY_STEPS as f32;
                }
                recent.push_back(snapshot(step, step as f32 * 3.0));
            }
            let newest = recent.back().expect("至少一帧").seq;
            render_seq = advance_render_seq(render_seq, FIXED_DT, newest);
            let (from, to, alpha) = interpolation_pair(&recent, render_seq);
            let x = player_center(from, 1)
                .unwrap()
                .lerp(player_center(to, 1).unwrap(), alpha)
                .x;
            xs.push(x);
        }
        // 跳过启动阶段（首帧没有前一帧可插值）；之后每个渲染步都必须前进。
        for pair in xs[3..].windows(2) {
            assert!(pair[1] > pair[0], "视觉冻结：{xs:?}");
        }
    }

    /// 渲染时钟越界（首帧之前 / 末帧之后）夹到端点，不外插。
    #[test]
    fn clamps_outside_snapshot_range() {
        let recent = VecDeque::from(vec![snapshot(10, 3.0), snapshot(11, 7.0)]);
        assert_eq!(interpolation_pair(&recent, 0.0).2, 0.0);
        assert_eq!(interpolation_pair(&recent, 99.0).2, 1.0);
    }

    /// 客户端只重放主机尚未确认的输入；确认序号处的输入必须被丢弃。
    #[test]
    fn acked_inputs_are_dropped_before_replay() {
        let mut pending: VecDeque<(u32, PlayerInput)> =
            (1..=5).map(|seq| (seq, PlayerInput::default())).collect();

        drop_acked_inputs(&mut pending, 3);
        assert_eq!(
            pending.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            vec![4, 5]
        );

        drop_acked_inputs(&mut pending, 5);
        assert!(pending.is_empty());
    }

    /// 正常节奏下渲染时钟与最新快照保持固定延迟。
    #[test]
    fn render_clock_keeps_delay_under_steady_packets() {
        // 快照 9 刚到达：渲染时钟对齐到 9 − 延迟，随后每步前进 1，与下一个到达同步。
        let mut render_seq = 9.0 - INTERP_DELAY_STEPS as f32;
        for newest in 10..200 {
            render_seq = advance_render_seq(render_seq, FIXED_DT, newest);
            assert!(
                (newest as f32 - render_seq - INTERP_DELAY_STEPS as f32).abs() < 0.01,
                "newest={newest} render_seq={render_seq}"
            );
        }
    }

    /// 暂停/长卡顿后对齐到「最新 − 延迟」，且从不越过最新快照。
    #[test]
    fn render_clock_resyncs_after_a_stall() {
        let render_seq = advance_render_seq(10.0, 0.0, 1000);
        assert_eq!(render_seq, 1000.0 - INTERP_DELAY_STEPS as f32);

        let render_seq = advance_render_seq(999.9, FIXED_DT, 1000);
        assert_eq!(render_seq, 1000.0);
    }
}
