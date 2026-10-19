//! 联机常量。**所有者文档：`docs/05-architecture.md` §5.15**。
//!
//! 联机层只用 `std::net::UdpSocket`（`09` §9.0 R1：不引入新的网络 crate）。

/// 协议版本。客户端与主机不一致时拒绝加入（`05` §5.15.2）。
pub const PROTOCOL_VERSION: u16 = 1;

/// 局域网发现与游戏共用的 UDP 端口（`05` §5.15.2）。
pub const NET_PORT: u16 = 42345;

/// 单房间最大玩家数（含主机）。见 `06` §6.10。
pub const MAX_PLAYERS: usize = 4;

/// 发现查询间隔（秒）：`OnlineMenu` 每隔这么久广播一次 `Discover`。
pub const DISCOVERY_INTERVAL_SECS: f32 = 1.0;

/// 发现结果的存活时间（秒）：超过该时间没有再次被通告就从列表移除。
pub const ROOM_TTL_SECS: f32 = 3.5;

/// 客户端超时（秒）：主机超过该时间没收到某客户端的任何包就移除其玩家。
pub const CLIENT_TIMEOUT_SECS: f32 = 5.0;

/// 客户端重发 `Join` 的间隔（秒）。
pub const JOIN_RETRY_SECS: f32 = 0.25;

/// 主机判定远程输入「停更」的阈值（秒）：超过该时间没有 `Input` 包就把持续量清零
/// （客户端暂停/卡顿不应让角色一直走）。
pub const REMOTE_INPUT_STALE_SECS: f32 = 0.25;

/// 单包字节上限：留出以太网 MTU 余量，避免 IP 分片。
pub const MAX_PACKET_BYTES: usize = 1200;

/// 每次轮询最多处理的包数：防止恶意/突发流量把一帧卡死。
pub const MAX_PACKETS_PER_FRAME: usize = 64;

/// 客户端等待主机接受加入的总时长（秒）。
pub const CONNECT_TIMEOUT_SECS: f32 = 10.0;

/// 本地双人出生时相邻玩家的水平间距（px）：一个玩家盒宽 + 4 px。
/// 第一个可行位置被墙占用时按 ±32 px 继续搜索（`02` §2.5）。
pub const PLAYER_SPAWN_GAP_PX: f32 = 24.0;

/// 多人出生点的左右搜索次数（每侧）：找不到空位时退化为按编号偏移。
pub const SPAWN_PLACEMENT_TRIES: usize = 8;

/// 快照通关位图覆盖的房间上限（`u64`，与 `01` §1.8 的单关卡上限一致）。
pub const SNAPSHOT_MAX_ROOMS: usize = 64;

/// 联机房间的默认显示名（`05` §5.15.4）。
pub const DEFAULT_ROOM_NAME: &str = "animj LAN";
