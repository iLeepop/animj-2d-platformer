//! 角色的 ECS 适配层。所有者文档：`docs/06-physics-and-movement.md` §6.1.5。
//!
//! 只做三件事：取输入 → 调 [`crate::physics::step`] → 写回组件。**不含任何物理逻辑**。

pub mod death;
pub mod decor;
pub mod input;
pub mod spawn;

use bevy::prelude::*;

use crate::physics::{Body, StepInput, StepOutcome, step};
use crate::state::GameSet;
use crate::tiles::LevelTiles;

/// 角色的物理状态（**逻辑真相**，`06` §6.8.1）。
#[derive(Component, Debug)]
pub struct PlayerMotion(pub Body);

/// 本帧采样出的输入（`00` D16 的动作结构）。
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PlayerInput {
    /// `-1` / `0` / `+1`。
    pub move_x: i8,
    /// **锁存**的"刚按下"：采样置位，物理步消费后清零（见 `input::sample_input`）。
    pub jump_pressed: bool,
    pub jump_held: bool,
    /// **锁存**的"刚按下 E"：采样置位，存档点系统消费后清零（见 `input::sample_input`）。
    pub interact_pressed: bool,
}

/// 本固定步的物理结果（表现层与 demo 用；语义见 `06` §6.1.4）。
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PlayerStepOutcome(pub StepOutcome);

/// 角色朝向（`true` = 朝右）。默认朝右（`03` §3.4 的出生点约定）。
///
/// 朝向是纯表现状态：只决定 sprite 是否水平翻转（`01` §1.3、`04` §4.5），
/// **不参与**碰撞与物理（逻辑真相仍是 `Aabb`）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerFacing(pub bool);

impl Default for PlayerFacing {
    fn default() -> Self {
        Self(true)
    }
}

/// 输入方向 ⇒ 朝向：`move_x == 0` 时保持原朝向（停下不翻面）。
pub fn next_facing(current: bool, move_x: i8) -> bool {
    match move_x.signum() {
        0 => current,
        sign => sign > 0,
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((input::PlayerInputPlugin, death::PlayerDeathPlugin))
            .init_resource::<decor::Decorations>()
            .add_systems(
                Update,
                (
                    update_player_facing,
                    decor::sync_decoration_layers,
                    decor::sync_decoration_facing,
                )
                    .chain(),
            )
            .add_systems(FixedUpdate, apply_player_physics.in_set(GameSet::Physics));
    }
}

/// 采样输入 ⇒ [`PlayerFacing`]（表现层，`Update` 内；输入已在 `PreUpdate` 采样完成）。
fn update_player_facing(input: Res<PlayerInput>, mut players: Query<&mut PlayerFacing>) {
    if input.move_x == 0 {
        return;
    }
    for mut facing in &mut players {
        let next = next_facing(facing.0, input.move_x);
        if facing.0 != next {
            facing.0 = next;
        }
    }
}

/// 把一个固定步推进到角色实体上。
pub fn apply_player_physics(
    mut input: ResMut<PlayerInput>,
    tiles: Option<Res<LevelTiles>>,
    mut players: Query<(&mut PlayerMotion, &mut PlayerStepOutcome)>,
) {
    // 关卡尚未加载时没有瓦片数据：物理不推进（此时 `GameState` 还在 Loading，本系统不会被门控触发，
    // 但保持 Option 以防编辑器/测试场景只加了 PlayerPlugin）。
    let Some(tiles) = tiles else {
        return;
    };

    let step_input = StepInput {
        move_x: input.move_x,
        jump_pressed: input.jump_pressed,
        jump_held: input.jump_held,
    };
    for (mut motion, mut outcome) in &mut players {
        let mut out = StepOutcome::default();
        step(
            &crate::config::physics::DEFAULT_PHYSICS,
            tiles.as_ref(),
            &mut motion.0,
            step_input,
            &mut out,
        );
        outcome.0 = out;
    }
    // ★ 消费锁存：一个"按下"恰好被**一个**固定步看到。否则同帧两次固定步会把缓冲反复刷新，
    //   而没有固定步的帧会把按下吞掉（`06` §6.1.5）。
    input.jump_pressed = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 零输入保持原朝向；±1 按符号定向。看的是**朝向**，不是速度。
    #[test]
    fn facing_follows_input_but_keeps_direction_when_stopped() {
        assert!(next_facing(true, 0));
        assert!(!next_facing(false, 0));
        assert!(next_facing(false, 1));
        assert!(!next_facing(true, -1));
        assert!(next_facing(true, 1));
        assert!(!next_facing(false, -1));
    }
}
