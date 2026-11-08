//! 屏幕空间 HUD。所有者文档：`docs/07-rendering.md` §7.10。
//!
//! HUD 走 **UI 窗口相机**（`IsDefaultUiCamera` 挂在它上面），`UiScale = k` 让逻辑坐标仍然是
//! `ViewLayout` 的尺寸（单人 320×320，本地双人 640×320；`07` §7.10.1）。因此所有尺寸与字号都用 `px()` 的整数值，
//! 禁止 `vw()` / `vh()` / `percent()`。
//!
//! 显示内容：本局计时、焦点玩家的当前道具、一次性提示条、终点结算面板。

use bevy::prelude::*;

use crate::config::render::HUD_TOAST_SECS;
use crate::config::ui::{
    HUD_FONT_PX, HUD_MARGIN_PX, UI_COMPLETION_HEIGHT, UI_COMPLETION_WIDTH, UI_HUD_BG,
    UI_HUD_PILL_MAX_WIDTH, UI_ORANGE, UI_PAPER, UI_PLUM, UI_SPACE_SM, UI_SPACE_XS, UI_TOAST_BOTTOM,
};
use crate::fonts::UiFont;
use crate::player::items::{CarriedItem, ItemPicked};
use crate::player::{PlayerId, PlayerMotion};
use crate::render::pixel_camera::ViewLayout;
use crate::render::ui_kit;
use crate::run::{PlayerFinished, RunState};
use crate::settings::GameSettings;
use crate::state::GameState;

/// HUD 文本节点标记。
///
/// 必须有这个过滤条件：项目里还有一个调试文本节点（`render::debug`），
/// 不加过滤的 `Single<&mut Text>` 会因匹配到两个实体而**静默跳过整个系统**（HUD 永远停在生成时的初值）。
#[derive(Component)]
struct HudText;

/// HUD 左上角药丸根。
#[derive(Component)]
struct HudPill;

/// 一次性的提示条（拾取道具、到达终点）。同一时刻只显示最新一条。
#[derive(Resource, Debug, Default)]
pub struct HudToast {
    pub text: String,
    remaining: f32,
}

impl HudToast {
    fn show(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.remaining = HUD_TOAST_SECS;
    }
}

#[derive(Component)]
struct ToastRoot;

#[derive(Component)]
struct ToastText;

/// 结算面板根。
#[derive(Component)]
struct CompletionPanel;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudToast>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (collect_toasts, tick_toast, update_hud, update_toast_ui).chain(),
            )
            .add_systems(OnEnter(GameState::GameComplete), spawn_completion);
    }
}

fn spawn_hud(mut commands: Commands, font: Res<UiFont>) {
    let pill = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(HUD_MARGIN_PX),
                top: px(HUD_MARGIN_PX),
                max_width: px(UI_HUD_PILL_MAX_WIDTH),
                padding: UiRect::axes(px(UI_SPACE_SM), px(UI_SPACE_XS)),
                border: UiRect::all(px(1.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(UI_HUD_BG),
            BorderColor::all(UI_PLUM),
            HudPill,
        ))
        .id();
    commands.spawn((
        Text::new(""),
        font.text_font(HUD_FONT_PX),
        TextColor(UI_PAPER),
        HudText,
        ChildOf(pill),
    ));

    let toast_root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                bottom: px(UI_TOAST_BOTTOM),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(20),
            ToastRoot,
        ))
        .id();
    let toast_pill = commands
        .spawn((
            Node {
                padding: UiRect::axes(px(UI_SPACE_SM), px(UI_SPACE_XS)),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BackgroundColor(UI_HUD_BG),
            BorderColor::all(UI_ORANGE),
            ChildOf(toast_root),
        ))
        .id();
    commands.spawn((
        Text::new(""),
        font.text_font(HUD_FONT_PX),
        TextColor(UI_PAPER),
        ToastText,
        ChildOf(toast_pill),
    ));
}

/// 拾取道具 / 到达终点消息 → 提示条。
fn collect_toasts(
    mut picked: MessageReader<ItemPicked>,
    mut finished: MessageReader<PlayerFinished>,
    settings: Res<GameSettings>,
    mut toast: ResMut<HudToast>,
) {
    let s = settings.language.strings();
    for event in picked.read() {
        toast.show(format!(
            "{}：{}",
            s.item_gained,
            settings.language.item_name(event.kind)
        ));
    }
    for event in finished.read() {
        toast.show(format!(
            "{} {} {} {:.2}{}",
            s.player,
            event.player.0 + 1,
            s.finish_title,
            event.time,
            s.seconds
        ));
    }
}

fn tick_toast(time: Res<Time>, mut toast: ResMut<HudToast>) {
    if toast.remaining <= 0.0 {
        return;
    }
    toast.remaining = (toast.remaining - time.delta_secs()).max(0.0);
    if toast.remaining == 0.0 {
        toast.text.clear();
    }
}

/// 只显示**玩法信息**：计时 + 焦点玩家当前携带的道具。
fn update_hud(
    run: Option<Res<RunState>>,
    settings: Res<GameSettings>,
    players: Query<(&PlayerId, &CarriedItem), With<PlayerMotion>>,
    mut text: Single<&mut Text, With<HudText>>,
    mut pill: Single<&mut Node, With<HudPill>>,
) {
    let s = settings.language.strings();
    let focus_item = players
        .iter()
        .min_by_key(|(id, _)| *id)
        .and_then(|(_, item)| item.0);
    let elapsed = run.as_ref().map(|run| run.elapsed).unwrap_or(0.0);
    let name = focus_item
        .map(|kind| settings.language.item_name(kind))
        .unwrap_or(s.no_item);
    let next = format!("{} {:.2}{}   {}", s.time, elapsed, s.seconds, name);

    if text.0 != next {
        text.0 = next.clone();
    }
    let display = if next.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if pill.display != display {
        pill.display = display;
    }
}

fn update_toast_ui(
    toast: Res<HudToast>,
    mut root: Single<&mut Node, With<ToastRoot>>,
    mut text: Single<&mut Text, With<ToastText>>,
) {
    let display = if toast.text.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if root.display != display {
        root.display = display;
    }
    if text.0 != toast.text {
        text.0 = toast.text.clone();
    }
}

/// 进入 `GameComplete` 时展示结算：每名玩家的通关时间与分数（`01` §1.6）。
fn spawn_completion(
    mut commands: Commands,
    run: Option<Res<RunState>>,
    players: Query<&PlayerId, With<PlayerMotion>>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
    layout: Res<ViewLayout>,
) {
    let s = settings.language.strings();
    let mut ids: Vec<u8> = players.iter().map(|id| id.0).collect();
    ids.sort_unstable();
    if ids.is_empty() {
        ids.push(0);
    }

    let root = ui_kit::scrim_screen(&mut commands, &layout, 50);
    commands
        .entity(root)
        .insert((CompletionPanel, DespawnOnExit(GameState::GameComplete)));
    let panel = ui_kit::panel(
        &mut commands,
        root,
        UI_COMPLETION_WIDTH,
        Some(UI_COMPLETION_HEIGHT),
    );
    let content = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
                row_gap: px(UI_SPACE_SM),
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    ui_kit::title(&mut commands, &font, content, s.finish_title);
    for id in ids {
        let player = PlayerId(id);
        let line = match run.as_ref().and_then(|run| run.time_of(player)) {
            Some(time) => format!(
                "{} {}  {} {:.2}{}  {} {}",
                s.player,
                id + 1,
                s.finish_line,
                time,
                s.seconds,
                run.as_ref()
                    .map(|run| run.score_of(player).unwrap_or(0))
                    .unwrap_or(0),
                s.points
            ),
            None => format!("{} {}  {}", s.player, id + 1, s.unfinished),
        };
        ui_kit::body_center(&mut commands, &font, content, line);
    }
}
