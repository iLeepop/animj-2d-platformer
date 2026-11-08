//! 道具生成器常量。所有者文档：`docs/01-game-design.md` §1.5。

/// 道具被拾取后，生成器重新生成道具的等待时间（秒）。
pub const ITEM_RESPAWN_SECS: f32 = 1.5;

/// 道具能力持续时间（秒）：超时后 buff 自动清除。
pub const ITEM_DURATION_SECS: f32 = 10.0;
