//! 调试可视化。所有者文档：`docs/07-rendering.md` §7.9。
//!
//! 全部绘制在自定义 `dev` feature 下注册：发布构建调用同名空实现，因此调用点不必写 `cfg`。
//!
//! **不使用 `bevy::dev_tools::fps_overlay`**：它由 `bevy_dev_tools/render` 门控，而那个 feature
//! 会拖入 `bevy_pbr`（3D），其运行期资源在 2D-only feature 集下不存在 ⇒ 同一门控下的
//! `render_debug` 初始化系统会 panic。本模块用 `FrameTimeDiagnosticsPlugin` + 自有 `Text` 节点实现同等能力。

use bevy::prelude::*;

/// 调试绘制开关（`dev` 构建默认开，`F3` 切换）。
#[derive(Resource)]
pub struct DebugDrawEnabled(pub bool);

impl Default for DebugDrawEnabled {
    fn default() -> Self {
        // 刻意不写 `Self(cfg!(feature = "dev"))`：那会让 clippy 误以为可以 `#[derive(Default)]`，
        // 而 derive 出来的是 `false`（与「dev 下默认开」的意图相反）。
        #[cfg(feature = "dev")]
        let enabled = true;
        #[cfg(not(feature = "dev"))]
        let enabled = false;
        Self(enabled)
    }
}

/// 调试信息文本节点标记。
#[cfg(feature = "dev")]
#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugDrawEnabled>()
            .add_systems(Update, toggle_debug_draw);
        add_debug_visualization(app);
    }
}

/// F3 的判定逻辑（纯函数）：**只在「刚按下」时翻转**，长按不重复翻转。
///
/// 抽成纯函数是为了可单测 —— 不需要窗口、输入后端或系统级按键注入（见 `docs/AGENTS.md` §4）。
/// 「OS 按键 → `ButtonInput`」这一段是 winit 后端的责任，不属于本项目逻辑。
pub(crate) fn toggled(keys: &ButtonInput<KeyCode>, current: bool) -> Option<bool> {
    keys.just_pressed(KeyCode::F3).then_some(!current)
}

fn toggle_debug_draw(keys: Res<ButtonInput<KeyCode>>, mut enabled: ResMut<DebugDrawEnabled>) {
    let Some(next) = toggled(&keys, enabled.0) else {
        return;
    };
    enabled.0 = next;
    info!("调试绘制：{}", if next { "开" } else { "关" });
}

#[cfg(feature = "dev")]
mod dev {
    use super::*;
    use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
    use bevy::text::FontSize;
    use level_format::{ROOM_PX, TILE_PX};

    use crate::coords::room_local_to_world;
    use crate::world::CurrentRoom;
    use crate::world::load::LoadedLevel;

    pub(crate) fn add(app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_debug_text)
            .add_systems(Update, update_debug_text);
    }

    pub(crate) fn debug_enabled(enabled: Res<DebugDrawEnabled>) -> bool {
        enabled.0
    }

    fn spawn_debug_text(mut commands: Commands) {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(8.0),
                bottom: px(8.0),
                ..default()
            },
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(10.0),
                ..default()
            },
            TextColor(Color::srgb_u8(0xef, 0x7d, 0x57)),
            DebugText,
        ));
    }

    /// 自建诊断行：FPS + 实体数。走 HUD 的同一套 `Node` + `Text`（内层相机），
    /// 因此与游戏画面共用一个像素格（`07` §7.10.1）。
    fn update_debug_text(
        enabled: Res<DebugDrawEnabled>,
        diagnostics: Res<DiagnosticsStore>,
        entities: Query<Entity>,
        text: Single<(&mut Text, &mut Visibility), With<DebugText>>,
    ) {
        let (mut text, mut visibility) = text.into_inner();
        if !enabled.0 {
            *visibility = Visibility::Hidden;
            return;
        }
        *visibility = Visibility::Visible;
        let fps = diagnostics
            .get_measurement(&FrameTimeDiagnosticsPlugin::FPS)
            .map(|measurement| measurement.value)
            .unwrap_or(0.0);
        text.0 = format!(
            "FPS {fps:.0}  entities {}\nF3: toggle debug draw",
            entities.iter().count()
        );
    }

    /// 只画**当前房间**的 `TILE_PX` 网格：每轴 `ROOM_TILES + 1` 条线。
    ///
    /// 不画全部房间（30 房间 × 34 条 = 1020 条/帧 是无谓开销）。
    pub(crate) fn draw_room_grid(
        mut gizmos: Gizmos,
        current: Res<CurrentRoom>,
        loaded: Res<LoadedLevel>,
    ) {
        let rows = loaded.level.grid.rows;
        let half = ROOM_PX as f32 / 2.0;
        let center = room_local_to_world(current.col, current.row, rows, Vec2::splat(half));
        let color = Color::srgba(1.0, 1.0, 1.0, 0.15);

        let mut offset = 0.0;
        while offset <= ROOM_PX as f32 {
            let o = -half + offset;
            gizmos.line_2d(
                Vec2::new(center.x + o, center.y - half),
                Vec2::new(center.x + o, center.y + half),
                color,
            );
            gizmos.line_2d(
                Vec2::new(center.x - half, center.y + o),
                Vec2::new(center.x + half, center.y + o),
                color,
            );
            offset += TILE_PX as f32;
        }
    }
}

#[cfg(feature = "dev")]
fn add_debug_visualization(app: &mut App) {
    dev::add(app);
    app.add_systems(
        Update,
        // ★ 只在**玩法运行**时画：编辑器停在 `Paused`，游戏侧 `CurrentRoom` 永远不更新，
        //   那个网格会钉死在 (0,0)（实机 bug："切房间后网格仍在房间 1"）。编辑器有自己的叠加层。
        dev::draw_room_grid
            .run_if(dev::debug_enabled)
            .run_if(in_state(crate::state::GameState::Playing))
            .run_if(resource_exists::<crate::world::load::LoadedLevel>),
    );
}

/// 生产构建：不做任何调试绘制（保留资源，避免系统参数缺资源报错）。
#[cfg(not(feature = "dev"))]
fn add_debug_visualization(_app: &mut App) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// F3 只在**刚按下**那一帧翻转：长按与松开都不得改变开关。
    #[test]
    fn f3_toggles_only_on_just_pressed() {
        let mut keys = ButtonInput::<KeyCode>::default();

        // 1) 刚按下 ⇒ 翻转（两个方向都要对）
        keys.press(KeyCode::F3);
        assert!(keys.just_pressed(KeyCode::F3));
        assert_eq!(toggled(&keys, true), Some(false));
        assert_eq!(toggled(&keys, false), Some(true));

        // 2) 长按（`just_pressed` 已被消费）⇒ 不翻转，否则按住 F3 会每帧狂闪
        keys.clear_just_pressed(KeyCode::F3);
        assert!(keys.pressed(KeyCode::F3));
        assert_eq!(toggled(&keys, true), None);

        // 3) 松开那一帧 ⇒ 不翻转
        keys.release(KeyCode::F3);
        assert!(keys.just_released(KeyCode::F3));
        assert_eq!(toggled(&keys, true), None);

        // 4) 再次按下 ⇒ 再次翻转（连按可反复切换）
        keys.press(KeyCode::F3);
        assert_eq!(toggled(&keys, true), Some(false));
    }

    /// **端到端输入链路**：注入 winit 后端本身产出的那种消息（`KeyboardInput`，0.20 里是 `Message`），
    /// 经 `InputPlugin` 的 `keyboard_input_system`（`PreUpdate`）写进 `ButtonInput<KeyCode>`，
    /// 再由本模块的系统（`Update`）读出来翻转开关。
    ///
    /// 这条测试钉住的是**调度顺序**：`.add_systems(Update, …)` 必须晚于输入系统，
    /// 若把它错放进 `First`（早于 `PreUpdate`），就会永远读到上一帧的输入 —— 这正是最容易写错的地方。
    /// 不需要窗口、GPU 或操作系统级按键注入。
    #[test]
    fn keyboard_message_reaches_the_toggle() {
        use bevy::input::ButtonState;
        use bevy::input::InputPlugin;
        use bevy::input::keyboard::{Key, KeyboardInput};
        use bevy::prelude::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(InputPlugin)
            .insert_resource(DebugDrawEnabled(true))
            .add_systems(Update, toggle_debug_draw);
        app.update(); // 让插件把资源与消息队列建好

        // 注入一次 F3 按下（与 winit 后端产生的类型完全一致）
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::F3,
            logical_key: Key::F3,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();

        assert!(
            !app.world().resource::<DebugDrawEnabled>().0,
            "F3 按下后调试绘制应被关闭（若失败，多半是本系统被排在了输入系统之前）"
        );

        // 松开不是「刚按下」⇒ 开关保持不变
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::F3,
            logical_key: Key::F3,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert!(
            !app.world().resource::<DebugDrawEnabled>().0,
            "松开 F3 不应改变开关"
        );
    }
}
