//! 暂停菜单。所有者文档：`docs/05-architecture.md` §5.5。
//!
//! `GameState::Paused` 冻结物理与计时（`05` §5.5）。注意：编辑器也把状态停在 `Paused`，
//! 但它不会把 [`PauseMenu::open`] 置真 ⇒ 编辑态不会弹出本菜单。

use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::config::ui::{UI_BUTTON_HEIGHT, UI_PAUSE_BUTTON_WIDTH, UI_PAUSE_PANEL_WIDTH};
use crate::fonts::UiFont;
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
struct PauseButton(PauseAction);

#[derive(Component, Debug, Clone)]
enum PauseAction {
    Resume,
    Settings,
    Save,
    MainMenu,
    Quit,
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

/// 菜单开关或语言变化时重建。
fn rebuild_pause_ui(
    mut commands: Commands,
    menu: Res<PauseMenu>,
    layout: Res<ViewLayout>,
    settings: Option<Res<GameSettings>>,
    state: Res<State<GameState>>,
    font: Res<UiFont>,
    roots: Query<Entity, With<PauseRoot>>,
) {
    let settings_changed = settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    let entering = state.is_changed() && *state.get() == GameState::Paused;
    let changed = menu.is_changed() || settings_changed || layout.is_changed() || entering;
    if !changed {
        return;
    }
    let was_open = !roots.is_empty();
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
                save::persist_now(progress, checkpoint.as_ref().and_then(|point| point.0));
            }
        }
        PauseAction::MainMenu => {
            if let Some(progress) = progress.as_ref().filter(|_| !mode.is_client()) {
                save::persist_now(progress, checkpoint.as_ref().and_then(|point| point.0));
            }
            menu.open = false;
            next.set(GameState::MainMenu);
        }
        PauseAction::Quit => {
            if let Some(progress) = progress.as_ref().filter(|_| !mode.is_client()) {
                save::persist_now(progress, checkpoint.as_ref().and_then(|point| point.0));
            }
            exit.write(AppExit::Success);
        }
    }
}
