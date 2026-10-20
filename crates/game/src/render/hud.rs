//! 屏幕空间 HUD。所有者文档：`docs/07-rendering.md` §7.10。
//!
//! HUD 走 **UI 窗口相机**（`IsDefaultUiCamera` 挂在它上面），`UiScale = k` 让逻辑坐标仍然是
//! `ViewLayout` 的尺寸（单人 320×320，本地双人 640×320；`07` §7.10.1）。因此所有尺寸与字号都用 `px()` 的整数值，
//! 禁止 `vw()` / `vh()` / `percent()`。

use crate::config::render::HUD_TOAST_SECS;
use crate::fonts::UiFont;
use crate::player::decor::Decorations;
use crate::room::{DecorationGranted, RoomCleared, fruit_count};
use crate::settings::GameSettings;
use crate::state::GameState;
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
        Text::new(""),
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

/// 只显示**玩法信息**：当前房间的目标进度与提示条。
///
/// 关卡名/网格尺寸/房间坐标/通关计数/装饰品数/资产根路径属于开发诊断信息，
/// 归 F3 调试叠层（`render::debug`）——不要放回常驻 HUD。
fn update_hud(
    current: Option<Res<CurrentRoom>>,
    loaded: Option<Res<LoadedLevel>>,
    progress: Option<Res<LevelProgress>>,
    toast: Option<Res<HudToast>>,
    settings: Res<GameSettings>,
    mut text: Single<&mut Text, With<HudText>>,
) {
    let s = settings.language.strings();
    let mut lines: Vec<String> = Vec::new();

    // 当前房间的目标进度（收集计数已包含在 `goals_summary` 里，例如 `果实 2/3`）
    if let (Some(loaded), Some(current), Some(progress)) =
        (loaded.as_ref(), current.as_ref(), progress.as_ref())
    {
        let index =
            level_format::room_index(current.col, current.row, loaded.level.grid.cols) as usize;
        if let (Some(room_data), Some(room_progress)) = (
            loaded.level.rooms.get(index),
            progress.room(current.col, current.row),
        ) {
            let goals = goals_summary(
                &room_data.goals,
                room_progress.collected_fruit,
                fruit_count(room_data),
                s,
            );
            if !goals.is_empty() {
                lines.push(format!("{} {goals}", s.goals));
            }
        }
    }

    if let Some(toast) = toast.as_ref()
        && !toast.text.is_empty()
    {
        lines.push(toast.text.clone());
    }

    let next = lines.join("\n");
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
