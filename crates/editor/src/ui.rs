//! 编辑器 UI（egui 面板骨架）。所有者文档：`docs/08-level-editor.md` §8.5.6。
//!
//! **纪律**：面板只读 `EditorSession`，所有修改都经 `session` 的方法（统一 push 快照），
//! 禁止在 UI 里直接改 `doc` 字段。

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::camera::{EditorCamera, room_size};
use crate::anim_tool::{AnimEdit, SheetTexture, ensure_sheet_texture};
use crate::tile_tool::{TileTexture, TileTool, ensure_tile_texture};
use crate::session::{EditorSession, Tool};
use crate::view::{
    EditorViewFlags, ERROR_COLOR, GRID_COLOR, HOVER_COLOR, MARQUEE_COLOR, ROOM_BORDER_COLOR,
};
use crate::tools::PlaytestRequest;
use crate::EditorMode;

pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BottomHeight>()
        .add_systems(
            EguiPrimaryContextPass,
            install_cjk_font.in_set(crate::EditorUiSet::Panels),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                // 先上传纹理、画世界叠加层（背景层），再构造成面板
                prepare_textures,
                paint_overlay,
                panels,
            )
                .chain()
                .after(install_cjk_font)
                .in_set(crate::EditorUiSet::Panels)
                .run_if(in_state(EditorMode::Editing))
                .run_if(resource_exists::<EditorSession>),
        );
    }
}

/// 屏幕空间叠加层（网格 / 房间边框 / 当前房间 / 选区）。
///
/// **为什么不用 Gizmos**：实机反馈 1-px 的 2D gizmo 线在 1×/2× 下逐帧闪烁（`G` 关掉网格即停），
/// 换 `line_2d` 与像素中心对齐后仍在目标机器上复现。egui 用**整数屏幕坐标**绘制，几何不随帧变化，
/// 从绘制路径上消除亚像素抖动。见 `08` §8.5.5。
fn paint_overlay(
    mut contexts: EguiContexts,
    session: Res<EditorSession>,
    flags: Res<EditorViewFlags>,
    view: Res<crate::camera::EditorViewState>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    camera: Option<Single<&Transform, With<EditorCamera>>>,
    mut last_room: Local<Option<(u32, u32)>>,
) -> Result {
    let (Some(window), Some(camera)) = (window, camera) else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let painter = ctx.layer_painter(egui::LayerId::background());
    let size = window.size();
    let scale = view.scale();
    let cam = camera.translation.truncate();
    // **不要用 `GlobalTransform`**：变换传播在 `PostUpdate`，egui 通行读到的是上一帧的相机矩阵
    // ⇒ 切房间后网格停在旧位置。这里直接用 `Transform` + `scale` 解析换算（与 `08` §8.4.2 的
    // `scale = 世界单位/逻辑像素` 一致），并四舍五入到整像素。
    let to_screen = |world: Vec2| -> egui::Pos2 {
        let rel = (world - cam) / scale;
        egui::pos2(
            (size.x * 0.5 + rel.x).round(),
            (size.y * 0.5 - rel.y).round(),
        )
    };
    let stroke = |width: f32, color: Color| {
        let c = color.to_srgba();
        egui::Stroke::new(
            width,
            egui::Color32::from_rgba_unmultiplied(
                (c.red * 255.0) as u8,
                (c.green * 255.0) as u8,
                (c.blue * 255.0) as u8,
                (c.alpha * 255.0) as u8,
            ),
        )
    };

    let (cols, rows) = (session.doc.grid.cols, session.doc.grid.rows);
    let room_px = level_format::ROOM_PX as f32;
    let cell = level_format::TILE_PX as f32;

    // 可见世界矩形（按屏幕四角反算，避免依赖相机中心公式）
    let corners = [
        Vec2::new(0.0, 0.0),
        Vec2::new(size.x, 0.0),
        Vec2::new(0.0, size.y),
        Vec2::new(size.x, size.y),
    ];
    if *last_room != Some(session.current_room) {
        *last_room = Some(session.current_room);
        let center = game::coords::room_center(
            session.current_room.0,
            session.current_room.1,
            session.doc.grid.rows,
        );
        let screen = to_screen(center);
        info!(
            "编辑器：叠加层投影 room ({}, {}) 中心 → 屏幕 ({:.0}, {:.0})（视口中心应为 {:.0}, {:.0}）",
            session.current_room.0,
            session.current_room.1,
            screen.x,
            screen.y,
            size.x * 0.5,
            size.y * 0.5
        );
    }

    let half_view = size * 0.5 * scale;
    let visible_min = cam - half_view;
    let visible_max = cam + half_view;
    let _ = corners;

    for row in 0..rows {
        for col in 0..cols {
            let center = game::coords::room_center(col, row, rows);
            let half = room_px * 0.5;
            let room_min = center - Vec2::splat(half);
            let room_max = center + Vec2::splat(half);
            let is_current = (col, row) == session.current_room;
            let offscreen = room_min.x >= visible_max.x
                || visible_min.x >= room_max.x
                || room_min.y >= visible_max.y
                || visible_min.y >= room_max.y;
            // 当前房间**永远**绘制（即使可见矩形换算失败/尚未更新），避免"切房间后没有网格"
            if offscreen && !is_current {
                continue;
            }
            if flags.grid {
                for i in 0..=level_format::ROOM_TILES {
                    let offset = i as f32 * cell;
                    let x = room_min.x + offset;
                    painter.line_segment(
                        [
                            to_screen(Vec2::new(x, room_min.y)),
                            to_screen(Vec2::new(x, room_max.y)),
                        ],
                        stroke(1.0, GRID_COLOR),
                    );
                    let y = room_min.y + offset;
                    painter.line_segment(
                        [
                            to_screen(Vec2::new(room_min.x, y)),
                            to_screen(Vec2::new(room_max.x, y)),
                        ],
                        stroke(1.0, GRID_COLOR),
                    );
                }
            }
            if flags.borders {
                painter.rect_stroke(
                    egui::Rect::from_min_max(to_screen(room_min), to_screen(room_max)),
                    0.0,
                    stroke(1.0, ROOM_BORDER_COLOR),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    // 当前房间高亮
    let center = game::coords::room_center(session.current_room.0, session.current_room.1, rows);
    let half = room_size() * 0.5;
    painter.rect_stroke(
        egui::Rect::from_min_max(to_screen(center - half), to_screen(center + half)),
        0.0,
        stroke(2.0, HOVER_COLOR),
        egui::StrokeKind::Inside,
    );
    // 选区
    if let Some((min, max)) = session.selection.world_bounds {
        painter.rect_stroke(
            egui::Rect::from_min_max(to_screen(min), to_screen(max)),
            0.0,
            stroke(2.0, MARQUEE_COLOR),
            egui::StrokeKind::Inside,
        );
    }
    // 错误常驻描边（`08` §8.8.2）：瓦片级错误在其格子上画红框，错误消失即消失
    let cell_size = level_format::TILE_PX as f32;
    for issue in &session.file_issues {
        if issue.severity == level_format::Severity::Warning {
            continue;
        }
        let Some(room) = room_of_path(&issue.path, session.doc.grid.cols) else {
            continue;
        };
        let Some(center) = cell_of_issue(&issue.message, &issue.path, room, &session) else {
            continue;
        };
        let half = Vec2::splat(cell_size * 0.5);
        painter.rect_stroke(
            egui::Rect::from_min_max(to_screen(center - half), to_screen(center + half)),
            0.0,
            stroke(2.0, ERROR_COLOR),
            egui::StrokeKind::Inside,
        );
    }

    for (col, row) in &session.selection.rooms {
        let center = game::coords::room_center(*col, *row, rows);
        painter.rect_stroke(
            egui::Rect::from_min_max(to_screen(center - half), to_screen(center + half)),
            0.0,
            stroke(2.0, MARQUEE_COLOR),
            egui::StrokeKind::Inside,
        );
    }
    Ok(())
}

/// 底部栏高度（用户拖动分隔条）。面板用 `exact_size` 固定，永远贴住窗口底边。
#[derive(Resource, Debug, Clone, Copy)]
pub struct BottomHeight(pub f32);

impl Default for BottomHeight {
    fn default() -> Self {
        Self(300.0)
    }
}

impl BottomHeight {
    pub const MIN: f32 = 120.0;

    /// 拖动竖直分隔条：`delta_y < 0`（向上拖）时变高。上限留出顶栏 + 世界区。
    pub fn drag(&mut self, delta_y: f32, viewport_height: f32) {
        let max = (viewport_height - 160.0).max(Self::MIN);
        self.0 = (self.0 - delta_y).clamp(Self::MIN, max);
    }
}

/// 底部工具区的标签页（`08` §8.5.7）。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomTab {
    #[default]
    Tiles,
    Animation,
}

/// 面板根 Ui。egui 0.36 的 `Panel::show` 以传入 Ui 的可用矩形为容器，
/// 四个面板必须共用同一个根、按 顶 → 底 → 左 → 右 顺序显示。
fn ui_root(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        "animj_editor_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

/// 中文字体：egui 内置字体没有 CJK 字形（中文全变方块 `□`），启动时把系统里的
/// 中文 TTF 注册为首选字体。只接受单字体 `.ttf`（ab_glyph 不支持 `.ttc` 集合）。
fn install_cjk_font(mut contexts: EguiContexts, mut done: Local<bool>) -> Result {
    if *done {
        return Ok(());
    }
    *done = true;
    let ctx = contexts.ctx_mut()?;
    const CANDIDATES: &[&str] = &[
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simkai.ttf",
        "C:/Windows/Fonts/simfang.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in CANDIDATES {
        // `.ttc` 是字体集合，ab_glyph 不支持，跳过（保留在列表里只为记录尝试路径）
        if path.ends_with(".ttc") {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(0, "cjk".to_owned());
            }
        }
        ctx.set_fonts(fonts);
        info!("编辑器：已加载中文字体 {path}");
        return Ok(());
    }
    error!(
        "编辑器：未找到可用的中文字体（试过 {}），中文会显示为方块",
        CANDIDATES.join(", ")
    );
    Ok(())
}

/// 同步读盘的图集缓存：`(路径, mtime, 图像)`；mtime 变化即重新解码（`04` §5.6）。
type ImageCache = Option<(String, Option<std::time::SystemTime>, Handle<Image>)>;

/// 纹理上传：角色图集（动画预览）与瓦片图集（左栏缩略图 / 地块选格）。
fn prepare_textures(
    mut contexts: EguiContexts,
    mut session: ResMut<EditorSession>,
    mut images: ResMut<Assets<Image>>,
    sheet: Option<Res<game::player::spawn::PlayerSheet>>,
    anim_edit: Option<Res<AnimEdit>>,
    mut sheet_cache: Local<ImageCache>,
    mut uploaded_sheet: Local<Option<Handle<Image>>>,
    mut sheet_texture: ResMut<SheetTexture>,
    tileset: Option<Res<game::render::tiles::Tileset>>,
    mut tile_texture: ResMut<TileTexture>,
    mut tile_texture_handle: Local<Option<Handle<Image>>>,
    mut sync_cache: Local<ImageCache>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // 动画图集：跟随 `AnimEdit.doc.sheet`（可编辑路径）；同样同步读盘
    let anim_sheet = anim_edit
        .as_ref()
        .and_then(|edit| {
            edit.doc
                .states
                .get(edit.preview_state)
                .or_else(|| edit.doc.states.first())
        })
        .map(|state| state.sheet.clone())
        .unwrap_or_else(|| "sprites/player/cat_sheet.png".to_string());
    let sheet_key = level_format::normalize_asset_path(&anim_sheet).unwrap_or_default();
    let sheet_stamp = game::assets::texture_stamp(&sheet_key);
    let sheet_handle = match sheet_cache.as_ref() {
        Some((cached, cached_stamp, handle))
            if cached == &sheet_key && *cached_stamp == sheet_stamp =>
        {
            Some(handle.clone())
        }
        _ => {
            let loaded = game::render::tiles::load_tileset_image(&sheet_key, &mut images);
            if let Some(handle) = &loaded {
                *sheet_cache = Some((sheet_key.clone(), sheet_stamp, handle.clone()));
            }
            loaded
        }
    };
    if let Some(handle) = sheet_handle {
        // 换图集必须强制重传：`ensure_sheet_texture` 见到已有纹理会直接返回
        if uploaded_sheet.as_ref() != Some(&handle) {
            sheet_texture.0 = None;
            *uploaded_sheet = Some(handle.clone());
        }
        ensure_sheet_texture(ctx, &images, &handle, &mut sheet_texture);
    } else if let Some(sheet) = sheet.as_ref() {
        // 回退到游戏加载的角色图集
        if uploaded_sheet.as_ref() != Some(&sheet.image) {
            sheet_texture.0 = None;
            *uploaded_sheet = Some(sheet.image.clone());
        }
        ensure_sheet_texture(ctx, &images, &sheet.image, &mut sheet_texture);
    }
    let tile_image = match &session.doc.tileset {
        Some(path) => {
            // 同步读盘（运行期 `AssetServer::load` 的图像会永远停在 Loading，
            // 见 `game::render::tiles::load_tileset_image` 的说明）
            let key = level_format::normalize_asset_path(path).unwrap_or_default();
            let stamp = game::assets::texture_stamp(&key);
            match sync_cache.as_ref() {
                Some((cached, cached_stamp, handle))
                    if cached == &key && *cached_stamp == stamp =>
                {
                    Some(handle.clone())
                }
                _ => {
                    let loaded = game::render::tiles::load_tileset_image(&key, &mut images);
                    if let Some(handle) = &loaded {
                        *sync_cache = Some((key.clone(), stamp, handle.clone()));
                    }
                    loaded
                }
            }
        }
        None => tileset.as_ref().map(|tileset| tileset.image.clone()),
    };
    if let Some(image) = tile_image {
        ensure_tile_texture(ctx, &images, &image, &mut tile_texture, &mut tile_texture_handle);
        // 列数按贴图实际宽度校正（`tileset1.png` = 192 px ⇒ 6 列；默认 8 会让左栏/渲染全部错位）
        if let Some(loaded) = images.get(&image) {
            let columns = level_format::columns_from_sheet_width(loaded.width());
            if session.doc.tileset_columns != columns {
                info!(
                    "编辑器：贴图宽 {} px ⇒ 列数 {}（原为 {}），已自动校正",
                    loaded.width(),
                    columns,
                    session.doc.tileset_columns
                );
                session.doc.tileset_columns = columns;
                // 必须 bump：投影/渲染只在 `doc_revision` 变化时重建，
                // 否则仍按旧列数（= 内置 8 列布局）出图，看起来"设置贴图没生效"。
                session.doc_revision = session.doc_revision.wrapping_add(1);
            }
        }
    }
    Ok(())
}

/// 顶栏：路径、错误计数、当前房间/工具、构建号、相机诊断。
fn top_bar(
    ui: &mut egui::Ui,
    session: &EditorSession,
    view: &crate::camera::EditorViewState,
    cam: Vec2,
    projection_count: usize,
) {
    egui::Panel::top("animj_editor_top").show(ui, |ui| {
        ui.horizontal(|ui| {
            let path = session
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<no path>".to_string());
            ui.label(path);
            if session.dirty {
                ui.colored_label(egui::Color32::YELLOW, "*");
            }
            let errors = session.error_count();
            let warnings = session.warning_count();
            let color = if errors > 0 {
                egui::Color32::RED
            } else if warnings > 0 {
                egui::Color32::YELLOW
            } else {
                egui::Color32::GREEN
            };
            ui.colored_label(color, format!("{errors} 个错误 / {warnings} 个警告"));
            ui.separator();
            ui.label(format!(
                "房间 ({}, {})",
                session.current_room.0, session.current_room.1
            ));
            ui.label(format!("工具 {}", session.tool.label()));
            ui.label(format!("构建 {}", crate::BUILD_TAG));
            let fps = ui.ctx().input(|input| 1.0 / input.stable_dt.max(1e-4));
            ui.label(format!(
                "cam({:.0},{:.0}) scale {:.2} room({},{}) fps {:.0} proj {}",
                cam.x,
                cam.y,
                view.scale(),
                session.current_room.0,
                session.current_room.1,
                fps,
                projection_count
            ));
            if !session.status.is_empty() {
                ui.separator();
                ui.label(&session.status);
            }
        });
    });
}

/// 左栏：工具选择 + 带缩略图的瓦片选择（`08` §8.5.6）。
fn left_panel(ui: &mut egui::Ui, session: &mut EditorSession, tile_texture: &TileTexture) {
    egui::Panel::left("animj_editor_left").show(ui, |ui| {
        ui.heading("工具");
        for tool in Tool::ALL {
            if ui
                .selectable_label(session.tool == tool, tool.label())
                .clicked()
            {
                session.tool = tool;
            }
        }
        ui.separator();
        ui.heading("瓦片（关卡调色板）");
        let palette = session.doc.palette();
        if palette.is_empty() {
            ui.label("调色板为空");
        }
        let columns = session.doc.tileset_columns.max(1);
        let texture = tile_texture.0.as_ref();
        let sheet = texture.map(|texture| texture.size_vec2());
        for chunk in palette.chunks(2) {
            ui.horizontal(|ui| {
                for entry in chunk {
                    ui.vertical(|ui| {
                        let response = match (texture, sheet) {
                            (Some(texture), Some(sheet)) => {
                                let cell = 32.0;
                                let col = entry.cell % columns;
                                let row = entry.cell / columns;
                                let uv = egui::Rect::from_min_size(
                                    egui::pos2(
                                        (col as f32 * cell) / sheet.x,
                                        (row as f32 * cell) / sheet.y,
                                    ),
                                    egui::vec2(cell / sheet.x, cell / sheet.y),
                                );
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(
                                        texture.id(),
                                        sheet,
                                    ))
                                    .uv(uv)
                                    .fit_to_exact_size(egui::vec2(32.0, 32.0))
                                    .sense(egui::Sense::click()),
                                )
                            }
                            _ => ui.allocate_response(
                                egui::vec2(32.0, 32.0),
                                egui::Sense::click(),
                            ),
                        };
                        if session.brush_char == entry.char {
                            ui.painter().rect_stroke(
                                response.rect,
                                0.0,
                                egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if ui.label(format!("{}  {}", entry.char, entry.name)).clicked()
                            || response.clicked()
                        {
                            session.brush_char = entry.char;
                        }
                    });
                }
            });
        }
        // 擦除
        ui.horizontal(|ui| {
            let response = ui.allocate_response(egui::vec2(32.0, 32.0), egui::Sense::click());
            if session.brush_char == '.' {
                ui.painter().rect_stroke(
                    response.rect,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
            if ui.label(".  擦除").clicked() || response.clicked() {
                session.brush_char = '.';
            }
        });
    });
}

/// 右栏：只做地图与选中房间的编辑（房间选择 / 奖励 / 目标 / 实体 / 多选）。
fn right_panel(ui: &mut egui::Ui, session: &mut EditorSession) {
    egui::Panel::right("animj_editor_right").show(ui, |ui| {
        // 房间选择器：**点击式**切房间（`Tab` 会被 egui 焦点遍历吞掉，键盘不可靠）
        // 切房间**不移动相机**；需要时点「对焦」。
        ui.horizontal(|ui| {
            ui.heading("房间");
            if ui.small_button("对焦").clicked() {
                session.focus_camera = true;
            }
        });
        for row in 0..session.doc.grid.rows {
            ui.horizontal(|ui| {
                for col in 0..session.doc.grid.cols {
                    let current = session.current_room == (col, row);
                    if ui
                        .selectable_label(current, format!("{col},{row}"))
                        .clicked()
                    {
                        session.goto_room(col, row);
                    }
                }
            });
        }
        ui.separator();
        // 多选（工具 7）：房间集合存在时切"批量"面板（`08` §8.5.3）
        if !session.selection.rooms.is_empty() {
            ui.heading(format!("选区 ({} 个房间)", session.selection.rooms.len()));
            let mut reward = session.room().reward.clone().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("批量 reward");
                if ui.text_edit_singleline(&mut reward).changed() {
                    let trimmed = reward.trim().to_string();
                    session.batch_set_reward(if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui.button("批量 + 出口").clicked() {
                    let mut goals = session.room().goals.clone();
                    goals.push(level_format::Goal::ReachExit);
                    session.batch_set_goals(goals);
                }
                if ui.button("批量 + 收集全部").clicked() {
                    let mut goals = session.room().goals.clone();
                    goals.push(level_format::Goal::CollectAll);
                    session.batch_set_goals(goals);
                }
                if ui.button("清空选区").clicked() {
                    session.clear_selection();
                }
            });
            ui.separator();
        }
        ui.heading(format!(
            "选中房间 ({}, {})",
            session.current_room.0, session.current_room.1
        ));
        let mut reward = session.room().reward.clone().unwrap_or_else(String::new);
        ui.horizontal(|ui| {
            ui.label("reward");
            if ui.text_edit_singleline(&mut reward).changed() {
                session.begin_edit();
                let trimmed = reward.trim();
                session.set_reward(if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                });
            }
        });
        ui.separator();
        ui.heading("目标");
        for (index, goal) in session.room().goals.clone().into_iter().enumerate() {
            ui.horizontal(|ui| {
                match goal {
                    level_format::Goal::ReachExit => {
                        ui.label("到达出口");
                    }
                    level_format::Goal::CollectAll => {
                        ui.label("收集全部");
                    }
                    level_format::Goal::Collect { count } => {
                        ui.label("收集");
                        let mut value = count;
                        if ui
                            .add(egui::DragValue::new(&mut value).range(1..=255))
                            .changed()
                        {
                            session.begin_edit();
                            let mut goals = session.room().goals.clone();
                            goals[index] = level_format::Goal::Collect { count: value };
                            session.set_goals(goals);
                        }
                    }
                }
                if ui.button("x").clicked() {
                    session.begin_edit();
                    let mut goals = session.room().goals.clone();
                    goals.remove(index);
                    session.set_goals(goals);
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("+ 到达出口").clicked() {
                session.begin_edit();
                let mut goals = session.room().goals.clone();
                goals.push(level_format::Goal::ReachExit);
                session.set_goals(goals);
            }
            if ui.button("+ 收集").clicked() {
                session.begin_edit();
                let mut goals = session.room().goals.clone();
                goals.push(level_format::Goal::Collect { count: 1 });
                session.set_goals(goals);
            }
            if ui.button("+ 收集全部").clicked() {
                session.begin_edit();
                let mut goals = session.room().goals.clone();
                goals.push(level_format::Goal::CollectAll);
                session.set_goals(goals);
            }
        });
        ui.separator();
        ui.heading("实体笔刷");
        for kind in ["fruit", "exit", "checkpoint", "player_spawn"] {
            if ui
                .selectable_label(session.entity_kind == kind, kind)
                .clicked()
            {
                session.entity_kind = kind.to_string();
            }
        }
        ui.separator();
        let total: usize = session
            .doc
            .rooms
            .iter()
            .map(|room| room.entities.len())
            .sum();
        ui.heading(format!("实体 ({total})"));
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            let cols = session.doc.grid.cols;
            let mut pending_delete: Option<(usize, usize)> = None;
            let rooms: Vec<(usize, u32, u32, Vec<level_format::EntitySpec>)> = session
                .doc
                .rooms
                .iter()
                .enumerate()
                .filter(|(_, room)| !room.entities.is_empty())
                .map(|(index, room)| {
                    (
                        index,
                        index as u32 % cols,
                        index as u32 / cols,
                        room.entities.clone(),
                    )
                })
                .collect();
            for (room_index, col, row, entities) in rooms {
                ui.label(format!("-- 房间 ({col}, {row}) --"));
                for (entity_index, spec) in entities.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let selected = session.current_room == (col, row)
                            && session.selected_entity == Some(entity_index);
                        let label = format!(
                            "{entity_index} {} @({}, {})",
                            spec.kind, spec.at.0, spec.at.1
                        );
                        if ui.selectable_label(selected, label).clicked() {
                            session.goto_room(col, row);
                            session.selected_entity = Some(entity_index);
                        }
                        if ui.small_button("删除").clicked() {
                            pending_delete = Some((room_index, entity_index));
                        }
                    });
                }
            }
            if let Some((room_index, entity_index)) = pending_delete {
                session.delete_entity(room_index, entity_index);
            }
        });
        if ui.button("去重：出生点 / 出口").clicked() {
            session.dedupe_singletons();
        }
    });
}

/// 底部栏：保存/试玩/撤销/重做 + 路径 + 地块/动画两个标签页 + 外部改动 + 错误列表。
fn bottom_panel(
    ui: &mut egui::Ui,
    session: &mut EditorSession,
    playtest: &mut PlaytestRequest,
    tile_tool: &mut TileTool,
    anim_edit: Option<&mut AnimEdit>,
    sheet_texture: &SheetTexture,
    tile_texture: &TileTexture,
    bottom_tab: &mut BottomTab,
    bottom_height: &mut BottomHeight,
    picker: &mut crate::tile_tool::AssetPickerState,
) {
    egui::Panel::bottom("animj_editor_bottom")
        // 自绘分隔条 + 固定高度：面板永远吸附窗口底边（内置 resizable 在自定义根 Ui 下会脱离底边）
        .resizable(false)
        .exact_size(bottom_height.0)
        .show(ui, |ui| {
        // 拖拽分隔条（向上拖 = 展开）
        let (handle_rect, handle) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::drag());
        if handle.dragged() {
            bottom_height.drag(handle.drag_delta().y, ui.ctx().viewport_rect().height());
        }
        let active = handle.hovered() || handle.dragged();
        let color = if active {
            egui::Color32::LIGHT_GRAY
        } else {
            egui::Color32::DARK_GRAY
        };
        ui.painter().line_segment(
            [handle_rect.center_top(), handle_rect.center_bottom()],
            egui::Stroke::new(1.0, color),
        );
        handle.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        ui.horizontal(|ui| {
            if ui.button("保存 (Ctrl+S)").clicked() {
                session.status = match session.save() {
                    Ok(()) => "已保存".to_string(),
                    Err(error) => format!("保存失败：{error}"),
                };
            }
            if ui.button("试玩 (F5)").clicked() {
                playtest.0 = true;
            }
            if ui.button("撤销").clicked() {
                session.undo();
            }
            if ui.button("重做").clicked() {
                session.redo();
            }
            ui.label("路径");
            ui.add(
                egui::TextEdit::singleline(&mut session.save_path_input)
                    .desired_width(320.0),
            );
        });

        // 标签页（`08` §8.5.7）：地块调色板 / 角色动画
        ui.horizontal(|ui| {
            if ui
                .selectable_label(*bottom_tab == BottomTab::Tiles, "地块")
                .clicked()
            {
                *bottom_tab = BottomTab::Tiles;
            }
            if ui
                .selectable_label(*bottom_tab == BottomTab::Animation, "动画")
                .clicked()
            {
                *bottom_tab = BottomTab::Animation;
            }
        });
        let tab_height = (ui.available_height() - 130.0).max(100.0);
        egui::ScrollArea::vertical().max_height(tab_height).show(ui, |ui| {
            match *bottom_tab {
                BottomTab::Tiles => {
                    crate::tile_tool::tile_panel(
                        ui,
                        session,
                        tile_tool,
                        tile_texture.0.as_ref(),
                        picker,
                    )
                }
                BottomTab::Animation => match anim_edit {
                    Some(anim_edit) => {
                        crate::anim_tool::anim_panel(
                            ui,
                            anim_edit,
                            sheet_texture.0.as_ref(),
                            picker,
                        )
                    }
                    None => {
                        ui.label("动画数据尚未加载");
                    }
                },
            }
        });

        // 外部改动（`08` §8.7.4）：显式确认，绝不自动覆盖编辑缓冲
        if session.pending_external.is_some() {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(255, 200, 80), "检测到外部改动");
                if ui.button("应用").clicked() {
                    session.accept_external();
                }
                if ui.button("忽略").clicked() {
                    session.ignore_external();
                }
            });
        }

        egui::ScrollArea::vertical().max_height(96.0).show(ui, |ui| {
            for issue in session.file_issues.clone() {
                let label = format!("{:?} {} — {}", issue.severity, issue.path, issue.message);
                if ui.small_button(label).clicked() {
                    if let Some(room) = room_of_path(&issue.path, session.doc.grid.cols) {
                        session.current_room = room;
                        if let Some(world) =
                            cell_of_issue(&issue.message, &issue.path, room, session)
                        {
                            session.focus_world(world);
                        } else {
                            session.focus_camera = true;
                        }
                    }
                }
            }
        });
    });
}

/// 面板系统：一个根 Ui 上按 顶 → 底 → 左 → 右 显示四个面板。
fn panels(
    mut contexts: EguiContexts,
    mut session: ResMut<EditorSession>,
    mut playtest: ResMut<PlaytestRequest>,
    view: Res<crate::camera::EditorViewState>,
    camera_state: Option<Single<&Transform, With<EditorCamera>>>,
    mut anim_edit: Option<ResMut<AnimEdit>>,
    sheet_texture: Res<SheetTexture>,
    projection_entities: Query<&crate::view::EditorProjectionRoom>,
    tile_texture: Res<TileTexture>,
    mut tile_tool: ResMut<TileTool>,
    mut bottom_tab: ResMut<BottomTab>,
    mut bottom_height: ResMut<BottomHeight>,
    mut picker: ResMut<crate::tile_tool::AssetPickerState>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = ui_root(ctx);
    let cam = camera_state
        .as_ref()
        .map(|state| state.translation.truncate())
        .unwrap_or(Vec2::ZERO);
    top_bar(
        &mut root,
        &session,
        &view,
        cam,
        projection_entities.iter().count(),
    );
    bottom_panel(
        &mut root,
        &mut session,
        &mut playtest,
        &mut tile_tool,
        anim_edit.as_mut().map(|edit| &mut **edit),
        &sheet_texture,
        &tile_texture,
        &mut bottom_tab,
        &mut bottom_height,
        &mut picker,
    );
    left_panel(&mut root, &mut session, &tile_texture);
    right_panel(&mut root, &mut session);
    Ok(())
}

/// 从错误的 `path`/`message` 解析出具体瓦片的世界坐标（`08` §8.8.2）。
fn cell_of_issue(
    message: &str,
    path: &str,
    room: (u32, u32),
    session: &EditorSession,
) -> Option<Vec2> {
    // path 形如 `rooms[N].tiles[R]`；message 形如 `V6 ... 列 C ...`
    let file_row = path
        .split("tiles[")
        .nth(1)?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<usize>()
        .ok()?;
    let col = message
        .split("列 ")
        .nth(1)?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<f32>()
        .ok()?;
    let rows = session.doc.grid.rows;
    let local = Vec2::new(
        col * level_format::TILE_PX as f32 + level_format::TILE_PX as f32 / 2.0,
        file_row as f32 * level_format::TILE_PX as f32 + level_format::TILE_PX as f32 / 2.0,
    );
    Some(game::coords::room_local_to_world(room.0, room.1, rows, local))
}

/// 从 `rooms[N]...` 解析出行主序房间（`08` §8.8.2 的点击定位）。
fn room_of_path(path: &str, cols: u32) -> Option<(u32, u32)> {
    let rest = path.strip_prefix("rooms[")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let index: u32 = digits.parse().ok()?;
    if cols == 0 {
        return None;
    }
    Some((index % cols, index / cols))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_height_drag_clamps() {
        let mut height = BottomHeight(300.0);
        height.drag(-100.0, 720.0);
        assert!((height.0 - 400.0).abs() < 1e-3, "向上拖应变高：{}", height.0);
        height.drag(10_000.0, 720.0);
        assert!((height.0 - BottomHeight::MIN).abs() < 1e-3, "不得低于最小值：{}", height.0);
        height.drag(-10_000.0, 720.0);
        assert!((height.0 - 560.0).abs() < 1e-3, "不得超过 viewport-160：{}", height.0);
    }

    #[test]
    fn bottom_tab_defaults_to_tiles() {
        assert_eq!(BottomTab::default(), BottomTab::Tiles);
    }

    #[test]
    fn tool_labels_are_chinese() {
        for (index, tool) in Tool::ALL.into_iter().enumerate() {
            let label = tool.label();
            let first = label.chars().next().expect("标签非空");
            assert!(!first.is_ascii_alphabetic(), "工具标签应为中文：{label}");
            assert!(
                label.contains(char::from(b'1' + index as u8)),
                "工具标签应含序号：{label}"
            );
        }
    }

    #[test]
    fn cell_of_issue_maps_path_and_column_to_world() {
        let level = level_format::Level {
            version: level_format::VERSION,
            name: "t".into(),
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            grid: level_format::Grid { cols: 2, rows: 3 },
            start: level_format::RoomCoord { col: 0, row: 0 },
            rooms: (0..6)
                .map(|_| level_format::Room {
                    name: None,
                    reward: None,
                    spawn: None,
                    goals: vec![level_format::Goal::ReachExit],
                    tiles: vec![".".repeat(16); 16],
                    entities: vec![],
                })
                .collect(),
        };
        let session = EditorSession::new(level, None);
        // `rooms[1].tiles[3]` + 「列 5」⇒ 房间 (1,0) 的第 3 文件行、第 5 列的中心
        let world = cell_of_issue(
            "V6 rooms[1].tiles[3]: 列 5 的字符 'x' 不在调色板中",
            "rooms[1].tiles[3]",
            (1, 0),
            &session,
        )
        .expect("应能解析");
        assert_eq!(
            world,
            game::coords::room_local_to_world(
                1,
                0,
                3,
                Vec2::new(5.0 * 32.0 + 16.0, 3.0 * 32.0 + 16.0)
            )
        );
        // 没有列号 ⇒ None（退回房间级定位）
        assert!(cell_of_issue("V12 ...", "rooms[1].entities[0].at", (1, 0), &session).is_none());
    }

    #[test]
    fn room_of_path_is_row_major() {
        assert_eq!(room_of_path("rooms[4].tiles[0]", 3), Some((1, 1)));
        assert_eq!(room_of_path("rooms[29]", 10), Some((9, 2)));
        assert_eq!(room_of_path("palette[0]", 10), None);
        assert_eq!(room_of_path("rooms[9].at", 0), None);
    }
}
