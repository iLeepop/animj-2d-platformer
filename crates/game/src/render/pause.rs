//! 暂停菜单与装饰品装备界面。所有者文档：`docs/05-architecture.md` §5.5、`docs/01-game-design.md` §1.5。
//!
//! `GameState::Paused` 冻结物理与计时（`05` §5.5）。注意：编辑器也把状态停在 `Paused`，
//! 但它不会把 [`PauseMenu::open`] 置真 ⇒ 编辑态不会弹出本菜单。

use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Button, ScrollArea};

use crate::config::render::{DECORATION_LAYERS, MENU_FONT_PX, VIEWPORT_H, VIEWPORT_W, z};
use crate::fonts::UiFont;
use crate::player::decor::{Decorations, spec_of};
use crate::save;
use crate::settings::GameSettings;
use crate::state::GameState;
use crate::world::progress::LevelProgress;

/// 暂停面板距视口边的留白。视口尺寸见 `07` §7.1.1。
const PAUSE_MARGIN_X: f32 = 12.0;
const PAUSE_MARGIN_Y: f32 = 8.0;
/// 面板铺满视口（减去留白）；内容过高时由内部滚动区处理，不再越出屏幕。
const PAUSE_WIDTH: f32 = VIEWPORT_W as f32 - 2.0 * PAUSE_MARGIN_X;
const PAUSE_HEIGHT: f32 = VIEWPORT_H as f32 - 2.0 * PAUSE_MARGIN_Y;

#[derive(Resource, Debug, Default)]
pub struct PauseMenu {
    pub open: bool,
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component)]
struct PauseButton(PauseAction);

#[derive(Component, Debug, Clone)]
enum PauseAction {
    Resume,
    Settings,
    Save,
    MainMenu,
    Quit,
    Equip(String),
    Unequip(usize),
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenu>()
            .add_systems(Update, toggle_pause)
            .add_systems(Update, (rebuild_pause_ui, handle_pause_buttons).chain());
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match state.get() {
        GameState::Playing => {
            menu.open = true;
            next.set(GameState::Paused);
            info!("暂停");
        }
        GameState::Paused if menu.open => {
            menu.open = false;
            next.set(GameState::Playing);
            info!("继续");
        }
        _ => {}
    }
}

/// 菜单开关或装备变化时重建（一次性重建，避免每帧 diff UI 树）。
fn rebuild_pause_ui(
    mut commands: Commands,
    menu: Res<PauseMenu>,
    decorations: Res<Decorations>,
    settings: Option<Res<GameSettings>>,
    font: Res<UiFont>,
    roots: Query<Entity, With<PauseRoot>>,
) {
    let settings_changed = settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    if !menu.is_changed() && !decorations.is_changed() && !settings_changed {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !menu.open {
        return;
    }

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(PAUSE_MARGIN_X),
                top: px(PAUSE_MARGIN_Y),
                width: px(PAUSE_WIDTH),
                height: px(PAUSE_HEIGHT),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                padding: UiRect::all(px(8.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
            PauseRoot,
            GlobalZIndex(10),
        ))
        .id();

    let language = settings
        .as_ref()
        .map(|settings| settings.language)
        .unwrap_or_default();
    let s = language.strings();
    button(
        &mut commands,
        &font,
        root,
        format!("{} (Esc)", s.resume),
        PauseAction::Resume,
    );
    button(
        &mut commands,
        &font,
        root,
        s.settings,
        PauseAction::Settings,
    );
    button(
        &mut commands,
        &font,
        root,
        format!("{} (Ctrl+S)", s.save),
        PauseAction::Save,
    );
    button(
        &mut commands,
        &font,
        root,
        s.main_menu,
        PauseAction::MainMenu,
    );
    button(&mut commands, &font, root, s.quit_save, PauseAction::Quit);

    // 装饰品行放进滚动区：行数多时不再把面板顶出视口。
    let scroll = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                flex_grow: 1.0,
                min_height: px(0.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            ScrollArea,
            ChildOf(root),
        ))
        .id();

    for layer in 0..DECORATION_LAYERS {
        let current = decorations
            .equipped_at(layer)
            .map(str::to_string)
            .unwrap_or_else(|| "-".to_string());
        let row = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    flex_shrink: 0.0,
                    column_gap: px(4.0),
                    row_gap: px(2.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                ChildOf(scroll),
            ))
            .id();
        commands.spawn((
            Text::new(format!("L{layer} {current}")),
            font.text_font(MENU_FONT_PX),
            TextColor(Color::WHITE),
            ChildOf(row),
        ));
        for id in decorations.owned() {
            let Some(spec) = spec_of(id) else {
                continue; // 未知 id：不显示（存档缺失元数据视为未装备）
            };
            if spec.layer != layer {
                continue;
            }
            button(
                &mut commands,
                &font,
                row,
                id,
                PauseAction::Equip(id.clone()),
            );
        }
        button(&mut commands, &font, row, "x", PauseAction::Unequip(layer));
    }
    let _ = z::DECORATION_BASE;
}

fn button(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    action: PauseAction,
) {
    commands
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(px(6.0), px(2.0)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
            PauseButton(action),
            ChildOf(parent),
        ))
        .with_child((
            Text::new(label),
            font.text_font(MENU_FONT_PX),
            TextColor(Color::WHITE),
        ));
}

#[allow(clippy::too_many_arguments)]
fn handle_pause_buttons(
    interactions: Query<&PauseButton, Added<Pressed>>,
    mut decorations: ResMut<Decorations>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut settings_return: ResMut<crate::state::SettingsReturn>,
    progress: Option<Res<LevelProgress>>,
    checkpoint: Option<Res<crate::checkpoint::ActiveCheckpoint>>,
    mode: Res<crate::state::SessionMode>,
    mut exit: MessageWriter<AppExit>,
) {
    for button in &interactions {
        match &button.0 {
            PauseAction::Resume => {
                menu.open = false;
                next.set(GameState::Playing);
            }
            PauseAction::Settings => {
                settings_return.0 = GameState::Paused;
                next.set(GameState::Settings);
            }
            PauseAction::Save => {
                if let Some(progress) = progress.as_ref().filter(|_| !mode.is_client()) {
                    save::persist_now(
                        progress,
                        &decorations,
                        checkpoint.as_ref().and_then(|point| point.0),
                    );
                }
            }
            PauseAction::MainMenu => {
                if let Some(progress) = progress.as_ref().filter(|_| !mode.is_client()) {
                    save::persist_now(
                        progress,
                        &decorations,
                        checkpoint.as_ref().and_then(|point| point.0),
                    );
                }
                menu.open = false;
                next.set(GameState::MainMenu);
            }
            PauseAction::Quit => {
                if let Some(progress) = progress.as_ref().filter(|_| !mode.is_client()) {
                    save::persist_now(
                        progress,
                        &decorations,
                        checkpoint.as_ref().and_then(|point| point.0),
                    );
                }
                exit.write(AppExit::Success);
            }
            PauseAction::Equip(id) => {
                decorations.equip(id);
            }
            PauseAction::Unequip(layer) => {
                decorations.unequip(*layer);
            }
        }
    }
}
