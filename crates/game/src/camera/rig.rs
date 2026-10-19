//! 相机跟随。所有者文档：`docs/02-world-and-rooms.md` §2.2。
//!
//! 内层视口小于一个房间。相机中心 = **全部本地/远程玩家 AABB 中心的中点**，并钳制在
//! 关卡 AABB 内，因此视野永远不会越过地图边界（`02` §2.2）。
//! 房间判定与相机解耦（见 `room::track_current_room`）。

use bevy::prelude::*;
use level_format::ROOM_PX;

use crate::config::render::{VIEWPORT_H, VIEWPORT_W};
use crate::player::PlayerMotion;
use crate::render::pixel_camera::InGameCamera;
use crate::state::GameSet;
use crate::world::load::LoadedLevel;

pub struct CameraRigPlugin;

impl Plugin for CameraRigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            drive_camera
                .in_set(GameSet::Camera)
                .run_if(resource_exists::<LoadedLevel>),
        );
    }
}

/// 相机目标：玩家 AABB 中心的中点，钳制到「视野完全落在地图内」的合法区间。
///
/// 关卡尺寸 = `grid × ROOM_PX`；内层视野 = `VIEWPORT_W × VIEWPORT_H`（小于一个房间，`07` §7.1.1）。
/// 单轴合法区间是 `[view/2, extent - view/2]`；当地图比视野还小时，相机钉在地图中心。
pub fn follow_target(player_center: Vec2, grid_cols: u32, grid_rows: u32) -> Vec2 {
    Vec2::new(
        clamp_axis(
            player_center.x,
            grid_cols as f32 * ROOM_PX as f32,
            VIEWPORT_W as f32,
        ),
        clamp_axis(
            player_center.y,
            grid_rows as f32 * ROOM_PX as f32,
            VIEWPORT_H as f32,
        ),
    )
}

fn clamp_axis(value: f32, extent: f32, view: f32) -> f32 {
    let half = view / 2.0;
    if extent <= view {
        extent / 2.0
    } else {
        value.clamp(half, extent - half)
    }
}

/// 每帧把相机放到跟随目标上；取整由 `render::snap` 统一完成（`07` §7.6）。
fn drive_camera(
    loaded: Res<LoadedLevel>,
    players: Query<&PlayerMotion>,
    camera: Option<Single<&mut Transform, With<InGameCamera>>>,
) {
    let Some(mut camera) = camera else {
        return;
    };
    let mut count = 0.0;
    let mut sum = Vec2::ZERO;
    for motion in &players {
        sum += motion.0.aabb.center();
        count += 1.0;
    }
    if count == 0.0 {
        return;
    }
    let target = follow_target(sum / count, loaded.level.grid.cols, loaded.level.grid.rows);
    camera.translation.x = target.x;
    camera.translation.y = target.y;
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;
    use level_format::ROOM_PX;

    use super::*;

    const GRID: (u32, u32) = (10, 3);

    /// 地图内部：相机中心精确等于玩家中心。
    #[test]
    fn follows_player_inside_map() {
        let player = Vec2::new(1500.0, 700.0);
        assert_eq!(follow_target(player, GRID.0, GRID.1), player);
    }

    /// 四条边：相机中心被钳制在半视野处，视野恰好贴住地图边界。
    #[test]
    fn clamps_at_all_four_edges() {
        let half = VIEWPORT_W as f32 / 2.0;
        let clamp_x_lo = Vec2::new(0.0, 700.0);
        let clamp_x_hi = Vec2::new(GRID.0 as f32 * ROOM_PX as f32, 700.0);
        let clamp_y_lo = Vec2::new(1500.0, 0.0);
        let clamp_y_hi = Vec2::new(1500.0, GRID.1 as f32 * ROOM_PX as f32);

        assert_eq!(
            follow_target(clamp_x_lo, GRID.0, GRID.1),
            Vec2::new(half, 700.0)
        );
        assert_eq!(
            follow_target(clamp_x_hi, GRID.0, GRID.1),
            Vec2::new(GRID.0 as f32 * ROOM_PX as f32 - half, 700.0)
        );
        assert_eq!(
            follow_target(clamp_y_lo, GRID.0, GRID.1),
            Vec2::new(1500.0, half)
        );
        assert_eq!(
            follow_target(clamp_y_hi, GRID.0, GRID.1),
            Vec2::new(1500.0, GRID.1 as f32 * ROOM_PX as f32 - half)
        );
    }

    /// 视野（边长 `VIEWPORT_W`）在任何玩家位置下都完整落在地图内。
    #[test]
    fn view_never_leaves_map() {
        let half = VIEWPORT_W as f32 / 2.0;
        let width = GRID.0 as f32 * ROOM_PX as f32;
        let height = GRID.1 as f32 * ROOM_PX as f32;
        for x in [
            -100.0,
            0.0,
            1.0,
            159.0,
            160.0,
            1000.0,
            width - 1.0,
            width,
            width + 100.0,
        ] {
            for y in [
                -100.0,
                0.0,
                1.0,
                159.0,
                160.0,
                700.0,
                height - 1.0,
                height,
                height + 100.0,
            ] {
                let camera = follow_target(Vec2::new(x, y), GRID.0, GRID.1);
                assert!(camera.x - half >= 0.0 && camera.x + half <= width, "x={x}");
                assert!(camera.y - half >= 0.0 && camera.y + half <= height, "y={y}");
            }
        }
    }

    /// 多玩家：相机目标取所有玩家 AABB 中心的中点（`02` §2.2）。
    #[test]
    fn multiplayer_camera_uses_the_midpoint() {
        let a = Vec2::new(1000.0, 700.0);
        let b = Vec2::new(1100.0, 760.0);
        let midpoint = (a + b) / 2.0;
        assert_eq!(follow_target(midpoint, GRID.0, GRID.1), midpoint);
        // 中点不会被单个玩家拽走。
        assert_ne!(follow_target(midpoint, GRID.0, GRID.1), a);
    }

    /// 单房间地图（房间大于视野）：相机可在房间内跟随，但不会让视野越界。
    #[test]
    fn single_room_map_clamps_at_view_half() {
        let half = VIEWPORT_W as f32 / 2.0;
        let extent = ROOM_PX as f32;
        assert_eq!(follow_target(Vec2::ZERO, 1, 1), Vec2::splat(half));
        assert_eq!(
            follow_target(Vec2::splat(extent / 2.0), 1, 1),
            Vec2::splat(extent / 2.0)
        );
        assert_eq!(
            follow_target(Vec2::splat(extent), 1, 1),
            Vec2::splat(extent - half)
        );
    }
}
