//! 房间判定与相机落点。所有者文档：`docs/02-world-and-rooms.md` §2.2。
//!
//! 两个职责：
//! 1. **当前房间**：由玩家 AABB 中心决定，带 `ROOM_SWITCH_HYSTERESIS` 滞回（防止边界来回抖动）。
//! 2. **相机落点**：房间变化时从**当前实际位置**过渡到新房间中心（`CAMERA_PAN_SECS` + ease-out quad），
//!    过渡期间玩家照常操作，不排队、不叠加。

use bevy::prelude::*;

use crate::config::room::{CAMERA_PAN_SECS, ease_out_quad};
use crate::coords::room_center;
use crate::render::pixel_camera::InGameCamera;
use crate::room::RoomCoord;
use crate::state::GameSet;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;

/// 相机落点状态机（纯数据 + 纯方法，可脱离 Bevy 单测）。
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CameraRigState {
    pub from: Vec2,
    pub to: Vec2,
    pub elapsed: f32,
    pub panning: bool,
    pub target_room: Option<RoomCoord>,
}

impl CameraRigState {
    /// 初始状态：尚未锁定任何房间（首次落点**不做过渡**，直接吸附）。
    pub fn uninitialized() -> Self {
        Self {
            from: Vec2::ZERO,
            to: Vec2::ZERO,
            elapsed: 0.0,
            panning: false,
            target_room: None,
        }
    }

    /// 直接吸附到落点（关卡加载后的第一次定位，或需要硬切的场合）。
    pub fn snap_to(&mut self, pos: Vec2, room: RoomCoord) {
        self.from = pos;
        self.to = pos;
        self.elapsed = 0.0;
        self.panning = false;
        self.target_room = Some(room);
    }

    /// 开一次过渡：**从当前实际位置**出发（`02` §2.2：不排队、不叠加）。
    pub fn retarget(&mut self, current_pos: Vec2, to: Vec2, room: RoomCoord) {
        self.from = current_pos;
        self.to = to;
        self.elapsed = 0.0;
        self.panning = current_pos.distance_squared(to) > 0.0;
        self.target_room = Some(room);
    }

    pub fn is_idle(&self) -> bool {
        !self.panning
    }

    /// 推进过渡并返回本帧的相机位置。
    ///
    /// 空闲时必须返回**落点**而不是调用前的位置：`snap_to`/`retarget` 之后紧接的第一帧
    /// 必须立刻生效（否则首次吸附会被"记住旧位置"吞掉，见回归测试）。
    pub fn advance(&mut self, dt: f32) -> Vec2 {
        if !self.panning {
            return self.to;
        }
        self.elapsed = (self.elapsed + dt).min(CAMERA_PAN_SECS);
        let t = self.elapsed / CAMERA_PAN_SECS;
        let pos = self.from.lerp(self.to, ease_out_quad(t));
        if self.elapsed >= CAMERA_PAN_SECS {
            self.panning = false;
            return self.to;
        }
        pos
    }
}

impl Default for CameraRigState {
    fn default() -> Self {
        Self::uninitialized()
    }
}

pub struct CameraRigPlugin;

impl Plugin for CameraRigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRigState>()
            .add_systems(
                Update,
                drive_camera
                    .in_set(GameSet::Camera)
                    .run_if(resource_exists::<LoadedLevel>),
            );
    }
}

/// 房间变化时把相机过渡到新房间中心；过渡期间玩家照常操作（`02` §2.2）。
fn drive_camera(
    time: Res<Time>,
    loaded: Res<LoadedLevel>,
    current: Res<CurrentRoom>,
    mut rig: ResMut<CameraRigState>,
    camera: Option<Single<&mut Transform, With<InGameCamera>>>,
) {
    let Some(mut camera) = camera else {
        return;
    };
    let target_room = (current.col, current.row);
    let pos = Vec2::new(camera.translation.x, camera.translation.y);

    if rig.target_room != Some(target_room) {
        let to = room_center(target_room.0, target_room.1, loaded.level.grid.rows);
        if rig.target_room.is_none() {
            // 关卡加载后的第一次定位：直接吸附，避免"从地图角落飞进来"
            rig.snap_to(to, target_room);
            info!("相机锁定房间 ({}, {}) 的中心", target_room.0, target_room.1);
        } else {
            rig.retarget(pos, to, target_room);
            info!(
                "相机过渡：房间 ({}, {}) → ({:.0}, {:.0})，用时 {:.2}s",
                target_room.0, target_room.1, to.x, to.y, CAMERA_PAN_SECS
            );
        }
    }

    let next = rig.advance(time.delta_secs());
    camera.translation.x = next.x;
    camera.translation.y = next.y;
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::*;
    use crate::config::room::{CAMERA_PAN_SECS, ease_out_quad};

    const GRID: (u32, u32) = (10, 3);

    /// ease-out quad 的端点与单调性。
    #[test]
    fn easing_shape() {
        assert_eq!(ease_out_quad(0.0), 0.0);
        assert_eq!(ease_out_quad(1.0), 1.0);
        assert!(ease_out_quad(0.5) > 0.5, "ease-out 应该先快后慢");
        let mut previous = -1.0;
        for i in 0..=20 {
            let v = ease_out_quad(i as f32 / 20.0);
            assert!(v >= previous - 1e-6);
            previous = v;
        }
    }

    /// 过渡：从当前实际位置出发、单调接近目标、按时结束在目标上。
    #[test]
    fn pan_reaches_target_and_is_retargetable() {
        let mut rig = CameraRigState::uninitialized();
        let start = Vec2::new(100.0, 50.0);
        let target = Vec2::new(356.0, 50.0);
        rig.retarget(start, target, (1, 0));
        assert!(rig.panning);

        let mut pos = start;
        let steps = (CAMERA_PAN_SECS / (1.0 / 60.0)).ceil() as u32;
        let mut previous_distance = f32::MAX;
        for _ in 0..steps {
            let next = rig.advance(1.0 / 60.0);
            let distance = next.distance(target);
            assert!(distance <= previous_distance + 1e-3, "必须单调接近目标");
            previous_distance = distance;
            pos = next;
        }
        assert!(!rig.panning, "到时应结束过渡");
        assert_eq!(pos, target, "结束位置必须是目标（精确）");

        // 中途重定向：从当前实际位置出发，用时重新计满
        rig.retarget(start, target, (1, 0));
        for _ in 0..5 {
            pos = rig.advance(1.0 / 60.0);
        }
        let redirected = Vec2::new(0.0, 400.0);
        rig.retarget(pos, redirected, (0, 1));
        assert_eq!(rig.from, pos, "重定向必须从当前实际位置出发");
        assert_eq!(rig.elapsed, 0.0);
        let end = rig.advance(CAMERA_PAN_SECS);
        assert_eq!(end, redirected);
        assert!(!rig.panning);
    }

    /// 首次定位直接吸附，不产生"从原点飞过来"的过渡。
    #[test]
    fn first_lock_snaps_without_panning() {
        let mut rig = CameraRigState::uninitialized();
        let center = room_center(3, 1, GRID.1);
        rig.snap_to(center, (3, 1));
        assert!(rig.is_idle());
        assert_eq!(rig.target_room, Some((3, 1)));
        assert_eq!(rig.advance(1.0 / 60.0), center);
    }

    /// 回归：吸附后的**同一帧**就必须拿到落点，不能被调用前的旧位置吞掉。
    /// （M2 实机验证暴露：相机停在初始 (0,0)，首帧吸附没写进相机。）
    #[test]
    fn snap_takes_effect_immediately() {
        let mut rig = CameraRigState::uninitialized();
        let center = room_center(0, 0, GRID.1);
        rig.snap_to(center, (0, 0));
        assert_eq!(rig.advance(1.0 / 60.0), center);
        // 后续每帧也稳定停在落点
        assert_eq!(rig.advance(1.0 / 60.0), center);
    }
}
