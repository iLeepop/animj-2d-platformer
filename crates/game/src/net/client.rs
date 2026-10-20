//! 客户端联机：加入重试、输入上行、应用权威快照（`05` §5.15.3）。
//!
//! 客户端**不跑物理**：玩家位置完全来自主机快照。这让碰撞、死亡、房间进度只有一个真相。

use std::collections::BTreeSet;
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use bevy::prelude::*;

use super::protocol::{self, NetMessage, Snapshot};
use crate::anim_data::ActiveAnim;
use crate::checkpoint::{ActiveCheckpoint, CheckpointPoint};
use crate::config::net::{
    CLIENT_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS, JOIN_RETRY_SECS, MAX_PACKET_BYTES,
    MAX_PACKETS_PER_FRAME,
};
use crate::config::physics::{COYOTE_STEPS, FIXED_DT};
use crate::config::render::VIEWPORT_W;
use crate::physics::Body;
use crate::player::spawn::{PlayerSheet, spawn_player};
use crate::player::{
    PlayerFacing, PlayerId, PlayerInput, PlayerInputs, PlayerMotion, PlayerStepOutcome,
};
use crate::render::snap::RenderCenter;
use crate::room::{PickedZones, ZoneId};
use crate::state::{GameState, SessionMode};
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
    pending: Option<Snapshot>,
    logged_snapshot: bool,
    logged_join: bool,
}

/// 远端玩家的渲染插值状态（只影响 `Transform`，不影响权威 `Aabb`）。
///
/// 客户端每 60 Hz 收到一次快照；渲染帧可能远高于 60 Hz。若直接跳位置，远端角色会
/// 「停几帧 → 跳一段」，在移动相机下看起来像拖影/残影。这里在两个快照之间线性插值。
#[derive(Component, Debug)]
pub struct NetVisual {
    from: Vec2,
    to: Vec2,
    current: Vec2,
    elapsed: f32,
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
                pending: None,
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

/// 应用最新快照（`Update`，早于相机；客户端不跑物理）。
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn apply_snapshot(
    mut commands: Commands,
    mut client: Option<ResMut<ClientNet>>,
    state: Res<State<GameState>>,
    sheet: Option<Res<PlayerSheet>>,
    anim: Option<Res<ActiveAnim>>,
    mut progress: Option<ResMut<LevelProgress>>,
    mut active: Option<ResMut<ActiveCheckpoint>>,
    mut picked: Option<ResMut<PickedZones>>,
    mut players: Query<(
        Entity,
        &PlayerId,
        &mut PlayerMotion,
        &mut PlayerStepOutcome,
        &mut PlayerFacing,
        Option<&mut NetVisual>,
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
        let mut body = Body::at_center(Vec2::new(snap.x, snap.y));
        body.vel = Vec2::new(snap.vx, snap.vy);
        body.grounded = snap.grounded;
        body.airborne_since = if snap.grounded { 0 } else { COYOTE_STEPS + 1 };
        body.buffer_age = u32::MAX;
        if let Some((_, entity)) = existing.iter().find(|(id, _)| *id == snap.id)
            && let Ok((_, _, mut motion, mut outcome, mut facing, visual)) =
                players.get_mut(*entity)
        {
            motion.0 = body;
            outcome.0 = crate::physics::StepOutcome::default();
            facing.0 = snap.facing_right;
            // 本地玩家不插值（相机跟随它，屏幕位置本来就稳定）；远端玩家做渲染插值。
            let is_local = client.player_id == Some(snap.id);
            if !is_local && let Some(mut visual) = visual {
                let target = Vec2::new(snap.x, snap.y);
                visual.from = visual.current;
                visual.to = target;
                visual.elapsed = 0.0;
                if visual.from.distance(target) > VIEWPORT_W as f32 {
                    visual.from = target;
                    visual.current = target;
                }
            }
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
            );
            if client.player_id != Some(snap.id) {
                let center = Vec2::new(snap.x, snap.y);
                commands.entity(entity).insert((
                    NetVisual {
                        from: center,
                        to: center,
                        current: center,
                        elapsed: FIXED_DT,
                    },
                    RenderCenter(center),
                ));
            }
        }
    }

    // 主机快照里没有的玩家 = 已离开。
    for (id, entity) in existing {
        if !alive.contains(&id) {
            commands.entity(entity).despawn();
        }
    }

    if let Some(progress) = progress.as_mut() {
        progress.apply_net(&snapshot.cleared, &snapshot.fruits);
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

/// 窗口关闭 / 进程正常退出时主动 `Bye`，主机立即移除角色，不等 5 秒超时。
pub fn bye_on_exit(mut exit: MessageReader<AppExit>, client: Option<Res<ClientNet>>) {
    if exit.read().count() == 0 {
        return;
    }
    let Some(client) = client else {
        return;
    };
    if let Some(bytes) = protocol::encode(&NetMessage::Bye) {
        let _ = client.socket.send_to(&bytes, client.server);
        info!("联机：退出，已通知主机断开（{}）", client.server);
    }
}

/// 把远端玩家从一个快照位置插值推进到下一个快照位置。
pub fn advance_net_visuals(
    time: Res<Time>,
    mut visuals: Query<(&mut NetVisual, &mut RenderCenter)>,
) {
    let dt = time.delta_secs();
    for (mut visual, mut center) in &mut visuals {
        visual.elapsed += dt;
        let t = (visual.elapsed / FIXED_DT).clamp(0.0, 1.0);
        visual.current = visual.from.lerp(visual.to, t);
        center.0 = visual.current;
    }
}

/// 每个固定步上行一次输入（客户端无物理，所以锁存由这里消费）。
pub fn send_input(
    mut client: Option<ResMut<ClientNet>>,
    state: Res<State<GameState>>,
    mut inputs: ResMut<PlayerInputs>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    if !client.welcomed || *state.get() != GameState::Playing {
        return;
    }
    let input: PlayerInput = inputs.slots[0];
    client.input_seq = client.input_seq.wrapping_add(1);
    let message = NetMessage::Input {
        seq: client.input_seq,
        move_x: input.move_x,
        jump_pressed: input.jump_pressed,
        jump_held: input.jump_held,
        interact_pressed: input.interact_pressed,
    };
    if let Some(bytes) = protocol::encode(&message) {
        let _ = client.socket.send_to(&bytes, client.server);
    }
    // 一个按下只发一次；`move_x` / `jump_held` 是持续量。
    inputs.slots[0].jump_pressed = false;
    inputs.slots[0].interact_pressed = false;
}
