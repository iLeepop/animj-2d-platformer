//! 关卡编辑器库（**只有 `animj-editor` 链接**；发布 bin `animj` 不依赖本 crate）。
//!
//! 权威定义见 `docs/08-level-editor.md`。

/// 构建标记：启动时打印，用于确认"跑的是不是最新构建"。
/// 每次改编辑器 UI/交互都手动递增末尾字母。
pub const BUILD_TAG: &str = "2026-10-18/l";

/// egui 通行内的执行顺序（`08` §8.5.6）：**先**构成本帧所有面板，**再**处理世界输入。
/// 顺序反了时 `ctx.egui_wants_pointer_input()` 读到的是上一帧的布局，点按钮会漏到画布。
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum EditorUiSet {
    /// 构建本帧面板（含纹理上传与叠加层）。
    Panels,
    /// 世界输入与快捷键（此时 egui 命中结果已是本帧的）。
    WorldInput,
}

pub mod anim_tool;
pub mod camera;
pub mod io;
pub mod playtest;
pub mod session;
pub mod tile_tool;
pub mod tools;
pub mod ui;
pub mod view;

use bevy::prelude::*;

use crate::session::EditorSession;

/// 编辑器独有状态（`08` §8.3.1）：**`game` 完全不知道它存在**。
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorMode {
    /// 编辑态：`GameState` 停在 `Paused`，`game` 的玩法系统天然停摆。
    #[default]
    Editing,
    /// 就地试玩：由 `game` 的输入与物理接管。
    Playing,
}

/// 编辑器总插件：装配相机/投影/工具/面板/试玩。
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<EditorMode>()
            .init_resource::<crate::tile_tool::TileTool>()
            .init_resource::<crate::anim_tool::SheetTexture>()
            .init_resource::<crate::tile_tool::TileTexture>()
            .init_resource::<crate::ui::BottomTab>()
            .init_resource::<crate::tile_tool::AssetPickerState>()
            .configure_sets(
                bevy_egui::EguiPrimaryContextPass,
                EditorUiSet::Panels.before(EditorUiSet::WorldInput),
            )
            .add_systems(Startup, announce_build)
            .add_systems(Update, init_session_from_loaded)
            .add_systems(Update, io::watch_external_changes)
            .add_systems(Update, anim_tool::init_anim_edit)
            .add_plugins((
                camera::EditorCameraPlugin,
                view::EditorViewPlugin,
                tools::EditorToolsPlugin,
                ui::EditorUiPlugin,
                playtest::EditorPlaytestPlugin,
            ));
    }
}

/// 加载时按贴图实际宽度校正 `tileset_columns`。
///
/// 必须在 `EditorSession::new` **之前**做：`new` 会把关卡文本记为 `saved_text` 基准，
/// 运行期再改列数会让"试玩往返缓冲未变"的断言误判。
fn correct_columns_from_tileset(level: &mut level_format::Level, root: &str) {
    let Some(path) = level.tileset.clone() else {
        return;
    };
    let Some(relative) = level_format::normalize_asset_path(&path) else {
        return;
    };
    let full = std::path::Path::new(root).join(&relative);
    let Ok(bytes) = std::fs::read(&full) else {
        return;
    };
    let extension = relative.rsplit('.').next().unwrap_or("png").to_string();
    let Ok(image) = Image::from_buffer(
        &bytes,
        bevy::image::ImageType::Extension(Box::leak(extension.into_boxed_str())),
        bevy::image::CompressedImageFormats::NONE,
        true,
        bevy::image::ImageSampler::nearest(),
        bevy::asset::RenderAssetUsages::default(),
    ) else {
        return;
    };
    let columns = level_format::columns_from_sheet_width(image.width());
    if level.tileset_columns != columns {
        info!(
            "编辑器：贴图宽 {} px ⇒ 列数 {}（原为 {}）",
            image.width(),
            columns,
            level.tileset_columns
        );
        level.tileset_columns = columns;
    }
}

/// 启动横幅：让用户/我都能从控制台确认二进制版本与已启用的编辑器特性。
fn announce_build() {
    info!(
        "animj-editor BUILD_TAG={} features=[room-picker, entity-brush(player_spawn), all-rooms-entity-list, screen-space-overlay]",
        BUILD_TAG
    );
}

/// 编辑器加载关卡：从 `game` 的 `LoadedLevel` 取一份克隆作为 `doc` 初值。
///
/// 依 `08` §8.7.2，编辑器**不自己解析文件**；首次启动复用 `game` 已加载的关卡。
pub fn init_session_from_loaded(
    mut commands: Commands,
    loaded: Option<Res<game::world::load::LoadedLevel>>,
    existing: Option<Res<EditorSession>>,
    root: Res<game::assets::AssetRoot>,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut fallback_done: Local<bool>,
    mut frames_waited: Local<u32>,
) {
    if existing.is_some() {
        return;
    }
    let Some(loaded) = loaded else {
        // 关卡校验失败时 `game` 永远不产出 `LoadedLevel`（它停在 Loading）。
        // 编辑器必须能打开"有错"的关卡去修；否则会静默停留在默认文档上，
        // 用户改贴图路径等任何编辑都作用在错误对象上（实机 bug）。
        // 用帧数而不是真实时间等待：`--frames` 无节流时 `Time::elapsed` 几乎不涨
        *frames_waited += 1;
        if *fallback_done || *frames_waited < 30 {
            return;
        }
        *fallback_done = true;
        let path = std::path::Path::new(&root.0).join("levels/world_01.level.ron");
        match std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| ron::from_str::<level_format::Level>(&text).map_err(|error| error.to_string()))
        {
            Ok(mut level) => {
                correct_columns_from_tileset(&mut level, &root.0);
                let mut session = EditorSession::new(level, Some(path.clone()));
                session.file_issues = level_format::validate(&session.doc);
                let count = session.file_issues.len();
                commands.insert_resource(session);
                next.set(game::state::GameState::Paused);
                warn!("编辑器：关卡校验未通过，已直接打开 {}（{count} 条问题，见底栏错误列表）", path.display());
            }
            Err(error) => error!("编辑器：无法读取关卡 {}：{error}", path.display()),
        }
        return;
    };
    let path = std::path::Path::new(&root.0).join(game::world::load::level_asset_path(
        &loaded.level.name,
    ));
    let mut level = loaded.level.clone();
    correct_columns_from_tileset(&mut level, &root.0);
    commands.insert_resource(EditorSession::new(level, Some(path)));
    // 编辑态：`GameState` 停在 `Paused`，`game` 的玩法系统天然停摆（`08` §8.3.2）
    next.set(game::state::GameState::Paused);
}
