//! 屏幕空间 HUD。所有者文档：`docs/07-rendering.md` §7.10。
//!
//! HUD 走**内层相机**（`IsDefaultUiCamera` 挂在它上面）⇒ HUD 与游戏画面共用一个像素格，
//! 因此所有尺寸与字号都用 `px()` 的整数值，禁止 `vw()` / `vh()` / `percent()`。

use bevy::prelude::*;
use bevy::text::FontSize;

use crate::assets::AssetRoot;
use crate::config::render::HUD_TOAST_SECS;
use crate::player::decor::Decorations;
use crate::render::pixel_camera::CanvasScale;
use crate::room::{DecorationGranted, RoomCleared, fruit_count};
use crate::state::GameState;
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;
use crate::world::progress::LevelProgress;

/// HUD 字号（逻辑像素，坐标系是 512 宽的视口）。
const HUD_FONT_PX: f32 = 12.0;
/// HUD 距视口左/上边距（逻辑像素）。
const HUD_MARGIN_PX: f32 = 8.0;
/// 结算面板的固定几何（`07` §7.10 禁止用百分比定位）。
const PANEL_W: f32 = 416.0;
const PANEL_H: f32 = 96.0;
const PANEL_TOP: f32 = 200.0;

// ★ HUD 文本**只能是 ASCII**：Bevy 的 `default_font` 只含 ASCII 字形，遇到缺字形会把后续文本一起丢掉
//   （实测：`"animj · M0 骨架…"` 只渲染出 `animj`）。仓库自带 TTF 入库前（`04` §9），UI/世界文本一律用 ASCII。
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
            .add_systems(
                Update,
                (collect_toasts, tick_toast, update_hud).chain(),
            )
            .add_systems(OnEnter(GameState::GameComplete), spawn_completion);
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(HUD_MARGIN_PX),
            top: px(HUD_MARGIN_PX),
            ..default()
        },
        Text::new(HUD_LABEL),
        TextFont {
            font_size: FontSize::Px(HUD_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        HudText,
    ));
}

/// 通关 / 获奖消息 → 提示条。
fn collect_toasts(
    mut cleared: MessageReader<RoomCleared>,
    mut granted: MessageReader<DecorationGranted>,
    mut toast: ResMut<HudToast>,
) {
    for _ in cleared.read() {
        toast.show("ROOM CLEARED");
    }
    for event in granted.read() {
        toast.show(format!("DECORATION +{}", event.decoration_id));
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

/// 当前房间的目标进度描述（ASCII，例如 `EXIT  FRUIT 2/3`）。
fn goals_summary(
    goals: &[level_format::Goal],
    collected: u32,
    total: u32,
) -> String {
    goals
        .iter()
        .map(|goal| match goal {
            level_format::Goal::ReachExit => "EXIT".to_string(),
            level_format::Goal::Collect { count } => format!("FRUIT {collected}/{count}"),
            level_format::Goal::CollectAll => format!("ALL FRUITS {collected}/{total}"),
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
    mut text: Single<&mut Text, With<HudText>>,
) {
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
        if let (Some(room_data), Some(room_progress)) =
            (loaded.level.rooms.get(index), progress.room(current.col, current.row))
        {
            goals = goals_summary(
                &room_data.goals,
                room_progress.collected_fruit,
                fruit_count(room_data),
            );
        }
    }

    let toast_line = toast.as_ref().map(|t| t.text.as_str()).unwrap_or("");
    let flags = if let Some(room_progress) = progress
        .as_ref()
        .zip(current.as_ref())
        .and_then(|(p, c)| p.room(c.col, c.row))
    {
        if room_progress.cleared {
            "CLEARED"
        } else {
            ""
        }
    } else {
        ""
    };

    text.0 = format!(
        "{HUD_LABEL}  {state:?} x{scale}  {level} {size}\n\
         room {room} {flags}  clear {cleared}/{total_rooms}  decor {decor}\n\
         goals {goals}\n\
         {toast_line}\n\
         assets {root}",
        state = state.get(),
        scale = canvas_scale.0,
        root = asset_root.0,
    );
}

/// 进入 `GameComplete` 时展示结算（`01` §1.6）。
fn spawn_completion(
    mut commands: Commands,
    loaded: Option<Res<LoadedLevel>>,
    decorations: Option<Res<Decorations>>,
) {
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
                left: px(48.0),
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
            "ALL ROOMS CLEARED\nDECORATIONS {owned}/{total_rewards}"
        )),
        TextFont {
            font_size: FontSize::Px(HUD_FONT_PX),
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
        ChildOf(panel),
    ));
}
