//! 输入采样：`ButtonInput<KeyCode>` → [`PlayerInput`]。见 `docs/00-conventions.md` D16、`docs/06-physics-and-movement.md` §6.1.5。
//!
//! 键位是本模块的**唯一**定义处（`00` D16：直接用 `ButtonInput`，在 `input` 模块内收敛为动作结构）。

use bevy::prelude::*;

use super::PlayerInput;
use crate::state::GameSet;

/// 左移：方向键 ← / A。
pub const MOVE_LEFT: [KeyCode; 2] = [KeyCode::ArrowLeft, KeyCode::KeyA];
/// 右移：方向键 → / D。
pub const MOVE_RIGHT: [KeyCode; 2] = [KeyCode::ArrowRight, KeyCode::KeyD];
/// 跳跃：空格 / 方向键 ↑ / Z。
pub const JUMP: [KeyCode; 3] = [KeyCode::Space, KeyCode::ArrowUp, KeyCode::KeyZ];
/// 交互/设置存档点：E（`02` §2.5）。
pub const INTERACT: [KeyCode; 1] = [KeyCode::KeyE];

pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>().add_systems(
            PreUpdate,
            // ★ 必须排在 Bevy 的输入系统之后：`ButtonInput<KeyCode>` 由它们在 `PreUpdate` 内更新，
            //   若本系统先跑就会读到上一帧的状态（表现为"按键被吞"）。
            sample_input
                .after(bevy::input::InputSystems)
                .in_set(GameSet::Input),
        );
    }
}

/// 采样一帧的输入。
///
/// `jump_pressed` 是**锁存**语义：按下时置位，由**恰好一个**固定步消费后清零
/// （消费在 `player::apply_player_physics`）。不能直接写 `just_pressed`——那只活一帧，
/// 而 `FixedUpdate` 每帧可能跑 **0 次**（渲染帧与固定步不同频），按下会被静默吞掉，
/// 表现为"跳跃键很不灵敏"。见 `06` §6.1.5。
///
/// 锁存不会造成"按住连跳"：`PlayerInput.jump_held` 才是持续量，起跳判定见 `06` §6.3.4。
pub fn sample_input(keys: Res<ButtonInput<KeyCode>>, mut input: ResMut<PlayerInput>) {
    let left = MOVE_LEFT.iter().any(|key| keys.pressed(*key));
    let right = MOVE_RIGHT.iter().any(|key| keys.pressed(*key));
    input.move_x = i8::from(right) - i8::from(left);
    if JUMP.iter().any(|key| keys.just_pressed(*key)) {
        input.jump_pressed = true; // 只置位；清零由物理步负责
    }
    input.jump_held = JUMP.iter().any(|key| keys.pressed(*key));
    if INTERACT.iter().any(|key| keys.just_pressed(*key)) {
        input.interact_pressed = true; // 只置位；清零由存档点系统负责
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    /// 回归（"跳跃键很不灵敏"）：按下那一刻若本帧没有固定步，
    /// `jump_pressed` 必须**保持**到下一个固定步，而不是被下一帧采样覆盖掉。
    #[test]
    fn jump_press_is_latched_until_consumed() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<PlayerInput>();

        // 第 1 帧：按下
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(app.world().resource::<PlayerInput>().jump_pressed);

        // 第 2 帧：键仍按住但不再 just_pressed，且本帧没有固定步
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(
            app.world().resource::<PlayerInput>().jump_pressed,
            "按下被下一帧采样覆盖 ⇒ 没有固定步的帧会丢按键"
        );
    }

    /// `E` 与跳跃同语义：按下置位后必须保持到存档点系统（固定步）消费。
    #[test]
    fn interact_press_is_latched_until_consumed() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<PlayerInput>();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(app.world().resource::<PlayerInput>().interact_pressed);

        // 下一帧仍按住但不再 just_pressed：锁存必须保留
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .run_system_once(sample_input)
            .expect("sample_input 运行失败");
        assert!(
            app.world().resource::<PlayerInput>().interact_pressed,
            "E 的按下不能被下一帧采样覆盖"
        );
    }
}
