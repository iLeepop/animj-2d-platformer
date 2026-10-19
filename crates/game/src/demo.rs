//! 验证用的脚本输入与物理度量。见 `docs/AGENTS.md` §4（物理/相机类改动必须实际跑起来看）。
//!
//! **为什么直接驱动 [`PlayerInput`] 而不是注入 `KeyboardInput`**：本项目的验证会话里窗口永远无法获得
//! 焦点（`SetForegroundWindow` 被系统拒绝），winit/输入层会丢掉 OS 级输入，注入的消息到不了
//! `ButtonInput<KeyCode>`。而 `KeyboardInput → ButtonInput → PlayerInput` 这条链路已由**无头测试**
//! 覆盖（`render::debug::tests::keyboard_message_reaches_the_toggle`，用 `MinimalPlugins` + `InputPlugin`）。
//! 因此本模块只负责把二者剩下的部分（固定步调度 → 物理 → AABB→Transform 投影）在真实 App 里测通。
//!
//! 脚本按**固定步**计时：本会话里游戏可能跑在 ~2000 fps，渲染帧与物理步相差两个数量级。

use bevy::prelude::*;

use crate::player::{PlayerInput, PlayerInputs, PlayerMotion, PlayerStepOutcome};
use crate::render::sprite_anim::CurrentAnimation;

/// 脚本的一步输入：物理动作 + 交互动作。
///
/// `interact_pressed` 不属于物理 `StepInput`（E 不参与物理），因此 demo 用自己的结构。
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptInput {
    pub move_x: i8,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub interact_pressed: bool,
}

/// 脚本条目：`(**该固定步起**生效的输入, ...)`，按顺序取"最后一个 `at <= 当前步`"的条目。
#[derive(Resource, Clone, Default)]
pub struct DemoScript(pub Vec<(u32, ScriptInput)>);

impl DemoScript {
    /// `walk`：第 5 步起一直按住右移。
    pub fn walk() -> Self {
        Self(vec![(5, step_input(1, false, true))])
    }

    /// `edge`：第 5 步起一直按住左移——撞地图左边界（`R5` 的实机对拍：
    /// 旧实现里盒子被压反后会"慢慢卡出去"，然后穿过一切掉落）。
    pub fn edge() -> Self {
        Self(vec![(5, step_input(-1, false, true))])
    }

    /// `hop`：一直右移并**一直按住跳跃**，每 15 步补一次"按下" ⇒ 连续小跳前进。
    ///
    /// **只用于检查平台/尖刺/果实几何**（比如重生点是否被平台顶住）。不能当通关判据：
    /// 脚本跳过错落在地面上的出口区域、也无法从平台右缘避开尖刺。M4 的判据用无头链路测试。
    pub fn hop() -> Self {
        let mut events = vec![(5, step_input(1, false, true))];
        let mut at = 20;
        while at <= 4000 {
            events.push((at, step_input(1, true, true)));
            events.push((at + 1, step_input(1, false, true)));
            at += 15;
        }
        Self(events)
    }

    /// `jump`：第 5 步起右移 30 步后停下（别走出房间视野），从第 40 步起每 25 步脉冲一次跳跃。
    pub fn jump() -> Self {
        let mut events = vec![
            (5, step_input(1, false, true)),
            (35, step_input(0, false, true)),
        ];
        let mut at = 40;
        while at <= 240 {
            events.push((at, step_input(1, true, true)));
            events.push((at + 1, step_input(0, false, true)));
            at += 25;
        }
        Self(events)
    }

    /// `checkpoint`：右移并在第 20 步起跳越过出生房间的高墙，第 22 步在存档点区域内按 `E`；
    /// 之后继续跳跃前进撞上尖刺 ⇒ 验证「设置存档点 → 死亡 → 回到存档点」（`02` §2.5）。
    pub fn checkpoint() -> Self {
        let mut events = vec![(5, step_input(1, false, true))];
        let mut at = 20;
        while at <= 4000 {
            events.push((at, step_input(1, true, true)));
            events.push((at + 1, step_input(1, false, true)));
            at += 15;
        }
        let mut pressing = step_input(1, false, true);
        pressing.interact_pressed = true;
        events.push((22, pressing));
        events.push((23, step_input(1, false, true)));
        events.sort_by_key(|(at, _)| *at);
        Self(events)
    }
}

fn step_input(move_x: i8, jump_pressed: bool, jump_held: bool) -> ScriptInput {
    ScriptInput {
        move_x,
        jump_pressed,
        jump_held,
        interact_pressed: false,
    }
}

/// 固定步计数。
#[derive(Resource, Default)]
pub struct DemoStep(u32);

pub struct DemoPlugin {
    pub script: DemoScript,
}

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.script.clone())
            .init_resource::<DemoStep>()
            // `FixedPreUpdate` 晚于 `PreUpdate` 的输入采样、早于 `FixedUpdate` 的物理消费
            .add_systems(FixedPreUpdate, (tick_demo_step, drive_script_input).chain())
            .add_systems(FixedUpdate, log_physics_metrics)
            // 帧动画是纯视觉的：demo 模式下把行切换打成日志，作为可自动断言的证据
            .add_systems(Update, log_animation_changes);
    }
}

/// 只在动画行**真正变化**时打一行（`04` §5.2 的 idle/run/jump/fall）。
fn log_animation_changes(players: Query<&CurrentAnimation, Changed<CurrentAnimation>>) {
    for current in &players {
        info!("动画：{:?}", current.0);
    }
}

fn tick_demo_step(mut step: ResMut<DemoStep>) {
    step.0 = step.0.saturating_add(1);
}

/// 用脚本覆盖本固定步的输入（覆盖采样结果，因此不依赖窗口焦点）。
fn drive_script_input(
    mut inputs: ResMut<PlayerInputs>,
    script: Res<DemoScript>,
    step: Res<DemoStep>,
    mut announced: Local<Option<u32>>,
) {
    let active = script
        .0
        .iter()
        .rev()
        .find(|(at, _)| *at <= step.0)
        .map(|(_, input)| *input);
    let Some(active) = active else {
        return;
    };
    inputs.slots[0] = PlayerInput {
        move_x: active.move_x,
        jump_pressed: active.jump_pressed,
        jump_held: active.jump_held,
        interact_pressed: active.interact_pressed,
    };
    if active.jump_pressed && *announced != Some(step.0) {
        *announced = Some(step.0);
        info!("DEMO 脚本：第 {} 个固定步按下跳跃", step.0);
    }
    if active.interact_pressed {
        info!("DEMO 脚本：第 {} 个固定步按下 E", step.0);
    }
}

/// 每个固定步打印关键事件与一次跳跃的峰值上升高度。
///
/// 峰值用于把「`06` §6.2.2 的解析值 80.0 px」在**真实 App 里**核对一遍。
///
/// 注意基准：本系统在物理步**之后**运行，起跳当步的位移已经发生，因此基准必须取**上一步**的脚底，
/// 否则峰值会少掉起跳当步那一段（实测会读成 73.47 而不是 80.00）。
fn log_physics_metrics(
    players: Query<(&PlayerMotion, &PlayerStepOutcome)>,
    mut state: Local<MetricsState>,
) {
    for (motion, outcome) in &players {
        let body = &motion.0;
        if outcome.0.jumped {
            state.jump = Some(JumpTracking {
                base_min_y: state.previous_min_y,
                peak: 0.0,
            });
            info!("DEMO 起跳：脚底 y = {:.2}", body.aabb.min.y);
        }
        if let Some(jump) = state.jump.as_mut() {
            jump.peak = jump.peak.max(body.aabb.min.y - jump.base_min_y);
            if body.vel.y <= 0.0 && jump.peak > 0.0 {
                info!(
                    "DEMO 峰值上升 = {:.2} px（解析值 {:.1} px，06 §6.2.6）",
                    jump.peak,
                    crate::config::physics::JUMP_HEIGHT_PX
                );
                state.jump = None;
            }
        }
        if outcome.0.landed {
            info!("DEMO 落地：脚底 y = {:.2}", body.aabb.min.y);
        }
        state.previous_min_y = body.aabb.min.y;
    }
}

#[derive(Default)]
struct MetricsState {
    previous_min_y: f32,
    jump: Option<JumpTracking>,
}

struct JumpTracking {
    base_min_y: f32,
    peak: f32,
}
