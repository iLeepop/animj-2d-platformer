//! 主机侧联机：加入、输入收集、快照广播、超时（`05` §5.15.3）。
//!
//! 主机**权威**：全部玩家（本地 + 远程）都由主机物理推进，远程玩家只是输入槽的写入者。
//! 客户端每固定步发一条输入，主机每固定步广播一条快照。

use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use bevy::prelude::*;

use super::protocol::{self, NetMessage, PlayerSnap, Snapshot};
use crate::anim_data::ActiveAnim;
use crate::checkpoint::ActiveCheckpoint;
use crate::config::net::{
    CLIENT_TIMEOUT_SECS, DEFAULT_ROOM_NAME, MAX_PACKET_BYTES, MAX_PACKETS_PER_FRAME, MAX_PLAYERS,
    NET_PORT, PROTOCOL_VERSION, REMOTE_INPUT_STALE_SECS,
};
use crate::player::spawn::{PlayerSheet, nearest_free_spawn, spawn_player};
use crate::player::{PlayerFacing, PlayerId, PlayerInput, PlayerInputs, PlayerMotion};
use crate::room::PickedZones;
use crate::state::SessionMode;
use crate::tiles::LevelTiles;
use crate::world::load::LoadedLevel;
use crate::world::progress::LevelProgress;

/// 一条已加入的客户端连接。
pub struct ClientConn {
    pub addr: SocketAddr,
    pub player_id: u8,
    pub entity: Option<Entity>,
    pub last_seen: Instant,
    pub last_input: Instant,
    pub last_input_seq: u32,
}

/// 主机运行时状态（`OnEnter(Playing)` 且会话模式为 `Host` 时存在）。
#[derive(Resource)]
pub struct HostNet {
    pub socket: UdpSocket,
    pub clients: Vec<ClientConn>,
    pub seq: u32,
    logged_snapshot: bool,
}

/// 进入游戏时开房（`05` §5.15.4）。绑定失败则降级为离线单人，不 panic。
pub fn start_host(mut commands: Commands, mut mode: ResMut<SessionMode>) {
    if !mode.is_host() {
        return;
    }
    match UdpSocket::bind(("0.0.0.0", NET_PORT)) {
        Ok(socket) => {
            if let Err(error) = socket.set_nonblocking(true) {
                warn!("联机：无法设置非阻塞 socket（{error}）");
            }
            let _ = socket.set_broadcast(true);
            info!(
                "联机：房间已开启（UDP {NET_PORT}，最多 {MAX_PLAYERS} 人，协议 v{PROTOCOL_VERSION}）"
            );
            commands.insert_resource(HostNet {
                socket,
                clients: Vec::new(),
                seq: 0,
                logged_snapshot: false,
            });
        }
        Err(error) => {
            warn!("联机：无法监听 UDP {NET_PORT}（{error}），本局按单人继续");
            *mode = SessionMode::Solo;
        }
    }
}

/// 收包：发现查询、加入、输入、离开；随后清理超时客户端。
#[allow(clippy::too_many_arguments)]
pub fn poll_host(
    mut commands: Commands,
    mut host: Option<ResMut<HostNet>>,
    mut inputs: ResMut<PlayerInputs>,
    loaded: Option<Res<LoadedLevel>>,
    tiles: Option<Res<LevelTiles>>,
    sheet: Option<Res<PlayerSheet>>,
    anim: Option<Res<ActiveAnim>>,
    players: Query<&PlayerMotion>,
) {
    let Some(host) = host.as_mut() else {
        return;
    };
    let now = Instant::now();

    // 先把本轮所有包读出来，避免在持有 `clients` 可变借用时再做 socket I/O。
    let mut packets = Vec::new();
    let mut buffer = [0u8; MAX_PACKET_BYTES];
    for _ in 0..MAX_PACKETS_PER_FRAME {
        match host.socket.recv_from(&mut buffer) {
            Ok((len, addr)) => {
                if let Some(message) = protocol::decode(&buffer[..len]) {
                    packets.push((message, addr));
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
                break;
            }
            Err(error) => {
                warn!("联机：主机收包失败（{error}）");
                break;
            }
        }
    }

    let level_id = loaded
        .as_ref()
        .map(|loaded| loaded.level.name.clone())
        .unwrap_or_default();

    for (message, addr) in packets {
        match message {
            NetMessage::Discover { token } => {
                let room = NetMessage::RoomInfo {
                    token,
                    protocol: PROTOCOL_VERSION,
                    name: DEFAULT_ROOM_NAME.to_string(),
                    level_id: level_id.clone(),
                    players: (1 + host.clients.len()) as u8,
                    max_players: MAX_PLAYERS as u8,
                    port: NET_PORT,
                };
                if let Some(bytes) = protocol::encode(&room) {
                    let _ = host.socket.send_to(&bytes, addr);
                }
            }
            NetMessage::Join {
                level_id: requested,
                ..
            } => {
                if let Some(existing) = host.clients.iter_mut().find(|conn| conn.addr == addr) {
                    existing.last_seen = now;
                    let welcome = NetMessage::Welcome {
                        player_id: existing.player_id,
                        level_id: level_id.clone(),
                        grid: loaded
                            .as_ref()
                            .map(|loaded| (loaded.level.grid.cols, loaded.level.grid.rows))
                            .unwrap_or((0, 0)),
                    };
                    if let Some(bytes) = protocol::encode(&welcome) {
                        let _ = host.socket.send_to(&bytes, addr);
                    }
                    continue;
                }
                if !requested.is_empty() && requested != level_id {
                    let reject = NetMessage::Reject {
                        reason: "level mismatch".into(),
                    };
                    if let Some(bytes) = protocol::encode(&reject) {
                        let _ = host.socket.send_to(&bytes, addr);
                    }
                    continue;
                }
                if host.clients.len() + 1 >= MAX_PLAYERS {
                    let reject = NetMessage::Reject {
                        reason: "room full".into(),
                    };
                    if let Some(bytes) = protocol::encode(&reject) {
                        let _ = host.socket.send_to(&bytes, addr);
                    }
                    continue;
                }
                let (Some(loaded), Some(tiles), Some(sheet), Some(anim)) = (
                    loaded.as_ref(),
                    tiles.as_ref(),
                    sheet.as_ref(),
                    anim.as_ref(),
                ) else {
                    let reject = NetMessage::Reject {
                        reason: "host not ready".into(),
                    };
                    if let Some(bytes) = protocol::encode(&reject) {
                        let _ = host.socket.send_to(&bytes, addr);
                    }
                    continue;
                };
                let Some(player_id) = (1..MAX_PLAYERS as u8)
                    .find(|id| host.clients.iter().all(|conn| conn.player_id != *id))
                else {
                    continue;
                };
                // 加入点：**关卡出生点**（`player_spawn` 所在房间的重生点），不是主机当前位置。
                // 出生点被实心瓦片/已有玩家占用时，就近找空位。
                let (_, _, base) = crate::world::initial_spawn(&loaded.level);
                let occupied: Vec<crate::physics::Aabb> =
                    players.iter().map(|motion| motion.0.aabb).collect();
                let center = nearest_free_spawn(base, tiles, &occupied);
                let entity =
                    spawn_player(&mut commands, sheet, &anim.set, PlayerId(player_id), center);
                host.clients.push(ClientConn {
                    addr,
                    player_id,
                    entity: Some(entity),
                    last_seen: now,
                    last_input: now,
                    last_input_seq: 0,
                });
                let welcome = NetMessage::Welcome {
                    player_id,
                    level_id: level_id.clone(),
                    grid: (loaded.level.grid.cols, loaded.level.grid.rows),
                };
                if let Some(bytes) = protocol::encode(&welcome) {
                    let _ = host.socket.send_to(&bytes, addr);
                }
                info!(
                    "联机：客户端已加入 —— 玩家 {player_id}（{addr}），出生点 ({:.0}, {:.0})",
                    center.x, center.y
                );
            }
            NetMessage::Input {
                seq,
                move_x,
                jump_pressed,
                jump_held,
                interact_pressed,
            } => {
                let Some(conn) = host.clients.iter_mut().find(|conn| conn.addr == addr) else {
                    continue;
                };
                conn.last_seen = now;
                if seq <= conn.last_input_seq {
                    continue; // 乱序/重复包
                }
                conn.last_input_seq = seq;
                conn.last_input = now;
                // 不能整槽覆盖：渲染帧可能比固定步快，两个输入包会在**同一个**固定步之间到达；
                // 后到的 `jump_pressed: false` 会把先到的按下吞掉（跳跃丢失）。
                let slot = inputs.get_mut(PlayerId(conn.player_id));
                slot.move_x = move_x;
                slot.jump_held = jump_held;
                slot.jump_pressed |= jump_pressed;
                slot.interact_pressed |= interact_pressed;
            }
            NetMessage::Bye => {
                remove_client(&mut host.clients, &mut commands, &mut inputs, addr, "离开");
            }
            _ => {}
        }
    }

    // 输入停更（客户端暂停/卡顿）⇒ 清零持续量，否则角色会一直按着最后的移动方向。
    // 锁存的 `jump_pressed` / `interact_pressed` 保留给下一个固定步消费。
    for conn in &host.clients {
        if now.duration_since(conn.last_input).as_secs_f32() > REMOTE_INPUT_STALE_SECS {
            let slot = inputs.get_mut(PlayerId(conn.player_id));
            slot.move_x = 0;
            slot.jump_held = false;
        }
    }

    // 超时：静默断开（拔网线/关进程）不会留下幽灵玩家。
    let mut expired = Vec::new();
    host.clients.retain(|conn| {
        if now.duration_since(conn.last_seen).as_secs_f32() > CLIENT_TIMEOUT_SECS {
            expired.push(conn.addr);
            false
        } else {
            true
        }
    });
    for addr in expired {
        remove_client(&mut host.clients, &mut commands, &mut inputs, addr, "超时");
    }
}

fn remove_client(
    clients: &mut Vec<ClientConn>,
    commands: &mut Commands,
    inputs: &mut PlayerInputs,
    addr: SocketAddr,
    reason: &str,
) {
    let Some(index) = clients.iter().position(|conn| conn.addr == addr) else {
        return;
    };
    let conn = clients.remove(index);
    if let Some(entity) = conn.entity {
        commands.entity(entity).despawn();
    }
    *inputs.get_mut(PlayerId(conn.player_id)) = PlayerInput::default();
    info!("联机：客户端{reason} —— 玩家 {}（{addr}）", conn.player_id);
}

/// 每个固定步广播权威快照（`05` §5.15.3）。
pub fn broadcast_snapshot(
    mut host: Option<ResMut<HostNet>>,
    loaded: Option<Res<LoadedLevel>>,
    progress: Option<Res<LevelProgress>>,
    picked: Option<Res<PickedZones>>,
    active: Option<Res<ActiveCheckpoint>>,
    players: Query<(&PlayerId, &PlayerMotion, &PlayerFacing)>,
) {
    let Some(host) = host.as_mut() else {
        return;
    };
    if host.clients.is_empty() {
        return;
    }
    let Some(loaded) = loaded else {
        return;
    };

    let mut snaps: Vec<PlayerSnap> = players
        .iter()
        .map(|(id, motion, facing)| PlayerSnap {
            id: id.0,
            x: motion.0.aabb.center().x,
            y: motion.0.aabb.center().y,
            vx: motion.0.vel.x,
            vy: motion.0.vel.y,
            grounded: motion.0.grounded,
            facing_right: facing.0,
        })
        .collect();
    snaps.sort_by_key(|player| player.id);

    let (cleared, fruits) = progress
        .as_ref()
        .map(|progress| {
            let cleared = progress
                .rooms
                .iter()
                .enumerate()
                .filter(|(_, room)| room.cleared)
                .map(|(index, _)| index as u32)
                .collect();
            let fruits = progress
                .rooms
                .iter()
                .enumerate()
                .filter(|(_, room)| room.collected_fruit > 0)
                .map(|(index, room)| (index as u32, room.collected_fruit))
                .collect();
            (cleared, fruits)
        })
        .unwrap_or_default();
    let picked_zones = picked
        .as_ref()
        .map(|picked| picked.0.iter().copied().collect())
        .unwrap_or_default();
    let checkpoint = active
        .as_ref()
        .and_then(|active| active.0)
        .map(|point| (point.col, point.row, point.at.0, point.at.1));

    let player_count = snaps.len();
    let message = NetMessage::Snapshot(Snapshot {
        seq: host.seq,
        players: snaps,
        cleared,
        fruits,
        picked: picked_zones,
        checkpoint,
    });
    let Some(bytes) = protocol::encode(&message) else {
        warn!("联机：快照超过单包上限，跳过本步");
        return;
    };

    let HostNet {
        socket,
        clients,
        seq,
        logged_snapshot,
    } = &mut **host;
    for conn in clients.iter() {
        if let Err(error) = socket.send_to(&bytes, conn.addr) {
            debug!("联机：向 {} 发送快照失败（{error}）", conn.addr);
        }
    }
    *seq = seq.wrapping_add(1);
    if !*logged_snapshot {
        *logged_snapshot = true;
        info!(
            "联机：开始广播快照（{player_count} 名玩家 → {} 个客户端，关卡 {}）",
            clients.len(),
            loaded.level.name
        );
    }
}
