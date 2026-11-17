//! 联机常量。**所有者文档：`docs/05-architecture.md` §5.15**。
//!
//! 联机层只用 `std::net::UdpSocket`（`09` §9.0 R1：不引入新的网络 crate）。

/// 协议版本。客户端与主机不一致时拒绝加入（`05` §5.15.2）。
///
/// v6：`PlayerSnap` 增加道具 buff 剩余时间。
pub const PROTOCOL_VERSION: u16 = 7;

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

/// 客户端渲染插值的延迟（固定步数）：渲染时刻 = 最新快照 `seq` − 该值（`05` §5.15.3）。
///
/// 至少 1 步才能在两帧快照之间插值；取 2 步多留一帧余量——单个快照丢失时，
/// 渲染时刻仍落在两个已收到快照之间，不会「冻结一帧再跳」。
pub const INTERP_DELAY_STEPS: u32 = 2;

/// 渲染时钟与最新快照的最大允许滞后（固定步数）：超过即重同步。
///
/// 用于客户端暂停（`Paused` 期间不推进渲染时钟，但仍在收包）或系统卡顿后
/// 重新对齐，避免恢复后长时间「追赶」。
pub const INTERP_MAX_LAG_STEPS: u32 = 8;

/// 每个输入包冗余携带的最近未确认输入数（`05` §5.15.2）。
///
/// 单个 UDP 包丢失时，后续包会重发这些输入；主机按 `seq` 去重后逐固定步消费。
pub const INPUT_REDUNDANCY: usize = 4;

/// 客户端未确认输入的最大保留数（约 4 秒 @ 60 Hz）。
///
/// 主机长时间不 ack（卡顿/断线）时丢弃最旧的输入，防止队列无限增长；
/// 丢弃后本地预测会与主机短暂不一致，下一份快照负责纠正。
pub const MAX_PENDING_INPUTS: usize = 240;

/// 客户端等待主机接受加入的总时长（秒）。
pub const CONNECT_TIMEOUT_SECS: f32 = 10.0;

/// 本地双人出生时相邻玩家的水平间距（px）：一个玩家盒宽 + 4 px。
/// 第一个可行位置被墙占用时按 ±32 px 继续搜索（`02` §2.5）。
pub const PLAYER_SPAWN_GAP_PX: f32 = 24.0;

/// 多人出生点的左右搜索次数（每侧）：找不到空位时退化为按编号偏移。
pub const SPAWN_PLACEMENT_TRIES: usize = 8;

/// 联机房间的默认显示名（`05` §5.15.4）。
pub const DEFAULT_ROOM_NAME: &str = "animj LAN";
