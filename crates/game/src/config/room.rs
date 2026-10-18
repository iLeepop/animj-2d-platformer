//! 房间与相机的逻辑常量。**所有者文档：`docs/02-world-and-rooms.md`**（§2.2 常量表）。

use bevy::math::Vec2;
use level_format::{ROOM_PX, TILE_PX};

use crate::config::physics::PLAYER_SIZE;

/// 房间默认重生点（**房间局部坐标**，原点 = 房间左上角、+Y 向下，见 `00` §0.3）。
///
/// 是玩家 **AABB 的中心**，脚底恰好贴合房间底行地板的顶面（`local.y = ROOM_PX - TILE_PX = 480`）：
/// `480 - PLAYER_SIZE.y / 2 = 466`。见 `02` §2.5。
///
/// ⚠️ **AABB 中心 ≠ 脚底**。曾把 `480`（地板顶面）当作中心：身体会有 14 px 埋进地板，
/// 进而把碰撞盒压扁/压反（`06` §6.4.9 的尺寸不变量）。
pub const DEFAULT_SPAWN_LOCAL: Vec2 = Vec2::new(
    ROOM_PX as f32 / 2.0,
    (ROOM_PX - TILE_PX) as f32 - PLAYER_SIZE.y / 2.0,
);

/// 相机在房间落点之间的过渡时长（秒）。见 `02` §2.2。
pub const CAMERA_PAN_SECS: f32 = 0.25;

/// 房间切换滞回（px）：玩家 AABB 中心必须**深入**新房间超过该距离才承认切换。见 `02` §2.2。
pub const ROOM_SWITCH_HYSTERESIS: f32 = 8.0;

/// 相机过渡的缓动曲线（ease-out quad）。见 `02` §2.2 的 `CAMERA_PAN_EASING`。
pub fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}
