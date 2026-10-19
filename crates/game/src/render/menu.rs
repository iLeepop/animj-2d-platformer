//! 主菜单 / 关卡选择 / 设置界面。所有者文档：`docs/05-architecture.md` §5.5、
//! `docs/07-rendering.md` §7.10.5；文案与语言见 `docs/05-architecture.md` §5.14。
//!
//! 只有发布 bin 注册本插件：编辑器不经过菜单流程（`StartupFlow::Direct`，`05` §5.5）。
//! UI 由窗口相机渲染，`UiScale = k` 让逻辑坐标仍是 `VIEWPORT_W × VIEWPORT_H`（`07` §7.10.1），
//! 因此尺寸一律用 `px()`；同时窗口目标保证 `bevy_picking` 能命中按钮。

use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;

use crate::assets;
use crate::config::render::{
    MENU_FONT_PX, MENU_GAP_PX, MENU_PADDING_PX, MENU_TITLE_FONT_PX, VIEWPORT_H, VIEWPORT_W,
};
use crate::fonts::UiFont;
use crate::net::discovery::LanBrowser;
use crate::save;
use crate::settings::GameSettings;
use crate::state::{GameState, SessionMode, SettingsReturn};
use crate::world::load::{LoadMode, LoadNotice, Notice, StartRequest};

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
struct OnlineRoot;

/// 房间列表的容器：列表变化时只重建它的子节点，按钮顺序不变。
#[derive(Component)]
struct OnlineRoomList;

#[derive(Component)]
struct MenuStatusText;

#[derive(Component)]
struct MenuButton(MenuAction);

#[derive(Component, Debug, Clone)]
enum MenuAction {
    StartGame,
    LocalCoop,
    Online,
    LoadSave,
    Settings,
    Exit,
    SelectLevel(String),
    CreateRoom,
    JoinRoom {
        addr: std::net::SocketAddr,
        level_id: String,
    },
    VolumeDown,
    VolumeUp,
    ToggleFullscreen,
    ToggleLanguage,
    Back,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LevelCatalog(assets::level_ids()))
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(GameState::LevelSelect), spawn_level_select)
            .add_systems(OnEnter(GameState::OnlineMenu), spawn_online_menu)
            .add_systems(
                Update,
                (
                    handle_menu_buttons.run_if(in_state(GameState::MainMenu)),
                    handle_level_buttons.run_if(in_state(GameState::LevelSelect)),
                    (handle_online_buttons, ensure_online_rooms)
                        .chain()
                        .run_if(in_state(GameState::OnlineMenu)),
                    (handle_settings_buttons, ensure_settings_ui)
                        .chain()
                        .run_if(in_state(GameState::Settings)),
                    escape_back,
                    update_menu_status.run_if(in_state(GameState::MainMenu)),
                ),
            );
    }
}

fn spawn_main_menu(
    mut commands: Commands,
    notice: Res<LoadNotice>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
) {
    let s = settings.language.strings();
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
        font.text_font(MENU_TITLE_FONT_PX),
        TextColor(Color::WHITE),
        ChildOf(root),
    ));
    commands.spawn((
        Text::new(notice.0.text(s)),
        font.text_font(MENU_FONT_PX),
        TextColor(Color::srgb(0.9, 0.55, 0.35)),
        MenuStatusText,
        ChildOf(root),
    ));

    menu_button(
        &mut commands,
        &font,
        root,
        s.start_game,
        MenuAction::StartGame,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.local_coop,
        MenuAction::LocalCoop,
    );
    menu_button(&mut commands, &font, root, s.online, MenuAction::Online);
    menu_button(
        &mut commands,
        &font,
        root,
        s.load_save,
        MenuAction::LoadSave,
    );
    menu_button(&mut commands, &font, root, s.settings, MenuAction::Settings);
    menu_button(&mut commands, &font, root, s.exit, MenuAction::Exit);
}

fn spawn_level_select(
    mut commands: Commands,
    catalog: Res<LevelCatalog>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
) {
    let s = settings.language.strings();
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
        Text::new(s.select_level),
        font.text_font(MENU_TITLE_FONT_PX),
        TextColor(Color::WHITE),
        ChildOf(root),
    ));

    let saved = save::load_raw()
        .filter(|data| data.version == save::SAVE_VERSION)
        .map(|data| data.level_name);
    if catalog.0.is_empty() {
        commands.spawn((
            Text::new(s.no_levels),
            font.text_font(MENU_FONT_PX),
            TextColor(Color::srgb(0.9, 0.55, 0.35)),
            ChildOf(root),
        ));
    }
    for level_id in &catalog.0 {
        let label = if saved.as_deref() == Some(level_id.as_str()) {
            format!("{level_id}  {}", s.saved)
        } else {
            level_id.clone()
        };
        menu_button(
            &mut commands,
            &font,
            root,
            label,
            MenuAction::SelectLevel(level_id.clone()),
        );
    }
    menu_button(&mut commands, &font, root, s.back, MenuAction::Back);
}

/// 联机页：搜索结果列表 + 创建房间 + 返回。
fn spawn_online_menu(mut commands: Commands, settings: Res<GameSettings>, font: Res<UiFont>) {
    let s = settings.language.strings();
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
            OnlineRoot,
            DespawnOnExit(GameState::OnlineMenu),
        ))
        .id();

    commands.spawn((
        Text::new(s.online_title),
        font.text_font(MENU_TITLE_FONT_PX),
        TextColor(Color::WHITE),
        ChildOf(root),
    ));
    commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(MENU_GAP_PX),
            ..default()
        },
        OnlineRoomList,
        ChildOf(root),
    ));
    menu_button(
        &mut commands,
        &font,
        root,
        s.create_room,
        MenuAction::CreateRoom,
    );
    menu_button(&mut commands, &font, root, s.back, MenuAction::Back);
}

/// 房间集合变化时重建列表（`LanBrowser::revision` 是唯一判据）。
#[allow(clippy::too_many_arguments)]
fn ensure_online_rooms(
    mut commands: Commands,
    browser: Option<Res<LanBrowser>>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    slots: Query<Entity, With<OnlineRoomList>>,
    children: Query<(Entity, &ChildOf)>,
    mut revision: Local<Option<u64>>,
) {
    let Some(browser) = browser else {
        return;
    };
    if *revision == Some(browser.revision) {
        return;
    }
    *revision = Some(browser.revision);
    let Some(slot) = slots.iter().next() else {
        return;
    };
    for (child, parent) in &children {
        if parent.parent() == slot {
            commands.entity(child).despawn();
        }
    }

    let s = settings.language.strings();
    if browser.rooms.is_empty() {
        commands.spawn((
            Text::new(s.searching),
            font.text_font(MENU_FONT_PX),
            TextColor(Color::srgb(0.75, 0.78, 0.85)),
            ChildOf(slot),
        ));
        return;
    }
    for (addr, room) in &browser.rooms {
        let state = if room.players >= room.max_players {
            s.room_full
        } else {
            ""
        };
        let label = format!(
            "{}  {}/{}  {}  {state}",
            room.name, room.players, room.max_players, room.level_id
        );
        menu_button(
            &mut commands,
            &font,
            slot,
            label,
            MenuAction::JoinRoom {
                addr: *addr,
                level_id: room.level_id.clone(),
            },
        );
    }
}

fn handle_online_buttons(
    mut commands: Commands,
    buttons: Query<&MenuButton, Added<Pressed>>,
    mut session: ResMut<SessionMode>,
    mut next: ResMut<NextState<GameState>>,
) {
    for button in &buttons {
        match &button.0 {
            MenuAction::CreateRoom => {
                *session = SessionMode::Host;
                info!("联机：创建房间（选择关卡后开始监听）");
                next.set(GameState::LevelSelect);
            }
            MenuAction::JoinRoom { addr, level_id } => {
                *session = SessionMode::Client { server: *addr };
                info!("联机：加入房间 {addr}（关卡 {level_id}）");
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

fn menu_button(
    commands: &mut Commands,
    font: &UiFont,
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
            font.text_font(MENU_FONT_PX),
            TextColor(Color::WHITE),
        ));
}

/// 菜单动作。`StartRequest` 由这里写入，`OnEnter(Loading)` 再消费（`05` §5.5）。
#[allow(clippy::too_many_arguments)]
fn handle_menu_buttons(
    mut commands: Commands,
    buttons: Query<&MenuButton, Added<Pressed>>,
    catalog: Res<LevelCatalog>,
    mut notice: ResMut<LoadNotice>,
    mut return_state: ResMut<SettingsReturn>,
    mut session: ResMut<SessionMode>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for button in &buttons {
        match &button.0 {
            MenuAction::StartGame => {
                *session = SessionMode::Solo;
                notice.0 = Notice::None;
                next.set(GameState::LevelSelect);
            }
            MenuAction::LocalCoop => {
                *session = SessionMode::LocalCoop;
                notice.0 = Notice::None;
                info!("本地双人：P1 方向键 + Z/E，P2 WASD + W/空格 + F");
                next.set(GameState::LevelSelect);
            }
            MenuAction::Online => {
                notice.0 = Notice::None;
                next.set(GameState::OnlineMenu);
            }
            MenuAction::LoadSave => match save::load_raw() {
                None => {
                    warn!("读取存档：没有存档文件");
                    notice.0 = Notice::NoSave;
                }
                Some(data) if data.version != save::SAVE_VERSION => {
                    warn!("读取存档：版本 {} 不符", data.version);
                    notice.0 = Notice::VersionMismatch;
                }
                Some(data) if !catalog.0.contains(&data.level_name) => {
                    warn!("读取存档：关卡 {} 不在 assets/levels", data.level_name);
                    notice.0 = Notice::LevelNotFound;
                }
                Some(data) => {
                    info!("读取存档：关卡 {}", data.level_name);
                    notice.0 = Notice::None;
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
                notice.0 = Notice::None;
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
            MenuAction::ToggleLanguage => settings.toggle_language(),
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
        GameState::LevelSelect | GameState::OnlineMenu => next.set(GameState::MainMenu),
        GameState::Settings => next.set(return_state.0),
        _ => {}
    }
}

fn update_menu_status(
    notice: Res<LoadNotice>,
    settings: Res<GameSettings>,
    mut texts: Query<&mut Text, With<MenuStatusText>>,
) {
    let text = notice.0.text(settings.language.strings()).to_string();
    for mut node in &mut texts {
        if node.0 != text {
            node.0 = text.clone();
        }
    }
}

/// 设置变化时整块重建（与暂停菜单同一策略：一次性重建，避免每帧 diff UI 树）。
fn ensure_settings_ui(
    mut commands: Commands,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    roots: Query<Entity, With<SettingsRoot>>,
) {
    if !roots.is_empty() && !settings.is_changed() {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }

    let s = settings.language.strings();
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
        Text::new(s.settings),
        font.text_font(MENU_TITLE_FONT_PX),
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
        Text::new(s.volume),
        font.text_font(MENU_FONT_PX),
        TextColor(Color::WHITE),
        ChildOf(volume_row),
    ));
    menu_button(
        &mut commands,
        &font,
        volume_row,
        "-",
        MenuAction::VolumeDown,
    );
    commands.spawn((
        Text::new(format!("{}%", settings.volume_percent())),
        font.text_font(MENU_FONT_PX),
        TextColor(Color::WHITE),
        ChildOf(volume_row),
    ));
    menu_button(&mut commands, &font, volume_row, "+", MenuAction::VolumeUp);

    menu_button(
        &mut commands,
        &font,
        root,
        format!(
            "{}  {}",
            s.fullscreen,
            if settings.fullscreen { s.on } else { s.off }
        ),
        MenuAction::ToggleFullscreen,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        format!("{}  {}", s.language, settings.language.self_name()),
        MenuAction::ToggleLanguage,
    );
    menu_button(&mut commands, &font, root, s.back, MenuAction::Back);
}
