//! 物理 / 手感常量。**所有者文档：`docs/06-physics-and-movement.md`**（§6.2.4 是权威表）。
//!
//! 改任何一个数值都必须：① 只改这里；② 重跑 `cargo test -p game`（`06` §6.9 的全部断言）；
//! ③ 同步 `06` §6.2 的表格与派生预算。

use bevy::math::Vec2;

/// 固定步长频率（Hz）。定义在 `config` 顶层，见 `06` §6.2.4 / `00` D11。
pub use crate::config::FIXED_HZ;

/// 物理积分步长（秒）。`06` §6.1.3：积分**必须**用常量而不是 `Time<Fixed>::delta_secs()`。
pub const FIXED_DT: f32 = 1.0 / FIXED_HZ as f32;

/// 设计意图：满跳上升高度（瓦片）。`06` §6.2.1。
pub const JUMP_HEIGHT: f32 = 2.5;
/// 设计意图：满跳上升高度（像素）。
pub const JUMP_HEIGHT_PX: f32 = JUMP_HEIGHT * level_format::TILE_PX as f32;
/// 设计意图：起跳 → 顶点的时间（秒）。
pub const JUMP_APEX_TIME: f32 = 0.40;

/// 上升段重力（px/s²）`= 2h/t²`。
pub const GRAVITY: f32 = 1000.0;
/// 起跳初速（px/s）`= 2h/t`。
pub const JUMP_VELOCITY: f32 = 400.0;

/// 下落重力倍率。
pub const FALL_GRAVITY_MULT: f32 = 1.35;
/// 下落段重力（px/s²）`= GRAVITY × FALL_GRAVITY_MULT`。
pub const FALL_GRAVITY: f32 = GRAVITY * FALL_GRAVITY_MULT;

/// 终端下落速度（px/s）。
pub const MAX_FALL_SPEED: f32 = 560.0;
/// 水平最大速度（px/s）。
pub const MAX_RUN_SPEED: f32 = 200.0;
/// 地面朝输入方向的加速度（px/s²）。
pub const GROUND_ACCEL: f32 = 1200.0;
/// 地面无输入时的减速度（px/s²）。
pub const GROUND_FRICTION: f32 = 1600.0;
/// 空中朝输入方向的加速度（px/s²）。
pub const AIR_ACCEL: f32 = 900.0;
/// 空中无输入时的减速度（px/s²）。
pub const AIR_DRAG: f32 = 400.0;

/// 松手后上升速度的上限（px/s）：可变跳跃高度。
pub const JUMP_CUT_VELOCITY: f32 = 280.0;

/// 离地后可起跳的固定步数（100 ms）。
pub const COYOTE_STEPS: u32 = 6;
/// 落地前跳跃按下的记忆固定步数（100 ms）。
pub const JUMP_BUFFER_STEPS: u32 = 6;

/// 垂直接触 / 落地吸附容差（px）。
pub const CONTACT_TOLERANCE: f32 = 4.0;
/// 分离余量 & 瓦片查询偏置（px）。同一个常量，用途见 `06` §6.4.2。
pub const GEOM_EPS: f32 = 0.001;

/// 单子步最大位移（px），推导见 `06` §6.4.6。
pub const SUBSTEP_MAX_DISTANCE: f32 = 16.0;
/// 单步最大子步数。
pub const SUBSTEP_HARD_CAP: u32 = 8;
/// 常量表硬约束上限（px）：任一轴速度 ≤ `MAX_STEP_DISPLACEMENT × FIXED_HZ`。
pub const MAX_STEP_DISPLACEMENT: f32 = SUBSTEP_MAX_DISTANCE * SUBSTEP_HARD_CAP as f32;

/// 角色 AABB 尺寸（宽 × 高，px）。
pub const PLAYER_SIZE: Vec2 = Vec2::new(20.0, 28.0);
/// Hazard 判定的角色 AABB 内缩量（px）。
pub const HAZARD_INSET: f32 = 2.0;
/// 拾取判定的角色 AABB 外扩量（px）。
pub const PICKUP_INFLATE: f32 = 2.0;
/// 拾取物 AABB 边长（px）。
pub const PICKUP_SIZE: f32 = 12.0;

/// 物理参数集。
///
/// `step()` 接收它而不是直接读 `const`，使测试能构造覆盖版本（`06` §6.9 的 T10、T10b）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsConfig {
    pub fixed_dt: f32,
    pub gravity: f32,
    pub jump_velocity: f32,
    pub fall_gravity: f32,
    pub max_fall_speed: f32,
    pub max_run_speed: f32,
    pub ground_accel: f32,
    pub ground_friction: f32,
    pub air_accel: f32,
    pub air_drag: f32,
    pub jump_cut_velocity: f32,
    pub coyote_steps: u32,
    pub jump_buffer_steps: u32,
    pub contact_tolerance: f32,
    pub geom_eps: f32,
    pub substep_max_distance: f32,
    pub substep_hard_cap: u32,
    pub player_size: Vec2,
}

/// 默认物理参数 = [`super`] 的常量表（`06` §6.2.4）。
pub const DEFAULT_PHYSICS: PhysicsConfig = PhysicsConfig {
    fixed_dt: FIXED_DT,
    gravity: GRAVITY,
    jump_velocity: JUMP_VELOCITY,
    fall_gravity: FALL_GRAVITY,
    max_fall_speed: MAX_FALL_SPEED,
    max_run_speed: MAX_RUN_SPEED,
    ground_accel: GROUND_ACCEL,
    ground_friction: GROUND_FRICTION,
    air_accel: AIR_ACCEL,
    air_drag: AIR_DRAG,
    jump_cut_velocity: JUMP_CUT_VELOCITY,
    coyote_steps: COYOTE_STEPS,
    jump_buffer_steps: JUMP_BUFFER_STEPS,
    contact_tolerance: CONTACT_TOLERANCE,
    geom_eps: GEOM_EPS,
    substep_max_distance: SUBSTEP_MAX_DISTANCE,
    substep_hard_cap: SUBSTEP_HARD_CAP,
    player_size: PLAYER_SIZE,
};
