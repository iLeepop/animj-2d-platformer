//! 房间与相机的逻辑常量。**所有者文档：`docs/02-world-and-rooms.md`**（§2.2 常量表）。

use bevy::math::Vec2;
use level_format::{ROOM_PX, TILE_PX};

use crate::config::physics::PLAYER_SIZE;

/// 房间默认重生点（**房间局部坐标**，原点 = 房间左上角、+Y 向下，见 `00` §0.3）。
///
/// 是玩家 **AABB 的中心**，脚底恰好贴合房间底行地板的顶面（`local.y = ROOM_PX - TILE_PX = 480`）：
/// `480 - player_height / 2`。见 `02` §2.5。
///
/// ⚠️ **AABB 中心 ≠ 脚底**。曾把 `480`（地板顶面）当作中心：身体会有 14 px 埋进地板，
/// 进而把碰撞盒压扁/压反（`06` §6.4.9 的尺寸不变量）。
pub fn default_spawn_local(player_height: f32) -> Vec2 {
    Vec2::new(
        ROOM_PX as f32 / 2.0,
        (ROOM_PX - TILE_PX) as f32 - player_height / 2.0,
    )
}

/// 使用默认玩家高度（`PLAYER_SIZE.y`）的默认重生点。见 `02` §2.5。
pub const DEFAULT_SPAWN_LOCAL: Vec2 = Vec2::new(
    ROOM_PX as f32 / 2.0,
    (ROOM_PX - TILE_PX) as f32 - PLAYER_SIZE.y / 2.0,
);

/// 房间切换滞回（px）：玩家 AABB 中心必须**深入**新房间超过该距离才承认切换。
/// 只影响 `CurrentRoom`（HUD/目标判定），不影响相机跟随。见 `02` §2.2。
pub const ROOM_SWITCH_HYSTERESIS: f32 = 8.0;

/// 死亡表现时长（固定步，60 Hz ⇒ 0.50 s）：到期才把玩家放回存档点/出生点。见 `02` §2.5。
///
/// 用**固定步数**而不是秒，保证死亡延迟与物理、暂停状态一致且可无头测试。
pub const DEATH_DELAY_STEPS: u32 = 30;

/// 死亡闪烁的一个相位时长（秒）。见 `02` §2.5。
///
/// 0.1 s ⇒ 0.5 s 内约 2 次完整明暗交替，低于 3 次/秒的光敏性上限。
pub const DEATH_BLINK_SECS: f32 = 0.1;

/// 死亡闪烁「暗」相位的 alpha（保留角色轮廓与多人分色）。见 `02` §2.5。
pub const DEATH_FLASH_ALPHA: f32 = 0.2;
