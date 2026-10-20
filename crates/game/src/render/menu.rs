//! 主菜单 / 关卡选择 / 联机 / 设置界面。所有者文档：`docs/05-architecture.md` §5.5、
//! `docs/07-rendering.md` §7.10.5；文案与语言见 `docs/05-architecture.md` §5.14。
//!
//! 只有发布 bin 注册本插件：编辑器不经过菜单流程（`StartupFlow::Direct`，`05` §5.5）。
//! UI 由窗口相机渲染，`UiScale = k` 让逻辑坐标仍是 `ViewLayout` 的尺寸（单人 320×320，本地双人 640×320；`07` §7.10.1），
//! 因此尺寸一律用 `px()`；同时窗口目标保证 `bevy_picking` 能命中按钮。

use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::assets;
use crate::config::ui::{
    MENU_FONT_PX, MENU_GAP_PX, MENU_PADDING_PX, UI_BUTTON_HEIGHT, UI_BUTTON_HEIGHT_SM,
    UI_BUTTON_WIDTH, UI_GOLD, UI_LEVEL_CARD_WIDTH, UI_ONLINE_ROOM_WIDTH, UI_ORANGE, UI_PAPER,
    UI_SETTINGS_CONTENT_WIDTH, UI_SETTINGS_PANEL_WIDTH, UI_SETTINGS_TOGGLE_WIDTH,
    UI_SETTINGS_VALUE_WIDTH, UI_SPACE_SM, UI_SPACE_XS,
};
use crate::fonts::UiFont;
use crate::net::discovery::LanBrowser;
use crate::render::pixel_camera::ViewLayout;
use crate::render::ui_kit::{self, UiButtonRole};
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
struct OnlineRoot;

/// 房间列表的容器：列表变化时只重建它的子节点，按钮顺序不变。
#[derive(Component)]
struct OnlineRoomList;

#[derive(Component)]
struct MenuStatusText;

#[derive(Component)]
struct MenuButton(MenuAction);

#[derive(Component)]
struct SettingsTitle;

#[derive(Component)]
struct SettingsVolumeLabel;

#[derive(Component)]
struct SettingsVolumeValue;

#[derive(Component)]
struct SettingsFullscreenLabel;

#[derive(Component)]
struct SettingsFullscreenValue;

#[derive(Component)]
struct SettingsLanguageLabel;

#[derive(Component)]
struct SettingsLanguageValue;

#[derive(Component)]
struct SettingsBackLabel;

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
            .add_observer(handle_menu_activate)
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(GameState::LevelSelect), spawn_level_select)
            .add_systems(OnEnter(GameState::OnlineMenu), spawn_online_menu)
            .add_systems(OnEnter(GameState::Settings), spawn_settings)
            .add_systems(
                Update,
                (
                    ensure_online_rooms.run_if(in_state(GameState::OnlineMenu)),
                    sync_settings_ui.run_if(in_state(GameState::Settings)),
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
    layout: Res<ViewLayout>,
) {
    let s = settings.language.strings();
    let root = ui_kit::screen(&mut commands, &layout, 100);
    commands
        .entity(root)
        .insert((MenuRoot, DespawnOnExit(GameState::MainMenu)));

    let header = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(UI_SPACE_XS),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    ui_kit::title(&mut commands, &font, header, "ANIMJ");
    commands.spawn((
        Node {
            width: px(48.0),
            height: px(2.0),
            ..default()
        },
        BackgroundColor(UI_GOLD),
        ChildOf(header),
    ));

    let status = notice.0.text(s);
    let status_text = ui_kit::text_with_marker(
        &mut commands,
        &font,
        root,
        status,
        MENU_FONT_PX,
        UI_ORANGE,
        MenuStatusText,
    );
    commands.entity(status_text).insert((
        Node {
            max_width: px(UI_BUTTON_WIDTH),
            display: if status.is_empty() {
                Display::None
            } else {
                Display::Flex
            },
            ..default()
        },
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
    ));

    let first = menu_button(
        &mut commands,
        &font,
        root,
        s.start_game,
        UiButtonRole::Primary,
        MenuAction::StartGame,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.local_coop,
        UiButtonRole::Secondary,
        MenuAction::LocalCoop,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.online,
        UiButtonRole::Secondary,
        MenuAction::Online,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.load_save,
        UiButtonRole::Secondary,
        MenuAction::LoadSave,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.settings,
        UiButtonRole::Secondary,
        MenuAction::Settings,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.exit,
        UiButtonRole::Danger,
        MenuAction::Exit,
    );

    let footer = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(MENU_PADDING_PX),
                right: px(MENU_PADDING_PX),
                bottom: px(UI_SPACE_SM),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    ui_kit::muted(
        &mut commands,
        &font,
        footer,
        format!("v{}", env!("CARGO_PKG_VERSION")),
    );
    ui_kit::muted(&mut commands, &font, footer, s.ui_hint);

    commands.entity(first).insert(AutoFocus);
}

fn spawn_level_select(
    mut commands: Commands,
    catalog: Res<LevelCatalog>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    layout: Res<ViewLayout>,
) {
    let s = settings.language.strings();
    let root = ui_kit::screen(&mut commands, &layout, 100);
    commands
        .entity(root)
        .insert((LevelRoot, DespawnOnExit(GameState::LevelSelect)));

    ui_kit::title(&mut commands, &font, root, s.select_level);

    let saved = save::load_raw()
        .filter(|data| data.version == save::SAVE_VERSION)
        .map(|data| data.level_name);

    if catalog.0.is_empty() {
        ui_kit::text_with_marker(
            &mut commands,
            &font,
            root,
            s.no_levels,
            MENU_FONT_PX,
            UI_ORANGE,
            (),
        );
    }

    let mut first = None;
    if !catalog.0.is_empty() {
        let grid = commands
            .spawn((
                Node {
                    width: px(UI_LEVEL_CARD_WIDTH * 2.0 + UI_SPACE_SM),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: px(UI_SPACE_SM),
                    row_gap: px(UI_SPACE_SM),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        for level_id in &catalog.0 {
            let subtitle =
                (saved.as_deref() == Some(level_id.as_str())).then(|| s.saved.to_string());
            let card = ui_kit::card_with_action(
                &mut commands,
                &font,
                grid,
                display_level_id(level_id),
                subtitle,
                UiButtonRole::Secondary,
                MenuButton(MenuAction::SelectLevel(level_id.clone())),
            );
            first.get_or_insert(card);
        }
    }

    let back = menu_button(
        &mut commands,
        &font,
        root,
        s.back,
        UiButtonRole::Secondary,
        MenuAction::Back,
    );
    commands.entity(first.unwrap_or(back)).insert(AutoFocus);
}

/// 联机页：搜索结果列表 + 创建房间 + 返回。
fn spawn_online_menu(
    mut commands: Commands,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    layout: Res<ViewLayout>,
) {
    let s = settings.language.strings();
    let root = ui_kit::screen(&mut commands, &layout, 100);
    commands
        .entity(root)
        .insert((OnlineRoot, DespawnOnExit(GameState::OnlineMenu)));

    ui_kit::title(&mut commands, &font, root, s.online_title);

    commands.spawn((
        Node {
            width: px(UI_ONLINE_ROOM_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(MENU_GAP_PX),
            ..default()
        },
        OnlineRoomList,
        ChildOf(root),
    ));

    let create = menu_button(
        &mut commands,
        &font,
        root,
        s.create_room,
        UiButtonRole::Primary,
        MenuAction::CreateRoom,
    );
    menu_button(
        &mut commands,
        &font,
        root,
        s.back,
        UiButtonRole::Secondary,
        MenuAction::Back,
    );
    commands.entity(create).insert(AutoFocus);
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
        let text = ui_kit::muted(&mut commands, &font, slot, s.searching);
        commands.entity(text).insert((
            Node {
                width: px(UI_ONLINE_ROOM_WIDTH),
                ..default()
            },
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
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
        ui_kit::button_sized_with_action(
            &mut commands,
            &font,
            slot,
            label,
            UiButtonRole::Secondary,
            px(UI_ONLINE_ROOM_WIDTH),
            UI_BUTTON_HEIGHT,
            MenuButton(MenuAction::JoinRoom {
                addr: *addr,
                level_id: room.level_id.clone(),
            }),
        );
    }
}

fn spawn_settings(
    mut commands: Commands,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    layout: Res<ViewLayout>,
) {
    let s = settings.language.strings();
    let root = ui_kit::screen(&mut commands, &layout, 100);
    commands
        .entity(root)
        .insert(DespawnOnExit(GameState::Settings));

    let panel = ui_kit::panel(&mut commands, root, UI_SETTINGS_PANEL_WIDTH, None);
    ui_kit::title_with_marker(&mut commands, &font, panel, s.settings, SettingsTitle);

    let volume_row = settings_row(&mut commands, panel);
    let volume_label = ui_kit::body_with_marker(
        &mut commands,
        &font,
        volume_row,
        s.volume,
        SettingsVolumeLabel,
    );
    commands.entity(volume_label).insert(Node {
        flex_grow: 1.0,
        ..default()
    });
    let volume_down = ui_kit::button_sized_with_action(
        &mut commands,
        &font,
        volume_row,
        "-",
        UiButtonRole::Secondary,
        Val::Auto,
        UI_BUTTON_HEIGHT_SM,
        MenuButton(MenuAction::VolumeDown),
    );
    let volume_value = ui_kit::text_with_marker(
        &mut commands,
        &font,
        volume_row,
        format!("{}%", settings.volume_percent()),
        MENU_FONT_PX,
        UI_PAPER,
        SettingsVolumeValue,
    );
    commands.entity(volume_value).insert((
        Node {
            width: px(UI_SETTINGS_VALUE_WIDTH),
            ..default()
        },
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
    ));
    ui_kit::button_sized_with_action(
        &mut commands,
        &font,
        volume_row,
        "+",
        UiButtonRole::Secondary,
        Val::Auto,
        UI_BUTTON_HEIGHT_SM,
        MenuButton(MenuAction::VolumeUp),
    );

    let fullscreen_row = settings_row(&mut commands, panel);
    let fullscreen_label = ui_kit::body_with_marker(
        &mut commands,
        &font,
        fullscreen_row,
        s.fullscreen,
        SettingsFullscreenLabel,
    );
    commands.entity(fullscreen_label).insert(Node {
        flex_grow: 1.0,
        ..default()
    });
    ui_kit::button_sized_with_action_label(
        &mut commands,
        &font,
        fullscreen_row,
        if settings.fullscreen { s.on } else { s.off },
        UiButtonRole::Secondary,
        px(UI_SETTINGS_TOGGLE_WIDTH),
        UI_BUTTON_HEIGHT_SM,
        MenuButton(MenuAction::ToggleFullscreen),
        SettingsFullscreenValue,
    );

    let language_row = settings_row(&mut commands, panel);
    let language_label = ui_kit::body_with_marker(
        &mut commands,
        &font,
        language_row,
        s.language,
        SettingsLanguageLabel,
    );
    commands.entity(language_label).insert(Node {
        flex_grow: 1.0,
        ..default()
    });
    ui_kit::button_sized_with_action_label(
        &mut commands,
        &font,
        language_row,
        settings.language.self_name(),
        UiButtonRole::Secondary,
        px(UI_SETTINGS_TOGGLE_WIDTH),
        UI_BUTTON_HEIGHT_SM,
        MenuButton(MenuAction::ToggleLanguage),
        SettingsLanguageValue,
    );

    ui_kit::button_sized_with_action_label(
        &mut commands,
        &font,
        panel,
        s.back,
        UiButtonRole::Secondary,
        px(UI_SETTINGS_CONTENT_WIDTH),
        UI_BUTTON_HEIGHT,
        MenuButton(MenuAction::Back),
        SettingsBackLabel,
    );

    commands.entity(volume_down).insert(AutoFocus);
}

fn settings_row(commands: &mut Commands, parent: Entity) -> Entity {
    commands
        .spawn((
            Node {
                width: px(UI_SETTINGS_CONTENT_WIDTH),
                height: px(UI_BUTTON_HEIGHT),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(UI_SPACE_SM),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn menu_button(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    action: MenuAction,
) -> Entity {
    ui_kit::button_with_action(commands, font, parent, label, role, MenuButton(action))
}

/// 业务动作统一由 `Activate` 触发：鼠标松开或焦点按钮 Enter/Space。
/// `Added<Pressed>` 会在按下瞬间触发且键盘不触发，禁止再用。
#[allow(clippy::too_many_arguments)]
fn handle_menu_activate(
    activate: On<Activate>,
    buttons: Query<&MenuButton>,
    mut commands: Commands,
    catalog: Res<LevelCatalog>,
    mut notice: ResMut<LoadNotice>,
    mut return_state: ResMut<SettingsReturn>,
    mut session: ResMut<SessionMode>,
    mut settings: ResMut<GameSettings>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(button) = buttons.get(activate.entity) else {
        return;
    };
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
        MenuAction::SelectLevel(level_id) => {
            info!("开始游戏：关卡 {level_id}");
            notice.0 = Notice::None;
            commands.insert_resource(StartRequest {
                level_id: level_id.clone(),
                mode: LoadMode::NewGame,
            });
            next.set(GameState::Loading);
        }
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
        MenuAction::VolumeDown => settings.adjust_volume(-1),
        MenuAction::VolumeUp => settings.adjust_volume(1),
        MenuAction::ToggleFullscreen => settings.fullscreen = !settings.fullscreen,
        MenuAction::ToggleLanguage => settings.toggle_language(),
        MenuAction::Back => match state.get() {
            GameState::Settings => next.set(return_state.0),
            _ => next.set(GameState::MainMenu),
        },
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
    mut texts: Query<(&mut Text, &mut Node), With<MenuStatusText>>,
) {
    let text = notice.0.text(settings.language.strings()).to_string();
    let display = if text.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    for (mut text_node, mut node) in &mut texts {
        if text_node.0 != text {
            text_node.0 = text.clone();
        }
        if node.display != display {
            node.display = display;
        }
    }
}

/// 设置变化时只更新文本，不重建 UI；焦点、hover、按钮状态因此保持。
///
/// 用一个查询 + `Option<&Marker>` 而不是多个 `Query<&mut Text, With<..>>`：
/// Bevy 无法证明不同 `With` 过滤条件互斥，会触发 B0001（同一 `Text` 的可变访问冲突）。
#[allow(clippy::type_complexity)]
fn sync_settings_ui(
    settings: Res<GameSettings>,
    mut texts: Query<(
        &mut Text,
        Option<&SettingsTitle>,
        Option<&SettingsVolumeLabel>,
        Option<&SettingsVolumeValue>,
        Option<&SettingsFullscreenLabel>,
        Option<&SettingsFullscreenValue>,
        Option<&SettingsLanguageLabel>,
        Option<&SettingsLanguageValue>,
        Option<&SettingsBackLabel>,
    )>,
) {
    if !settings.is_changed() {
        return;
    }
    let s = settings.language.strings();
    let volume_value = format!("{}%", settings.volume_percent());
    let fullscreen_value = if settings.fullscreen { s.on } else { s.off };
    for (
        mut text,
        title,
        volume_label,
        volume_value_marker,
        fullscreen_label,
        fullscreen_value_marker,
        language_label,
        language_value,
        back_label,
    ) in &mut texts
    {
        let value = if title.is_some() {
            s.settings
        } else if volume_label.is_some() {
            s.volume
        } else if volume_value_marker.is_some() {
            volume_value.as_str()
        } else if fullscreen_label.is_some() {
            s.fullscreen
        } else if fullscreen_value_marker.is_some() {
            fullscreen_value
        } else if language_label.is_some() {
            s.language
        } else if language_value.is_some() {
            settings.language.self_name()
        } else if back_label.is_some() {
            s.back
        } else {
            continue;
        };
        if text.0 != value {
            text.0 = value.to_string();
        }
    }
}

fn display_level_id(id: &str) -> String {
    id.replace('_', " ").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::display_level_id;

    #[test]
    fn level_id_display_is_uppercase_with_spaces() {
        assert_eq!(display_level_id("world_01"), "WORLD 01");
        assert_eq!(display_level_id("single"), "SINGLE");
    }
}
