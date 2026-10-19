//! 输入采样：`ButtonInput<KeyCode>` → [`PlayerInputs`] 的槽位。
//! 见 `docs/00-conventions.md` D16、`docs/06-physics-and-movement.md` §6.1.5、§6.10。
//!
//! 键位是本模块的**唯一**定义处（`00` D16：直接用 `ButtonInput`，在 `input` 模块内收敛为动作结构）。

use bevy::prelude::*;

use super::{PlayerId, PlayerInput, PlayerInputs};
use crate::state::{GameSet, SessionMode};

/// 单人 / 主机玩家的左移：方向键 ← / A（单人保留 WASD，避免破坏既有手感）。
pub const P1_MOVE_LEFT: [KeyCode; 2] = [KeyCode::ArrowLeft, KeyCode::KeyA];
/// 单人 / 主机玩家的右移：方向键 → / D。
pub const P1_MOVE_RIGHT: [KeyCode; 2] = [KeyCode::ArrowRight, KeyCode::KeyD];
/// 单人 / 主机玩家的跳跃：空格 / 方向键 ↑ / Z。
pub const P1_JUMP: [KeyCode; 3] = [KeyCode::Space, KeyCode::ArrowUp, KeyCode::KeyZ];
/// 单人 / 主机玩家的交互：E（`02` §2.5）。
pub const P1_INTERACT: [KeyCode; 1] = [KeyCode::KeyE];

/// 本地双人时 P1 只保留方向键 + Z/E：把 WASD/空格让给 P2（`06` §6.1.6）。
pub const P1_COOP_MOVE_LEFT: [KeyCode; 1] = [KeyCode::ArrowLeft];
pub const P1_COOP_MOVE_RIGHT: [KeyCode; 1] = [KeyCode::ArrowRight];
pub const P1_COOP_JUMP: [KeyCode; 2] = [KeyCode::ArrowUp, KeyCode::KeyZ];
pub const P1_COOP_INTERACT: [KeyCode; 1] = [KeyCode::KeyE];

/// 本地双人时 P2：WASD 移动、W/空格跳跃、F 交互。
pub const P2_MOVE_LEFT: [KeyCode; 1] = [KeyCode::KeyA];
pub const P2_MOVE_RIGHT: [KeyCode; 1] = [KeyCode::KeyD];
pub const P2_JUMP: [KeyCode; 2] = [KeyCode::KeyW, KeyCode::Space];
pub const P2_INTERACT: [KeyCode; 1] = [KeyCode::KeyF];

pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInputs>().add_systems(
            PreUpdate,
            // ★ 必须排在 Bevy 的输入系统之后：`ButtonInput<KeyCode>` 由它们在 `PreUpdate` 内更新，
            //   若本系统先跑就会读到上一帧的状态（表现为"按键被吞"）。
            sample_input
                .after(bevy::input::InputSystems)
                .in_set(GameSet::Input),
        );
    }
}

fn any_pressed(keys: &ButtonInput<KeyCode>, keyset: &[KeyCode]) -> bool {
    keyset.iter().any(|key| keys.pressed(*key))
}

fn any_just_pressed(keys: &ButtonInput<KeyCode>, keyset: &[KeyCode]) -> bool {
    keyset.iter().any(|key| keys.just_pressed(*key))
}

fn sample_player(keys: &ButtonInput<KeyCode>, slot: &mut PlayerInput, bindings: Bindings) {
    let left = any_pressed(keys, bindings.left);
    let right = any_pressed(keys, bindings.right);
    slot.move_x = i8::from(right) - i8::from(left);
    if any_just_pressed(keys, bindings.jump) {
        slot.jump_pressed = true; // 只置位；清零由物理步负责
    }
    slot.jump_held = any_pressed(keys, bindings.jump);
    if any_just_pressed(keys, bindings.interact) {
        slot.interact_pressed = true; // 只置位；清零由存档点系统负责
    }
}

/// 一组键位（借用以避免在采样函数里出现四个参数）。
struct Bindings {
    left: &'static [KeyCode],
    right: &'static [KeyCode],
    jump: &'static [KeyCode],
    interact: &'static [KeyCode],
}

/// 采样一帧的输入。
///
/// `jump_pressed` 是**锁存**语义：按下时置位，由**恰好一个**固定步消费后清零
/// （消费在 `player::apply_player_physics`）。不能直接写 `just_pressed`——那只活一帧，
/// 而 `FixedUpdate` 每帧可能跑 **0 次**（渲染帧与固定步不同频），按下会被静默吞掉，
/// 表现为"跳跃键很不灵敏"。见 `06` §6.1.5。
///
/// 锁存不会造成"按住连跳"：`PlayerInput.jump_held` 才是持续量，起跳判定见 `06` §6.3.4。
pub fn sample_input(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<SessionMode>,
    mut inputs: ResMut<PlayerInputs>,
) {
    let coop = matches!(*mode, SessionMode::LocalCoop);
    // 本地双人：P1 让出 WASD/空格。单人（含主机、客户端）保留完整键位。
    let p1 = if coop {
        Bindings {
            left: &P1_COOP_MOVE_LEFT,
            right: &P1_COOP_MOVE_RIGHT,
            jump: &P1_COOP_JUMP,
            interact: &P1_COOP_INTERACT,
        }
    } else {
        Bindings {
            left: &P1_MOVE_LEFT,
            right: &P1_MOVE_RIGHT,
            jump: &P1_JUMP,
            interact: &P1_INTERACT,
        }
    };
    sample_player(&keys, &mut inputs.slots[PlayerId(0).index()], p1);

    // P2 只在本地双人时采样：否则主机上的 WASD 会错误地控制远程玩家 1。
    if coop {
        sample_player(
            &keys,
            &mut inputs.slots[PlayerId(1).index()],
            Bindings {
                left: &P2_MOVE_LEFT,
                right: &P2_MOVE_RIGHT,
                jump: &P2_JUMP,
                interact: &P2_INTERACT,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<PlayerInputs>()
            .init_resource::<SessionMode>();
        app
    }

    /// 回归（"跳跃键很不灵敏"）：按下那一刻若本帧没有固定步，
    /// `jump_pressed` 必须**保持**到下一个固定步，而不是被下一帧采样覆盖掉。
    #[test]
    fn jump_press_is_latched_until_consumed() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(app.world().resource::<PlayerInputs>().slots[0].jump_pressed);

        // 第 2 帧：键仍按住但不再 just_pressed，且本帧没有固定步
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(
            app.world().resource::<PlayerInputs>().slots[0].jump_pressed,
            "按下被下一帧采样覆盖 ⇒ 没有固定步的帧会丢按键"
        );
    }

    /// `E` 与跳跃同语义：按下置位后必须保持到存档点系统（固定步）消费。
    #[test]
    fn interact_press_is_latched_until_consumed() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(app.world().resource::<PlayerInputs>().slots[0].interact_pressed);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(
            app.world().resource::<PlayerInputs>().slots[0].interact_pressed,
            "E 的按下不能被下一帧采样覆盖"
        );
    }

    /// 本地双人：WASD 属于 P2，方向键属于 P1，互不串台（`06` §6.1.6）。
    #[test]
    fn local_coop_splits_the_keyboard_between_two_players() {
        let mut app = app();
        *app.world_mut().resource_mut::<SessionMode>() = SessionMode::LocalCoop;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyD);
            keys.press(KeyCode::ArrowLeft);
        }
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        let inputs = app.world().resource::<PlayerInputs>();
        assert_eq!(inputs.slots[0].move_x, -1, "P1 只认方向键");
        assert_eq!(inputs.slots[1].move_x, 1, "P2 认 WASD");
    }

    /// 单人模式不采样 P2：主机上的 WASD 不得控制远程玩家（`06` §6.10）。
    #[test]
    fn solo_mode_does_not_write_the_second_slot() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        let inputs = app.world().resource::<PlayerInputs>();
        assert_eq!(inputs.slots[0].move_x, 1);
        assert_eq!(inputs.slots[1].move_x, 0);
    }
}
