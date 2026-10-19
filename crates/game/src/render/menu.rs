//! 主菜单 / 关卡选择 / 设置界面。所有者文档：`docs/05-architecture.md` §5.5、`docs/07-rendering.md` §7.10.5。
//!
//! 只有发布 bin 注册本插件：编辑器不经过菜单流程（`StartupFlow::Direct`，`05` §5.5）。
//! UI 由内层相机渲染，坐标系是 `VIEWPORT_W × VIEWPORT_H`（`07` §7.10），因此尺寸一律用 `px()`。
//!
//! 文本只能是 ASCII（Bevy 默认字体缺 CJK 字形，会把该行后续文本一起丢掉，`10` §10.3.1 E7）。

use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;

use crate::assets;
use crate::config::render::{
    MENU_FONT_PX, MENU_GAP_PX, MENU_PADDING_PX, MENU_TITLE_FONT_PX, VIEWPORT_H, VIEWPORT_W,
};
use crate::save;
use crate::settings::GameSettings;
use crate::state::{GameState, SettingsReturn};
use crate::world::load::{LoadMode, LoadNotice, StartRequest};

/// 扫描到的关卡 id（`assets/levels/*.level.ron` 的文件名去后缀）。
///
/// 启动时扫描一次：关卡文件在运行中新增不会出现在列表里，需要重启（热重载仍作用于当前关卡）。
#[derive(Resource, Debug, Default, Clone)]
pub struct LevelCatalog(pub Vec<String>);

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct LevelRoot;

#[derive(Component)]
struct SettingsRoot;

#[derive(Component)]
struct MenuStatusText;

#[derive(Component)]
struct MenuButton(MenuAction);

#[derive(Component, Debug, Clone)]
enum MenuAction {
    StartGame,
    LoadSave,
    Settings,
    Exit,
    SelectLevel(String),
    VolumeDown,
    VolumeUp,
    ToggleFullscreen,
    Back,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LevelCatalog(assets::level_ids()))
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(GameState::LevelSelect), spawn_level_select)
            .add_systems(
                Update,
                (
                    handle_menu_buttons.run_if(in_state(GameState::MainMenu)),
                    handle_level_buttons.run_if(in_state(GameState::LevelSelect)),
                    (handle_settings_buttons, ensure_settings_ui)
                        .chain()
                        .run_if(in_state(GameState::Settings)),
                    escape_back,
                    update_menu_status.run_if(in_state(GameState::MainMenu)),
                ),
            );
    }
}

fn spawn_main_menu(mut commands: Commands, notice: Res<LoadNotice>) {
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: px(VIEWPORT_W as f32),
                height: px(VIEWPORT_H as f32),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(MENU_PADDING_PX * 2.0)),
                row_gap: px(MENU_GAP_PX),
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.05, 0.09)),
            GlobalZIndex(100),
            MenuRoot,
            DespawnOnExit(GameState::MainMenu),
        ))
        .id();

    commands.spawn((
        Text::new("ANIMJ"),
        TextFont {
            font_size: FontSize::Px(MENU_TITLE_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        ChildOf(root),
    ));
    commands.spawn((
        Text::new(notice.0.clone()),
        TextFont {
            font_size: FontSize::Px(MENU_FONT_PX),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.55, 0.35)),
        MenuStatusText,
        ChildOf(root),
    ));

    menu_button(&mut commands, root, "START GAME", MenuAction::StartGame);
    menu_button(&mut commands, root, "LOAD SAVE", MenuAction::LoadSave);
    menu_button(&mut commands, root, "SETTINGS", MenuAction::Settings);
    menu_button(&mut commands, root, "EXIT", MenuAction::Exit);
}

fn spawn_level_select(mut commands: Commands, catalog: Res<LevelCatalog>) {
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: px(VIEWPORT_W as f32),
                height: px(VIEWPORT_H as f32),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(MENU_PADDING_PX * 2.0)),
                row_gap: px(MENU_GAP_PX),
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.05, 0.09)),
            GlobalZIndex(100),
            LevelRoot,
            DespawnOnExit(GameState::LevelSelect),
        ))
        .id();

    commands.spawn((
        Text::new("SELECT LEVEL"),
        TextFont {
            font_size: FontSize::Px(MENU_TITLE_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        ChildOf(root),
    ));

    let saved = save::load_raw()
        .filter(|data| data.version == save::SAVE_VERSION)
        .map(|data| data.level_name);
    if catalog.0.is_empty() {
        commands.spawn((
            Text::new("NO LEVELS FOUND"),
            TextFont {
                font_size: FontSize::Px(MENU_FONT_PX),
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.55, 0.35)),
            ChildOf(root),
        ));
    }
    for level_id in &catalog.0 {
        let label = if saved.as_deref() == Some(level_id.as_str()) {
            format!("{level_id}  (SAVED)")
        } else {
            level_id.clone()
        };
        menu_button(
            &mut commands,
            root,
            label,
            MenuAction::SelectLevel(level_id.clone()),
        );
    }
    menu_button(&mut commands, root, "BACK", MenuAction::Back);
}

fn menu_button(
    commands: &mut Commands,
    parent: Entity,
    label: impl Into<String>,
    action: MenuAction,
) {
    commands
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(px(8.0), px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.18, 0.20, 0.28)),
            MenuButton(action),
            ChildOf(parent),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(MENU_FONT_PX),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

/// 菜单动作。`StartRequest` 由这里写入，`OnEnter(Loading)` 再消费（`05` §5.5）。
fn handle_menu_buttons(
    mut commands: Commands,
    buttons: Query<&MenuButton, Added<Pressed>>,
    catalog: Res<LevelCatalog>,
    mut notice: ResMut<LoadNotice>,
    mut return_state: ResMut<SettingsReturn>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for button in &buttons {
        match &button.0 {
            MenuAction::StartGame => {
                notice.0.clear();
                next.set(GameState::LevelSelect);
            }
            MenuAction::LoadSave => match save::load_raw() {
                None => {
                    warn!("读取存档：没有存档文件");
                    notice.0 = "NO SAVE DATA".to_string();
                }
                Some(data) if data.version != save::SAVE_VERSION => {
                    warn!("读取存档：版本 {} 不符", data.version);
                    notice.0 = "SAVE VERSION MISMATCH".to_string();
                }
                Some(data) if !catalog.0.contains(&data.level_name) => {
                    warn!("读取存档：关卡 {} 不在 assets/levels", data.level_name);
                    notice.0 = "SAVE LEVEL NOT FOUND".to_string();
                }
                Some(data) => {
                    info!("读取存档：关卡 {}", data.level_name);
                    notice.0.clear();
                    commands.insert_resource(StartRequest {
                        level_id: data.level_name,
                        mode: LoadMode::Continue,
                    });
                    next.set(GameState::Loading);
                }
            },
            MenuAction::Settings => {
                return_state.0 = GameState::MainMenu;
                next.set(GameState::Settings);
            }
            MenuAction::Exit => {
                info!("退出游戏");
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

fn handle_level_buttons(
    mut commands: Commands,
    buttons: Query<&MenuButton, Added<Pressed>>,
    mut notice: ResMut<LoadNotice>,
    mut next: ResMut<NextState<GameState>>,
) {
    for button in &buttons {
        match &button.0 {
            MenuAction::SelectLevel(level_id) => {
                info!("开始游戏：关卡 {level_id}");
                notice.0.clear();
                commands.insert_resource(StartRequest {
                    level_id: level_id.clone(),
                    mode: LoadMode::NewGame,
                });
                next.set(GameState::Loading);
            }
            MenuAction::Back => next.set(GameState::MainMenu),
            _ => {}
        }
    }
}

fn handle_settings_buttons(
    buttons: Query<&MenuButton, Added<Pressed>>,
    mut settings: ResMut<GameSettings>,
    return_state: Res<SettingsReturn>,
    mut next: ResMut<NextState<GameState>>,
) {
    for button in &buttons {
        match &button.0 {
            MenuAction::VolumeDown => settings.adjust_volume(-1),
            MenuAction::VolumeUp => settings.adjust_volume(1),
            MenuAction::ToggleFullscreen => settings.fullscreen = !settings.fullscreen,
            MenuAction::Back => next.set(return_state.0),
            _ => {}
        }
    }
}

/// Esc 在子菜单里返回上一层；主菜单不响应（避免误触退出）。
fn escape_back(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    return_state: Res<SettingsReturn>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match state.get() {
        GameState::LevelSelect => next.set(GameState::MainMenu),
        GameState::Settings => next.set(return_state.0),
        _ => {}
    }
}

fn update_menu_status(notice: Res<LoadNotice>, mut texts: Query<&mut Text, With<MenuStatusText>>) {
    for mut text in &mut texts {
        if text.0 != notice.0 {
            text.0 = notice.0.clone();
        }
    }
}

/// 设置变化时整块重建（与暂停菜单同一策略：一次性重建，避免每帧 diff UI 树）。
fn ensure_settings_ui(
    mut commands: Commands,
    settings: Res<GameSettings>,
    roots: Query<Entity, With<SettingsRoot>>,
) {
    if !roots.is_empty() && !settings.is_changed() {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: px(VIEWPORT_W as f32),
                height: px(VIEWPORT_H as f32),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(MENU_PADDING_PX * 2.0)),
                row_gap: px(MENU_GAP_PX),
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.05, 0.09)),
            GlobalZIndex(100),
            SettingsRoot,
            DespawnOnExit(GameState::Settings),
        ))
        .id();

    commands.spawn((
        Text::new("SETTINGS"),
        TextFont {
            font_size: FontSize::Px(MENU_TITLE_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        ChildOf(root),
    ));

    let volume_row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4.0),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Text::new("VOLUME"),
        TextFont {
            font_size: FontSize::Px(MENU_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        ChildOf(volume_row),
    ));
    menu_button(&mut commands, volume_row, "-", MenuAction::VolumeDown);
    commands.spawn((
        Text::new(format!("{}%", settings.volume_percent())),
        TextFont {
            font_size: FontSize::Px(MENU_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        ChildOf(volume_row),
    ));
    menu_button(&mut commands, volume_row, "+", MenuAction::VolumeUp);

    menu_button(
        &mut commands,
        root,
        format!(
            "FULLSCREEN  {}",
            if settings.fullscreen { "ON" } else { "OFF" }
        ),
        MenuAction::ToggleFullscreen,
    );
    menu_button(&mut commands, root, "BACK", MenuAction::Back);
}
