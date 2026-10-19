//! 屏幕空间 HUD。所有者文档：`docs/07-rendering.md` §7.10。
//!
//! HUD 走 **UI 窗口相机**（`IsDefaultUiCamera` 挂在它上面），`UiScale = k` 让逻辑坐标仍然是
//! `VIEWPORT_W × VIEWPORT_H`（`07` §7.10.1）。因此所有尺寸与字号都用 `px()` 的整数值，
//! 禁止 `vw()` / `vh()` / `percent()`。

use crate::assets::AssetRoot;
use crate::config::render::HUD_TOAST_SECS;
use crate::fonts::UiFont;
use crate::player::decor::Decorations;
use crate::render::pixel_camera::CanvasScale;
use crate::room::{DecorationGranted, RoomCleared, fruit_count};
use crate::settings::GameSettings;
use crate::state::GameState;
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;
use crate::world::progress::LevelProgress;
use bevy::prelude::*;

/// HUD 字号（逻辑像素，坐标系是 `VIEWPORT_W` 宽的视口）。
const HUD_FONT_PX: f32 = 12.0;
/// HUD 距视口左/上边距（逻辑像素）。
const HUD_MARGIN_PX: f32 = 8.0;
/// 结算面板的固定几何（`07` §7.10 禁止用百分比定位）。
const PANEL_W: f32 = 296.0;
const PANEL_H: f32 = 96.0;
const PANEL_LEFT: f32 = 12.0;
const PANEL_TOP: f32 = 112.0;

// HUD 文案由 `i18n` 提供（默认中文）。字体来自 `fonts::UiFont`；缺 CJK 字形时 Bevy 会把整行
// 后续文本一起丢掉（`10` §10.3.1 E7），所以字体加载失败只保证英文可见。
const HUD_LABEL: &str = "animj";

/// HUD 文本节点标记。
///
/// 必须有这个过滤条件：项目里还有一个调试文本节点（`render::debug`），
/// 不加过滤的 `Single<&mut Text>` 会因匹配到两个实体而**静默跳过整个系统**（HUD 永远停在生成时的初值）。
#[derive(Component)]
struct HudText;

/// 一次性的提示条（通关、获奖）。同一时刻只显示最新一条。
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

/// 结算面板根。
#[derive(Component)]
struct CompletionPanel;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudToast>()
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (collect_toasts, tick_toast, update_hud).chain())
            .add_systems(OnEnter(GameState::GameComplete), spawn_completion);
    }
}

fn spawn_hud(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(HUD_MARGIN_PX),
            top: px(HUD_MARGIN_PX),
            ..default()
        },
        Text::new(HUD_LABEL),
        font.text_font(HUD_FONT_PX),
        TextColor(Color::WHITE),
        HudText,
    ));
}

/// 通关 / 获奖消息 → 提示条。
fn collect_toasts(
    mut cleared: MessageReader<RoomCleared>,
    mut granted: MessageReader<DecorationGranted>,
    settings: Res<GameSettings>,
    mut toast: ResMut<HudToast>,
) {
    let s = settings.language.strings();
    for _ in cleared.read() {
        toast.show(s.room_cleared);
    }
    for event in granted.read() {
        toast.show(format!("{}+{}", s.decoration_gained, event.decoration_id));
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

/// 当前房间的目标进度描述（例如中文 `出口  果实 2/3`）。
fn goals_summary(
    goals: &[level_format::Goal],
    collected: u32,
    total: u32,
    s: &crate::i18n::Strings,
) -> String {
    goals
        .iter()
        .map(|goal| match goal {
            level_format::Goal::ReachExit => s.exit_goal.to_string(),
            level_format::Goal::Collect { count } => format!("{} {collected}/{count}", s.fruit),
            level_format::Goal::CollectAll => {
                format!("{} {collected}/{total}", s.all_fruits)
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    state: Res<State<GameState>>,
    canvas_scale: Res<CanvasScale>,
    asset_root: Res<AssetRoot>,
    current: Option<Res<CurrentRoom>>,
    loaded: Option<Res<LoadedLevel>>,
    tiles: Option<Res<LevelTiles>>,
    progress: Option<Res<LevelProgress>>,
    decorations: Option<Res<Decorations>>,
    toast: Option<Res<HudToast>>,
    settings: Res<GameSettings>,
    mut text: Single<&mut Text, With<HudText>>,
) {
    let language = settings.language;
    let s = language.strings();
    let mut room = String::from("-");
    if let Some(current) = current.as_ref() {
        room = format!("({}, {})", current.col, current.row);
    }
    let mut size = String::from("-");
    if let Some(tiles) = tiles.as_ref() {
        let (cols, rows) = tiles.grid();
        size = format!("{cols}x{rows}");
    }
    let mut level = String::from("-");
    if let Some(loaded) = loaded.as_ref() {
        level = loaded.level.name.clone();
    }

    // 清关进度与装饰品
    let (cleared, total_rooms) = progress
        .as_ref()
        .map(|p| (p.cleared_count(), p.rooms.len()))
        .unwrap_or((0, 0));
    let decor = decorations.as_ref().map(|d| d.len()).unwrap_or(0);

    // 当前房间的目标进度
    let mut goals = String::from("-");
    if let (Some(loaded), Some(current), Some(progress)) =
        (loaded.as_ref(), current.as_ref(), progress.as_ref())
    {
        let index =
            level_format::room_index(current.col, current.row, loaded.level.grid.cols) as usize;
        if let (Some(room_data), Some(room_progress)) = (
            loaded.level.rooms.get(index),
            progress.room(current.col, current.row),
        ) {
            goals = goals_summary(
                &room_data.goals,
                room_progress.collected_fruit,
                fruit_count(room_data),
                s,
            );
        }
    }

    let toast_line = toast.as_ref().map(|t| t.text.as_str()).unwrap_or("");
    let flags = if let Some(room_progress) = progress
        .as_ref()
        .zip(current.as_ref())
        .and_then(|(p, c)| p.room(c.col, c.row))
    {
        if room_progress.cleared { s.cleared } else { "" }
    } else {
        ""
    };

    let next = format!(
        "{HUD_LABEL}  {state} x{scale}  {level} {size}\n\
         {room_label} {room} {flags}  {clear_label} {cleared}/{total_rooms}  {decor_label} {decor}\n\
         {goals_label} {goals}\n\
         {toast_line}\n\
         {assets_label} {root}",
        state = language.state_name(*state.get()),
        scale = canvas_scale.0,
        root = asset_root.0,
        room_label = s.room,
        clear_label = s.cleared_count,
        decor_label = s.decorations,
        goals_label = s.goals,
        assets_label = s.assets,
    );
    // 只在内容变化时写回：避免每帧触发 CJK 文本的重新断行/排版（ICU4X 的开销与日志噪声）。
    if text.0 != next {
        text.0 = next;
    }
}

/// 进入 `GameComplete` 时展示结算（`01` §1.6）。
fn spawn_completion(
    mut commands: Commands,
    loaded: Option<Res<LoadedLevel>>,
    decorations: Option<Res<Decorations>>,
    settings: Res<GameSettings>,
    font: Res<UiFont>,
) {
    let s = settings.language.strings();
    let total_rewards = loaded
        .as_ref()
        .map(|loaded| {
            loaded
                .level
                .rooms
                .iter()
                .filter(|room| room.reward.is_some())
                .count()
        })
        .unwrap_or(0);
    let owned = decorations.as_ref().map(|d| d.len()).unwrap_or(0);

    let panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(PANEL_LEFT),
                top: px(PANEL_TOP),
                width: px(PANEL_W),
                height: px(PANEL_H),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            CompletionPanel,
        ))
        .id();
    commands.spawn((
        Text::new(format!(
            "{}\n{} {owned}/{total_rewards}",
            s.all_rooms_cleared, s.decorations
        )),
        font.text_font(HUD_FONT_PX),
        TextColor(Color::WHITE),
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
        ChildOf(panel),
    ));
}
