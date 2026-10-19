//! 局域网房间搜索（`05` §5.15.4）。
//!
//! 协议：浏览器向 `255.255.255.255:NET_PORT` 与回环地址广播 `Discover{token}`；
//! 主机在同一端口收包并单播 `RoomInfo`。没有中心服务器，也不需要端口复用。
//! 用「查询/响应」而不是主机周期广播：同一台机器上同时跑主机与客户端时不会互相抢端口。

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bevy::prelude::*;

use super::protocol::{self, NetMessage};
use crate::config::net::{
    DISCOVERY_INTERVAL_SECS, MAX_PACKET_BYTES, NET_PORT, PROTOCOL_VERSION, ROOM_TTL_SECS,
};

/// 一个被发现的房间（UI 渲染用；带最后一次响应时间）。
#[derive(Debug, Clone)]
pub struct DiscoveredRoom {
    pub name: String,
    pub level_id: String,
    pub players: u8,
    pub max_players: u8,
    pub last_seen: Instant,
}

/// 房间搜索器（只在 `OnlineMenu` 存在）。
#[derive(Resource)]
pub struct LanBrowser {
    pub socket: std::net::UdpSocket,
    pub rooms: BTreeMap<SocketAddr, DiscoveredRoom>,
    /// 每次房间集合变化 +1；UI 用它决定是否重建列表（`Res::is_changed` 会被每帧刷新污染）。
    pub revision: u64,
    token: u64,
    last_query: Instant,
}

impl LanBrowser {
    pub fn bind() -> std::io::Result<Self> {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        // 广播在部分 Windows 网络栈上需要显式开启；失败只影响跨机器发现，回环仍然可用。
        let _ = socket.set_broadcast(true);
        Ok(Self {
            socket,
            rooms: BTreeMap::new(),
            revision: 0,
            token: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(1),
            last_query: Instant::now() - Duration::from_secs(60),
        })
    }

    /// 过期清理（`ROOM_TTL_SECS`）：主机停止响应后从列表消失。
    fn expire(&mut self, now: Instant) {
        let before = self.rooms.len();
        self.rooms
            .retain(|_, room| now.duration_since(room.last_seen).as_secs_f32() <= ROOM_TTL_SECS);
        if self.rooms.len() != before {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn query(&mut self, now: Instant) {
        if now.duration_since(self.last_query).as_secs_f32() < DISCOVERY_INTERVAL_SECS {
            return;
        }
        self.last_query = now;
        let message = protocol::NetMessage::Discover { token: self.token };
        let Some(bytes) = protocol::encode(&message) else {
            return;
        };
        // 广播 + 回环：跨机器和本机双开都能发现。
        let _ = self
            .socket
            .send_to(&bytes, SocketAddr::from(([255, 255, 255, 255], NET_PORT)));
        let _ = self
            .socket
            .send_to(&bytes, SocketAddr::from(([127, 0, 0, 1], NET_PORT)));
    }

    fn drain(&mut self, now: Instant) {
        let mut buffer = [0u8; MAX_PACKET_BYTES];
        while let Ok((len, addr)) = self.socket.recv_from(&mut buffer) {
            let Some(NetMessage::RoomInfo {
                token,
                protocol: version,
                name,
                level_id,
                players,
                max_players,
                ..
            }) = protocol::decode(&buffer[..len])
            else {
                continue;
            };
            if token != self.token || version != PROTOCOL_VERSION {
                continue;
            }
            let is_new = !self.rooms.contains_key(&addr);
            self.rooms.insert(
                addr,
                DiscoveredRoom {
                    name: name.clone(),
                    level_id: level_id.clone(),
                    players,
                    max_players,
                    last_seen: now,
                },
            );
            if is_new {
                info!("发现房间：{name}（{level_id}，{players}/{max_players}）@ {addr}");
            }
            // 玩家数等字段可能变化：每次响应都刷新列表。
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

pub fn start_browser(mut commands: Commands) {
    match LanBrowser::bind() {
        Ok(browser) => {
            info!("联机：开始搜索局域网房间（UDP {NET_PORT}）");
            commands.insert_resource(browser);
        }
        Err(error) => {
            warn!("联机：无法绑定发现端口（{error}）");
        }
    }
}

pub fn stop_browser(mut commands: Commands) {
    commands.remove_resource::<LanBrowser>();
}

pub fn refresh_browser(mut browser: Option<ResMut<LanBrowser>>) {
    let Some(browser) = browser.as_mut() else {
        return;
    };
    let now = Instant::now();
    browser.query(now);
    browser.drain(now);
    browser.expire(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机回环上的「发现 → 响应」链路：证明查询包与 `RoomInfo` 解析能互通。
    #[test]
    fn discovery_round_trip_over_loopback() {
        let host = std::net::UdpSocket::bind("127.0.0.1:0").expect("host bind");
        host.set_nonblocking(true).unwrap();
        let browser = LanBrowser::bind().expect("browser bind");
        browser
            .socket
            .send_to(
                &protocol::encode(&NetMessage::Discover { token: 7 }).unwrap(),
                host.local_addr().unwrap(),
            )
            .unwrap();

        let mut buffer = [0u8; MAX_PACKET_BYTES];
        let (len, from) = host.recv_from(&mut buffer).expect("host receives query");
        assert_eq!(
            protocol::decode(&buffer[..len]),
            Some(NetMessage::Discover { token: 7 })
        );

        host.send_to(
            &protocol::encode(&NetMessage::RoomInfo {
                token: 7,
                protocol: PROTOCOL_VERSION,
                name: "test".into(),
                level_id: "world_01".into(),
                players: 1,
                max_players: 4,
                port: NET_PORT,
            })
            .unwrap(),
            from,
        )
        .unwrap();

        let (len, _) = browser
            .socket
            .recv_from(&mut buffer)
            .expect("browser receives reply");
        match protocol::decode(&buffer[..len]) {
            Some(NetMessage::RoomInfo { name, token, .. }) => {
                assert_eq!(name, "test");
                assert_eq!(token, 7);
            }
            other => panic!("expected RoomInfo, got {other:?}"),
        }
    }
}
