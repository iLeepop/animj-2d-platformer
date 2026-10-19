//! 运行时常量的唯一代码落点。**常量归属**见 `docs/00-conventions.md` §0.2：
//! 一个常量只有一个所有者文档，本文只放「核心/时间步」这类跨主题常量。

pub mod net;
pub mod physics;
pub mod render;
pub mod room;

/// 固定物理步长频率（Hz）。见 `docs/00-conventions.md` §0.1 **D11**。
///
/// 覆盖 Bevy 默认的 64 Hz：64 Hz 与显示器刷新率会产生病态交替（一帧跑两次 / 零次）。
pub const FIXED_HZ: f64 = 60.0;
