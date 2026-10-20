//! 联机协议（`05` §5.15.2）。
//!
//! 传输是**非阻塞 UDP**，零新依赖（`00` D15）：
//! - 发现/加入/输入等低频消息用 RON（与关卡/存档同一套 `serde`）；
//! - 每固定步的 [`Snapshot`] 用**定长小端二进制**（RON 的浮点与数组开销会超过 MTU）。
//!
//! 包必须小于 [`MAX_PACKET_BYTES`]，否则接收端直接丢弃（避免 IP 分片）。

use serde::{Deserialize, Serialize};

use crate::config::net::{MAX_PACKET_BYTES, PROTOCOL_VERSION, SNAPSHOT_MAX_ROOMS};

/// 快照的二进制标签（其余消息是 `RON` 标签 + 文本）。
const TAG_SNAPSHOT: u8 = 0x01;
const TAG_RON: u8 = 0x02;

/// 一个玩家在快照里的状态（`05` §5.15.3）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerSnap {
    pub id: u8,
    /// AABB 中心（世界像素）。
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    pub facing_right: bool,
}

/// 主机 → 客户端的进度快照（`05` §5.15.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub seq: u32,
    pub players: Vec<PlayerSnap>,
    /// 已通关房间的行主序下标（关卡上限 64 房间，`01` §1.8 ⇒ 位图）。
    pub cleared: Vec<u32>,
    /// `(房间下标, 已收集果实数)`，只包含非零项。
    pub fruits: Vec<(u32, u32)>,
    /// 已拾取并销毁的房间实体编号（`room::ZoneId`）。
    pub picked: Vec<u32>,
    /// 已激活存档点：`(col, row, at.x, at.y)`。
    pub checkpoint: Option<(u32, u32, i32, i32)>,
}

/// 局域网消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NetMessage {
    /// 客户端广播查询；`token` 用于丢弃过期响应。
    Discover { token: u64 },
    /// 主机响应发现请求。
    RoomInfo {
        token: u64,
        protocol: u16,
        name: String,
        level_id: String,
        players: u8,
        max_players: u8,
        port: u16,
    },
    /// 客户端请求加入。`client_id` 在同一安装内持久，主机用它识别「重连」而不是新玩家。
    Join {
        name: String,
        level_id: String,
        client_id: u64,
    },
    /// 主机接受加入并分配玩家编号。
    Welcome {
        player_id: u8,
        level_id: String,
        grid: (u32, u32),
    },
    /// 主机拒绝加入（房间满 / 关卡不符 / 版本不符）。
    Reject { reason: String },
    /// 客户端上行输入（每个固定步一条）。
    Input {
        seq: u32,
        move_x: i8,
        jump_pressed: bool,
        jump_held: bool,
        interact_pressed: bool,
    },
    /// 主机下行权威快照（每个固定步一条）。
    Snapshot(Snapshot),
    /// 客户端主动离开。
    Bye,
}

/// 编码；超过 MTU 上限返回 `None`（调用方跳过该包并 warn）。
pub fn encode(message: &NetMessage) -> Option<Vec<u8>> {
    let bytes = match message {
        NetMessage::Snapshot(snapshot) => encode_snapshot(snapshot)?,
        other => {
            let text = ron::to_string(other).ok()?;
            let mut bytes = Vec::with_capacity(text.len() + 1);
            bytes.push(TAG_RON);
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
    };
    (bytes.len() <= MAX_PACKET_BYTES).then_some(bytes)
}

/// 解码；非法/超长包返回 `None`（不 panic）。
pub fn decode(bytes: &[u8]) -> Option<NetMessage> {
    if bytes.is_empty() || bytes.len() > MAX_PACKET_BYTES {
        return None;
    }
    match bytes[0] {
        TAG_SNAPSHOT => decode_snapshot(&bytes[1..]).map(NetMessage::Snapshot),
        TAG_RON => ron::from_str(std::str::from_utf8(&bytes[1..]).ok()?).ok(),
        _ => None,
    }
}

// ---------- 快照二进制 ----------
//
// 布局（全小端）：
//   seq u32 | players u8 | [id u8, x/y/vx/vy f32, flags u8] × n
//   cleared u64（房间下标位图）
//   fruits u8 | [(room u8, count u16)] × n
//   picked u16 | [zone u16] × n
//   checkpoint u8（0/1）| [col u16, row u16, x i16, y i16]
//
// 房间最大 64（`01` §1.8），果实数量 u16，实体编号 u16；越界值夹取而不是 panic。

fn encode_snapshot(snapshot: &Snapshot) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(256);
    out.push(TAG_SNAPSHOT);
    out.extend_from_slice(&snapshot.seq.to_le_bytes());

    let players = snapshot.players.len().min(u8::MAX as usize);
    out.push(players as u8);
    for player in snapshot.players.iter().take(players) {
        out.push(player.id);
        for value in [player.x, player.y, player.vx, player.vy] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        let flags = u8::from(player.grounded) | (u8::from(player.facing_right) << 1);
        out.push(flags);
    }

    let mut cleared = 0u64;
    for room in &snapshot.cleared {
        if *room < SNAPSHOT_MAX_ROOMS as u32 {
            cleared |= 1u64 << *room;
        }
    }
    out.extend_from_slice(&cleared.to_le_bytes());

    let fruits = snapshot
        .fruits
        .iter()
        .take(u8::MAX as usize)
        .collect::<Vec<_>>();
    out.push(fruits.len() as u8);
    for (room, count) in fruits {
        out.push((*room).min(u8::MAX as u32) as u8);
        out.extend_from_slice(&((*count).min(u16::MAX as u32) as u16).to_le_bytes());
    }

    let picked = snapshot
        .picked
        .iter()
        .take(u16::MAX as usize)
        .collect::<Vec<_>>();
    out.extend_from_slice(&(picked.len() as u16).to_le_bytes());
    for zone in picked {
        out.extend_from_slice(&((*zone).min(u16::MAX as u32) as u16).to_le_bytes());
    }

    match snapshot.checkpoint {
        Some((col, row, x, y)) => {
            out.push(1);
            out.extend_from_slice(&(col.min(u16::MAX as u32) as u16).to_le_bytes());
            out.extend_from_slice(&(row.min(u16::MAX as u32) as u16).to_le_bytes());
            out.extend_from_slice(
                &(x.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes(),
            );
            out.extend_from_slice(
                &(y.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes(),
            );
        }
        None => out.push(0),
    }
    Some(out)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.bytes.get(self.at..self.at + 2)?;
        self.at += 2;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.bytes.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Option<u64> {
        let bytes = self.bytes.get(self.at..self.at + 8)?;
        self.at += 8;
        Some(u64::from_le_bytes(bytes.try_into().ok()?))
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|value| value as i16)
    }

    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
}

fn decode_snapshot(bytes: &[u8]) -> Option<Snapshot> {
    let mut reader = Reader::new(bytes);
    let seq = reader.u32()?;
    let player_count = reader.u8()? as usize;
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        let id = reader.u8()?;
        let x = reader.f32()?;
        let y = reader.f32()?;
        let vx = reader.f32()?;
        let vy = reader.f32()?;
        let flags = reader.u8()?;
        players.push(PlayerSnap {
            id,
            x,
            y,
            vx,
            vy,
            grounded: flags & 1 != 0,
            facing_right: flags & 2 != 0,
        });
    }
    let cleared_bits = reader.u64()?;
    let cleared = (0..SNAPSHOT_MAX_ROOMS as u32)
        .filter(|room| cleared_bits & (1u64 << room) != 0)
        .collect();

    let fruit_count = reader.u8()? as usize;
    let mut fruits = Vec::with_capacity(fruit_count);
    for _ in 0..fruit_count {
        let room = reader.u8()? as u32;
        let count = reader.u16()? as u32;
        fruits.push((room, count));
    }

    let picked_count = reader.u16()? as usize;
    let mut picked = Vec::with_capacity(picked_count);
    for _ in 0..picked_count {
        picked.push(reader.u16()? as u32);
    }

    let checkpoint = match reader.u8()? {
        0 => None,
        _ => Some((
            reader.u16()? as u32,
            reader.u16()? as u32,
            reader.i16()? as i32,
            reader.i16()? as i32,
        )),
    };
    Some(Snapshot {
        seq,
        players,
        cleared,
        fruits,
        picked,
        checkpoint,
    })
}

/// 本进程的协议版本（编译期常量，便于日志与测试读取）。
pub const VERSION: u16 = PROTOCOL_VERSION;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::net::MAX_PLAYERS;

    fn snapshot() -> NetMessage {
        NetMessage::Snapshot(Snapshot {
            seq: 7,
            players: vec![PlayerSnap {
                id: 0,
                x: 100.5,
                y: 200.0,
                vx: 1.0,
                vy: -2.0,
                grounded: true,
                facing_right: false,
            }],
            cleared: vec![0, 3],
            fruits: vec![(1, 2)],
            picked: vec![4, 5],
            checkpoint: Some((0, 0, 96, 464)),
        })
    }

    /// 往返编码必须逐字段相等。
    #[test]
    fn messages_round_trip() {
        let messages = [
            NetMessage::Discover { token: 42 },
            NetMessage::RoomInfo {
                token: 42,
                protocol: VERSION,
                name: "room".into(),
                level_id: "world_01".into(),
                players: 2,
                max_players: MAX_PLAYERS as u8,
                port: 42345,
            },
            NetMessage::Join {
                name: "player".into(),
                level_id: "world_01".into(),
                client_id: 0x1234_5678_9abc_def0,
            },
            NetMessage::Welcome {
                player_id: 1,
                level_id: "world_01".into(),
                grid: (10, 3),
            },
            NetMessage::Reject {
                reason: "full".into(),
            },
            NetMessage::Input {
                seq: 9,
                move_x: -1,
                jump_pressed: true,
                jump_held: false,
                interact_pressed: true,
            },
            snapshot(),
            NetMessage::Bye,
        ];
        for message in messages {
            let bytes = encode(&message).expect("应能编码");
            assert_eq!(decode(&bytes), Some(message));
        }
    }

    /// 超长/空/非 UTF-8/未知标签包必须被拒绝，不得 panic。
    #[test]
    fn malformed_packets_are_rejected() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[TAG_RON, 0xff, 0xfe]), None);
        assert_eq!(decode(&[0x7f, 1, 2, 3]), None);
        let too_long = vec![b'x'; MAX_PACKET_BYTES + 1];
        assert_eq!(decode(&too_long), None);
        // 快照被截断
        let bytes = encode(&snapshot()).unwrap();
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
    }

    /// 完整快照（4 玩家 + 64 房间 + 大量已拾取实体）必须留在单包上限内。
    #[test]
    fn full_snapshot_fits_in_one_packet() {
        let message = NetMessage::Snapshot(Snapshot {
            seq: u32::MAX,
            players: (0..MAX_PLAYERS)
                .map(|id| PlayerSnap {
                    id: id as u8,
                    x: 12345.678,
                    y: 12345.678,
                    vx: -200.0,
                    vy: 560.0,
                    grounded: true,
                    facing_right: true,
                })
                .collect(),
            cleared: (0..64).collect(),
            fruits: (0..64).map(|i| (i, 99)).collect(),
            picked: (0..256).collect(),
            checkpoint: Some((9, 2, -123, 4567)),
        });
        let bytes = encode(&message).expect("完整快照必须可编码");
        assert!(
            bytes.len() <= MAX_PACKET_BYTES,
            "快照 {} 字节超过单包上限",
            bytes.len()
        );
        assert_eq!(decode(&bytes), Some(message));
    }
}
