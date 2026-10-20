//! 相机跟随。所有者文档：`docs/02-world-and-rooms.md` §2.2。
//!
//! 单机 / 主机 / 客户端：一个内层相机跟随**本进程的玩家**（单人 0、主机 0、客户端为主机分配的编号）。
//! 本地双人：两个内层相机各自跟随 P1/P2，画面左右分屏（`07` §7.2.3）。
//! 相机中心先钳制在关卡 AABB 内，再做指数平滑；取整仍由 `render::snap` 统一完成。

use bevy::prelude::*;
use level_format::ROOM_PX;

use crate::config::render::{VIEWPORT_H, VIEWPORT_W};
use crate::config::room::CAMERA_SMOOTH_TAU_SECS;
use crate::net::client::ClientNet;
use crate::player::{PlayerId, PlayerMotion};
use crate::render::pixel_camera::{MAX_VIEWS, PlayerView, ViewLayout};
use crate::state::{GameSet, GameState, SessionMode};
use crate::world::load::LoadedLevel;

/// 每个内层相机的平滑状态（`None` = 首帧，直接落到目标）。
#[derive(Component, Debug, Default)]
pub struct CameraRig {
    pub last: Option<Vec2>,
}

/// 每个视图跟随哪个玩家。索引 = `PlayerView.0`。
///
/// 客户端在收到 `Welcome` 之前不知道自己的编号，此时相机不动（等首帧快照）。
pub fn view_players(mode: &SessionMode, client_id: Option<u8>) -> [Option<PlayerId>; MAX_VIEWS] {
    let mut targets = [None; MAX_VIEWS];
    match mode {
        SessionMode::LocalCoop => {
            targets[0] = Some(PlayerId(0));
            targets[1] = Some(PlayerId(1));
        }
        SessionMode::Client { .. } => {
            targets[0] = client_id.map(PlayerId);
        }
        SessionMode::Solo | SessionMode::Host => {
            targets[0] = Some(PlayerId(0));
        }
    }
    targets
}

pub struct CameraRigPlugin;

impl Plugin for CameraRigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), reset_camera_rigs)
            .add_systems(
                Update,
                drive_cameras
                    .in_set(GameSet::Camera)
                    .run_if(resource_exists::<LoadedLevel>),
            );
    }
}

/// 新一局 / 从暂停恢复：丢掉平滑历史，相机首帧直接对准目标（不跨地图滑行）。
fn reset_camera_rigs(mut rigs: Query<&mut CameraRig>) {
    for mut rig in &mut rigs {
        rig.last = None;
    }
}

/// 解一时步的指数平滑（帧率无关）。`tau <= 0` 或 `dt <= 0` 时直接返回目标。
///
/// 只做插值、不做钳制：调用方先钳制目标，两个端点都在合法区间内，插值结果必然也在区间内。
pub fn smooth_towards(current: Vec2, target: Vec2, dt: f32, tau: f32) -> Vec2 {
    if dt <= 0.0 || tau <= 0.0 {
        return target;
    }
    let alpha = 1.0 - (-dt / tau).exp();
    current + (target - current) * alpha.clamp(0.0, 1.0)
}

/// 相机目标：指定玩家 AABB 中心，钳制到「视野完全落在地图内」的合法区间。
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

/// 每个激活视图跟着自己的玩家；平滑后再由 `render::snap` 取整。
fn drive_cameras(
    loaded: Res<LoadedLevel>,
    layout: Res<ViewLayout>,
    mode: Res<SessionMode>,
    client: Option<Res<ClientNet>>,
    players: Query<(&PlayerId, &PlayerMotion)>,
    mut cameras: Query<(&PlayerView, &mut CameraRig, &mut Transform)>,
    time: Res<Time>,
) {
    let targets = view_players(&mode, client.and_then(|client| client.player_id));
    let dt = time.delta_secs();

    for (view, mut rig, mut transform) in &mut cameras {
        if view.0 as usize >= layout.views as usize {
            continue;
        }
        let Some(player_id) = targets[view.0 as usize] else {
            continue;
        };
        let Some((_, motion)) = players.iter().find(|(id, _)| **id == player_id) else {
            continue;
        };
        let target = follow_target(
            motion.0.aabb.center(),
            loaded.level.grid.cols,
            loaded.level.grid.rows,
        );
        let next = match rig.last {
            None => target,
            // 跨房间重生/传送：不做长距离滑行（否则会横穿整张地图，也会让自检误判）。
            Some(current) if current.distance(target) > VIEWPORT_W as f32 => target,
            Some(current) => smooth_towards(current, target, dt, CAMERA_SMOOTH_TAU_SECS),
        };
        rig.last = Some(next);
        transform.translation.x = next.x;
        transform.translation.y = next.y;
    }
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

    /// 指数平滑：单调逼近、不越过目标；`dt = 0` 保持不动。
    #[test]
    fn smoothing_converges_without_overshoot() {
        let target = Vec2::new(100.0, -40.0);
        let mut current = Vec2::ZERO;
        let mut previous = f32::INFINITY;
        for _ in 0..600 {
            current = smooth_towards(current, target, 1.0 / 60.0, 0.10);
            let distance = (target - current).length();
            assert!(distance <= previous.max(f32::EPSILON));
            assert!(current.x <= target.x && current.y >= target.y);
            previous = distance;
        }
        assert!((target - current).length() < 0.01);
        assert_eq!(smooth_towards(current, target, 0.0, 0.10), target);
    }

    /// 视角映射：本地双人两路；客户端用主机分配的编号；主机/单人只有 0。
    #[test]
    fn view_targets_follow_session_mode() {
        let solo = view_players(&SessionMode::Solo, None);
        assert_eq!(solo[0], Some(PlayerId(0)));
        assert_eq!(solo[1], None);

        let coop = view_players(&SessionMode::LocalCoop, None);
        assert_eq!(coop[0], Some(PlayerId(0)));
        assert_eq!(coop[1], Some(PlayerId(1)));

        let client = view_players(
            &SessionMode::Client {
                server: "127.0.0.1:42345".parse().unwrap(),
            },
            Some(2),
        );
        assert_eq!(client[0], Some(PlayerId(2)));
        assert_eq!(client[1], None);

        let host = view_players(&SessionMode::Host, None);
        assert_eq!(host[0], Some(PlayerId(0)));
        assert_eq!(host[1], None);
    }
}
