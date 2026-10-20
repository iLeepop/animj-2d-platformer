//! 暂停菜单与装饰品装备界面。所有者文档：`docs/05-architecture.md` §5.5、`docs/01-game-design.md` §1.5。
//!
//! `GameState::Paused` 冻结物理与计时（`05` §5.5）。注意：编辑器也把状态停在 `Paused`，
//! 但它不会把 [`PauseMenu::open`] 置真 ⇒ 编辑态不会弹出本菜单。

use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ScrollArea};

use crate::config::render::DECORATION_LAYERS;
use crate::config::ui::{
    MENU_GAP_PX, UI_BUTTON_HEIGHT, UI_BUTTON_HEIGHT_SM, UI_PAUSE_BUTTON_WIDTH, UI_PAUSE_PANEL_WIDTH,
};
use crate::fonts::UiFont;
use crate::player::decor::{Decorations, spec_of};
use crate::render::pixel_camera::ViewLayout;
use crate::render::ui_kit::{self, UiButtonRole};
use crate::save;
use crate::settings::GameSettings;
use crate::state::GameState;
use crate::world::progress::LevelProgress;

#[derive(Resource, Debug, Default)]
pub struct PauseMenu {
    pub open: bool,
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component)]
struct PauseScroll;

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
            .add_observer(handle_pause_activate)
            .add_systems(Update, (toggle_pause, rebuild_pause_ui).chain());
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

/// 菜单开关、装备或语言变化时重建；重建前保留滚动位置。
#[allow(clippy::too_many_arguments)]
fn rebuild_pause_ui(
    mut commands: Commands,
    menu: Res<PauseMenu>,
    decorations: Res<Decorations>,
    layout: Res<ViewLayout>,
    settings: Option<Res<GameSettings>>,
    state: Res<State<GameState>>,
    font: Res<UiFont>,
    roots: Query<Entity, With<PauseRoot>>,
    scrolls: Query<&ScrollPosition, With<PauseScroll>>,
) {
    let settings_changed = settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    let entering = state.is_changed() && *state.get() == GameState::Paused;
    let changed = menu.is_changed()
        || decorations.is_changed()
        || settings_changed
        || layout.is_changed()
        || entering;
    if !changed {
        return;
    }
    let was_open = !roots.is_empty();
    let scroll_pos = scrolls
        .iter()
        .next()
        .map(|position| position.0)
        .unwrap_or(Vec2::ZERO);
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !menu.open {
        return;
    }

    let root = ui_kit::scrim_screen(&mut commands, &layout, 10);
    commands.entity(root).insert(PauseRoot);
    let panel = ui_kit::panel(&mut commands, root, UI_PAUSE_PANEL_WIDTH, None);

    let language = settings
        .as_ref()
        .map(|settings| settings.language)
        .unwrap_or_default();
    let s = language.strings();
    ui_kit::title(&mut commands, &font, panel, s.paused);

    let resume = pause_button(
        &mut commands,
        &font,
        panel,
        format!("{} (Esc)", s.resume),
        UiButtonRole::Primary,
        PauseAction::Resume,
    );
    pause_button(
        &mut commands,
        &font,
        panel,
        s.settings,
        UiButtonRole::Secondary,
        PauseAction::Settings,
    );
    pause_button(
        &mut commands,
        &font,
        panel,
        format!("{} (Ctrl+S)", s.save),
        UiButtonRole::Secondary,
        PauseAction::Save,
    );
    pause_button(
        &mut commands,
        &font,
        panel,
        s.main_menu,
        UiButtonRole::Secondary,
        PauseAction::MainMenu,
    );
    pause_button(
        &mut commands,
        &font,
        panel,
        s.quit_save,
        UiButtonRole::Danger,
        PauseAction::Quit,
    );
    if entering || !was_open {
        commands.entity(resume).insert(AutoFocus);
    }

    // 装饰品行放进滚动区：行数多时不再把面板顶出视口。
    let scroll = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(MENU_GAP_PX),
                flex_grow: 1.0,
                min_height: px(0.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition(scroll_pos),
            ScrollArea,
            PauseScroll,
            ChildOf(panel),
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
        ui_kit::body(&mut commands, &font, row, format!("L{layer} {current}"));
        for id in decorations.owned() {
            let Some(spec) = spec_of(id) else {
                continue; // 未知 id：不显示（存档缺失元数据视为未装备）
            };
            if spec.layer != layer {
                continue;
            }
            ui_kit::button_sized_with_action(
                &mut commands,
                &font,
                row,
                id,
                UiButtonRole::Secondary,
                Val::Auto,
                UI_BUTTON_HEIGHT_SM,
                PauseButton(PauseAction::Equip(id.clone())),
            );
        }
        ui_kit::button_sized_with_action(
            &mut commands,
            &font,
            row,
            "x",
            UiButtonRole::Secondary,
            Val::Auto,
            UI_BUTTON_HEIGHT_SM,
            PauseButton(PauseAction::Unequip(layer)),
        );
    }
}

fn pause_button(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    action: PauseAction,
) -> Entity {
    ui_kit::button_sized_with_action(
        commands,
        font,
        parent,
        label,
        role,
        px(UI_PAUSE_BUTTON_WIDTH),
        UI_BUTTON_HEIGHT,
        PauseButton(action),
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_pause_activate(
    activate: On<Activate>,
    buttons: Query<&PauseButton>,
    mut decorations: ResMut<Decorations>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut settings_return: ResMut<crate::state::SettingsReturn>,
    progress: Option<Res<LevelProgress>>,
    checkpoint: Option<Res<crate::checkpoint::ActiveCheckpoint>>,
    mode: Res<crate::state::SessionMode>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(button) = buttons.get(activate.entity) else {
        return;
    };
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
