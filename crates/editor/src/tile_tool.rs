//! 地块工具：编辑关卡调色板（v2，`03` §3.9）与贴图切分（`08` §8.5.7 B）。
//!
//! 与关卡编辑同纪律：每次修改都是一个 undo 步骤；保存前由 `validate` 把关（V6/V19）。

use bevy::prelude::*;
use bevy_egui::egui;
use level_format::{Behavior, CollisionShape, MAX_COLLISION_VERTICES, PaletteEntry};

use crate::session::EditorSession;

/// 已注册到 egui 的**关卡瓦片图集**纹理（内置图集或关卡自带 `tileset`）。
#[derive(Resource, Default)]
pub struct TileTexture(pub Option<egui::TextureHandle>);

/// 上传瓦片图集（与动画面板同一机制：`ColorImage` + `NEAREST`）；句柄变化时重建。
pub fn ensure_tile_texture(
    ctx: &egui::Context,
    sheets: &Assets<Image>,
    handle: &Handle<Image>,
    store: &mut TileTexture,
    current: &mut Option<Handle<Image>>,
) {
    if store.0.is_some() && current.as_ref() == Some(handle) {
        return;
    }
    let Some(image) = sheets.get(handle) else {
        return;
    };
    let (width, height) = (image.width() as usize, image.height() as usize);
    let Some(data) = image.data.as_ref() else {
        return;
    };
    let pixels: Vec<[u8; 4]> = data
        .chunks_exact(4)
        .map(|px| [px[0], px[1], px[2], px[3]])
        .collect();
    if pixels.len() != width * height {
        return;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels.concat());
    store.0 = Some(ctx.load_texture("tile_sheet", color, egui::TextureOptions::NEAREST));
    *current = Some(handle.clone());
}

#[derive(Resource, Debug)]
pub struct TileTool {
    pub selected: Option<usize>,
    pub new_char: String,
    pub new_name: String,
    pub new_behavior: Behavior,
    pub new_cell: u32,
    /// 贴图框选的起点（屏幕坐标，按 32 px 格吸附）。
    pub pick_drag_start: Option<egui::Pos2>,
    /// 碰撞多边形顶点拖拽：拖拽中只改草稿，松开时提交为**一个** undo 步骤。
    pub collision_drag: Option<CollisionDrag>,
}

/// 正在拖拽的碰撞多边形顶点（`08` §8.5.7 B）。
#[derive(Debug, Clone)]
pub struct CollisionDrag {
    pub entry: usize,
    pub vertex: usize,
    pub vertices: Vec<(i32, i32)>,
}

impl Default for TileTool {
    fn default() -> Self {
        Self {
            selected: None,
            new_char: String::new(),
            new_name: String::new(),
            new_behavior: Behavior::Solid,
            new_cell: 4,
            pick_drag_start: None,
            collision_drag: None,
        }
    }
}

const BEHAVIORS: [Behavior; 4] = [
    Behavior::Solid,
    Behavior::OneWay,
    Behavior::Hazard,
    Behavior::None,
];

fn behavior_label(behavior: Behavior) -> &'static str {
    match behavior {
        Behavior::Solid => "实心",
        Behavior::OneWay => "单向",
        Behavior::Hazard => "危险",
        Behavior::None => "无",
    }
}

/// 「浏览…」文件选择器的状态（一次只开一个）。
#[derive(Resource, Default)]
pub struct AssetPickerState {
    /// `(字段 id, 当前目录（相对 assets/）)`
    pub open: Option<(&'static str, std::path::PathBuf)>,
}

/// 按可用宽度计算横向列数与等宽单元格宽度（左栏瓦片选择器 / 地块卡片共用）。
///
/// `columns = max(1, floor((available + spacing) / (min_item + spacing)))`；
/// 返回的宽度把剩余空间平均分给每一列，保证行末不留半格。
pub fn responsive_columns(available: f32, spacing: f32, min_item: f32) -> (usize, f32) {
    let columns = (((available + spacing) / (min_item + spacing)).floor() as usize).max(1);
    let item_w =
        ((available - spacing * columns.saturating_sub(1) as f32) / columns as f32).max(1.0);
    (columns, item_w)
}

/// 资源路径选择器：按钮 + 内嵌目录列表（只列目录与 `.png`）。
///
/// 返回被点选的**相对 assets/** 路径。用内嵌列表而不是系统对话框：
/// 不引入额外依赖，且和 `tileset`/动画图集的路径约定完全一致。
pub fn asset_picker(
    ui: &mut egui::Ui,
    state: &mut AssetPickerState,
    id: &'static str,
) -> Option<String> {
    let mut picked = None;
    if ui.button("浏览…").clicked() {
        state.open = if state.open.as_ref().is_some_and(|(open, _)| *open == id) {
            None
        } else {
            Some((
                id,
                state
                    .open
                    .as_ref()
                    .map(|(_, dir)| dir.clone())
                    .unwrap_or_default(),
            ))
        };
    }
    let Some((open_id, mut dir)) = state.open.clone() else {
        return None;
    };
    if open_id != id {
        return None;
    }
    let root = std::path::PathBuf::from(game::assets::asset_file_path());
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(format!("assets/{}", dir.display()));
            if ui.small_button("上一级").clicked() {
                dir.pop();
                state.open = Some((id, dir.clone()));
            }
            if ui.small_button("关闭").clicked() {
                state.open = None;
            }
        });
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                let mut entries: Vec<(String, bool)> = Vec::new();
                if let Ok(read) = std::fs::read_dir(root.join(&dir)) {
                    for entry in read.flatten() {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        if path.is_dir() {
                            entries.push((name, true));
                        } else if path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                        {
                            entries.push((name, false));
                        }
                    }
                }
                entries.sort();
                for (name, is_dir) in entries {
                    let label = if is_dir {
                        format!("[目录] {name}")
                    } else {
                        name.clone()
                    };
                    if ui.selectable_label(false, label).clicked() {
                        if is_dir {
                            state.open = Some((id, dir.join(&name)));
                        } else {
                            picked = Some(dir.join(&name).to_string_lossy().replace('\\', "/"));
                            state.open = None;
                        }
                    }
                }
            });
    });
    picked
}

/// 新地块的自动编号：`1..9` → `a..z` → `A..Z`，取第一个未被占用的字符。
pub fn next_free_char(doc: &level_format::Level) -> char {
    let used: Vec<char> = doc.palette().iter().map(|entry| entry.char).collect();
    ('1'..='9')
        .chain('a'..='z')
        .chain('A'..='Z')
        .find(|candidate| *candidate != '.' && !used.contains(candidate))
        .unwrap_or('~')
}

/// 取贴图里 `cell` 对应的 32×32 格 UV。
fn cell_uv(cell: u32, columns: u32, sheet: egui::Vec2) -> egui::Rect {
    let px = 32.0;
    let columns = columns.max(1);
    let (col, row) = (cell % columns, cell / columns);
    egui::Rect::from_min_size(
        egui::pos2((col as f32 * px) / sheet.x, (row as f32 * px) / sheet.y),
        egui::vec2(px / sheet.x, px / sheet.y),
    )
}

/// 地块卡片最小宽度；横向列数由此与可用宽度推导。见 `08` §8.5.7 B。
const TILE_CARD_MIN_W: f32 = 200.0;

/// 面板：贴图路径/列数 + 调色板列表 + 新建地块 + 32×32 网格选格。
pub fn tile_panel(
    ui: &mut egui::Ui,
    session: &mut EditorSession,
    tool: &mut TileTool,
    texture: Option<&egui::TextureHandle>,
    picker: &mut AssetPickerState,
) {
    // 横向分区：左 = 图集设置 + 地块列表，右 = 选中地块编辑 + 新建地块。
    ui.columns(2, |panels| {
        // ---------------- 左栏：图集设置 + 现有地块 ----------------
        let ui = &mut panels[0];
        ui.heading("地块调色板");

        // ---- 贴图 ----
        let mut path = session.doc.tileset.clone().unwrap_or_default();
        ui.label("贴图路径（相对 assets/，留空 = 内置图集）");
        let width = ui.available_width();
        let response = ui.add(
            egui::TextEdit::singleline(&mut path)
                .desired_width(width)
                .hint_text("sprites/tiles/tileset.png"),
        );
        if response.changed() {
            session.begin_edit();
            let trimmed = path.trim().to_string();
            session.doc.tileset = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
        }
        // 失焦时把 `assets/xxx`、反斜杠等写法统一成相对 `assets/` 的形式
        if response.lost_focus() {
            let normalized = level_format::normalize_asset_path(&path);
            if session.doc.tileset != normalized {
                session.begin_edit();
                session.doc.tileset = normalized;
            }
        }
        if let Some(picked) = asset_picker(ui, picker, "tileset") {
            session.begin_edit();
            session.doc.tileset = Some(picked);
        }
        let mut columns = session.doc.tileset_columns.max(1);
        ui.horizontal(|ui| {
            ui.label("列数");
            if ui
                .add(egui::DragValue::new(&mut columns).range(1..=64))
                .changed()
            {
                session.begin_edit();
                session.doc.tileset_columns = columns;
            }
        });
        ui.separator();

        // ---- 现有地块（按当前栏宽自适应列数）----
        let palette = session.doc.palette();
        let sheet = texture.map(|texture| texture.size_vec2());
        let spacing = ui.spacing().item_spacing.x;
        let available = ui.available_width();
        let (columns_n, card_w) = responsive_columns(available, spacing, TILE_CARD_MIN_W);
        // 显式计算列数后按行切分：保证每行列数固定、卡片等宽（不再依赖自动换行的取整差异）。
        for (row_index, chunk) in palette.chunks(columns_n).enumerate() {
            ui.horizontal(|ui| {
                for (col_index, entry) in chunk.iter().enumerate() {
                    let index = row_index * columns_n + col_index;
                    let selected = tool.selected == Some(index);
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.horizontal(|ui| {
                                // 单格缩略图：直接看清该地块用的是哪一格
                                if let (Some(texture), Some(sheet)) = (texture, sheet) {
                                    ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(
                                            texture.id(),
                                            sheet,
                                        ))
                                        .uv(cell_uv(entry.cell, columns, sheet))
                                        .fit_to_exact_size(egui::vec2(32.0, 32.0))
                                        // 单格裁切后必须占满 32×32；否则会按整张图集的宽高比压扁
                                        .maintain_aspect_ratio(false),
                                    );
                                }
                                ui.vertical(|ui| {
                                    let label = format!("{}  {}", entry.char, entry.name);
                                    if ui
                                        .selectable_label(selected, label.clone())
                                        .on_hover_text(&label)
                                        .clicked()
                                    {
                                        tool.selected = Some(index);
                                    }
                                    ui.label(format!(
                                        "{} · 格 {}",
                                        behavior_label(entry.behavior),
                                        entry.cell
                                    ));
                                });
                            });
                            if ui.small_button("删除").clicked() {
                                session.remove_palette_entry(index);
                                tool.selected = None;
                            }
                        },
                    );
                }
            });
        }

        // ---------------- 右栏：选中地块编辑 + 新建地块 ----------------
        let ui = &mut panels[1];
        ui.heading("选中地块");
        let Some(index) = tool.selected else {
            ui.label("在上方列表中选择一个地块，即可编辑行为与贴图格。");
            ui.separator();
            new_tile_form(ui, session, tool);
            return;
        };
        let Some(entry) = session.doc.palette().get(index).cloned() else {
            tool.selected = None;
            ui.label("选中的地块已不存在。");
            return;
        };

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("编辑 {}", entry.char));
            let mut behavior = entry.behavior;
            for candidate in BEHAVIORS {
                if ui
                    .selectable_label(behavior == candidate, behavior_label(candidate))
                    .clicked()
                {
                    behavior = candidate;
                }
            }
            if behavior != entry.behavior {
                session.set_palette_behavior(index, behavior);
            }
        });
        ui.horizontal(|ui| {
            ui.label("贴图格");
            let mut cell = entry.cell;
            if ui
                .add(egui::DragValue::new(&mut cell).range(0..=255))
                .changed()
            {
                session.set_palette_cell(index, cell);
            }
        });
        ui.separator();
        collision_editor(ui, session, tool, index, &entry, texture);
        ui.separator();
        // 整张贴图 + 点击/拖动框选（吸附到 32×32 格，`08` §8.5.7 B）
        ui.label("选择贴图格：点击或拖动框选（吸附到 32×32）");
        let sheet = texture.map(|texture| texture.size_vec2());
        if let (Some(texture), Some(sheet)) = (texture, sheet) {
            let cell = 32.0;
            let cols = (sheet.x / cell).round().max(1.0) as usize;
            let rows = (sheet.y / cell).round().max(1.0) as usize;
            if cols as u32 != columns {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("警告：贴图宽 {cols} 列，但文档列数为 {columns}"),
                );
            }
            // 按当前列宽缩放整张图集，避免在横向分栏里溢出
            let available = (ui.available_width() - 8.0).max(32.0);
            let scale = (available / sheet.x).clamp(0.25, 4.0);
            let response = ui.add(
                egui::Image::new(egui::load::SizedTexture::new(texture.id(), sheet))
                    .fit_to_exact_size(egui::vec2(sheet.x * scale, sheet.y * scale))
                    .sense(egui::Sense::click_and_drag()),
            );
            // 指针 → 贴图像素 → 吸附到 32 px 格
            let snap = |pos: egui::Pos2| -> (usize, usize, egui::Vec2) {
                let local = ((pos - response.rect.min) / scale).max(egui::Vec2::ZERO);
                let col = ((local.x / cell).floor() as isize).clamp(0, cols as isize - 1) as usize;
                let row = ((local.y / cell).floor() as isize).clamp(0, rows as isize - 1) as usize;
                (col, row, local)
            };
            if response.drag_started() {
                tool.pick_drag_start = response.interact_pointer_pos();
            }
            if let Some(pos) = response.interact_pointer_pos() {
                let (col, row, local) = snap(pos);
                if response.clicked() || response.dragged() {
                    // 索引按**文档列数**（渲染/物理都用它），网格按贴图实际列数
                    let index_cell = (row * columns as usize + col).min(255) as u32;
                    if entry.cell != index_cell {
                        session.set_palette_cell(index, index_cell);
                    }
                }
                // 悬停格高亮 + 像素读数（框选时也显示当前格）
                if response.hovered() || response.dragged() {
                    let rect = egui::Rect::from_min_size(
                        response.rect.min + egui::vec2(col as f32, row as f32) * cell * scale,
                        egui::vec2(cell, cell) * scale,
                    );
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                    ui.label(format!(
                        "指针：像素 ({:.0}, {:.0}) → 格 (列 {col}, 行 {row})，已吸附 32 px",
                        local.x, local.y
                    ));
                }
            }
            // 框选矩形：从按下点到当前点，四边吸附到格线
            if response.dragged()
                && let (Some(start), Some(pos)) =
                    (tool.pick_drag_start, response.interact_pointer_pos())
            {
                let (c0, r0, _) = snap(start);
                let (c1, r1, _) = snap(pos);
                let (cmin, cmax) = (c0.min(c1), c0.max(c1) + 1);
                let (rmin, rmax) = (r0.min(r1), r0.max(r1) + 1);
                let rect = egui::Rect::from_min_size(
                    response.rect.min + egui::vec2(cmin as f32 * cell, rmin as f32 * cell) * scale,
                    egui::vec2((cmax - cmin) as f32 * cell, (rmax - rmin) as f32 * cell) * scale,
                );
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 220, 255)),
                    egui::StrokeKind::Inside,
                );
            }
            if response.drag_stopped() {
                tool.pick_drag_start = None;
            }
            let current = session
                .doc
                .palette()
                .get(index)
                .map(|entry| entry.cell)
                .unwrap_or(entry.cell);
            let (cur_col, cur_row) = (current as usize % cols, current as usize / cols);
            let rect = egui::Rect::from_min_size(
                response.rect.min
                    + egui::vec2(cur_col as f32 * cell, cur_row as f32 * cell) * scale,
                egui::vec2(cell, cell) * scale,
            );
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(2.0, egui::Color32::YELLOW),
                egui::StrokeKind::Inside,
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(texture.id(), sheet))
                        .uv(cell_uv(current, columns, sheet))
                        .fit_to_exact_size(egui::vec2(32.0, 32.0))
                        .maintain_aspect_ratio(false),
                );
                ui.label(format!(
                    "当前格：{current}（列 {cur_col} / 行 {cur_row}，32×32）"
                ));
            });
        } else {
            ui.label("尚无贴图纹理：可先设置路径，或直接用数字选择贴图格");
            let mut cell = entry.cell;
            if ui
                .add(egui::DragValue::new(&mut cell).range(0..=255))
                .changed()
            {
                session.set_palette_cell(index, cell);
            }
        }

        ui.separator();
        new_tile_form(ui, session, tool);
    });
}

/// 选中地块的碰撞范围编辑（`03` §3.9 / `08` §8.5.7 B）。
///
/// 顶点坐标与关卡文件一致：**瓦片局部像素，原点左上，+Y 向下**。物理按像素中心栅格化，
/// 因此斜边呈阶梯；预览里的绿色块就是实际参与碰撞的矩形。
fn collision_editor(
    ui: &mut egui::Ui,
    session: &mut EditorSession,
    tool: &mut TileTool,
    index: usize,
    entry: &PaletteEntry,
    texture: Option<&egui::TextureHandle>,
) {
    ui.heading("碰撞范围");
    ui.label("顶点 = 瓦片局部像素（0..32，左上原点，+Y 向下）；物理按像素栅格化，斜边呈阶梯。");

    let mut collision = entry.collision.clone();
    ui.horizontal(|ui| {
        if ui.button("整格（默认 32×32）").clicked() && collision.is_some() {
            session.set_palette_collision(index, None);
            collision = None;
            tool.collision_drag = None;
        }
        if ui.button("自定义多边形").clicked() && collision.is_none() {
            let shape = CollisionShape::full();
            if session.set_palette_collision(index, Some(shape.clone())) {
                collision = Some(shape);
            }
        }
    });
    let Some(mut shape) = collision else {
        ui.label("当前使用整格碰撞；点「自定义多边形」后可编辑顶点。");
        return;
    };
    if entry.behavior == Behavior::None {
        ui.colored_label(
            egui::Color32::YELLOW,
            "行为为「无」：该地块不参与物理，碰撞范围不会生效。",
        );
    }

    // 常用预设：一次点击 = 一个 undo 步骤
    ui.horizontal_wrapped(|ui| {
        for (label, preset) in [
            ("下半", CollisionShape::rectangle(0, 16, 32, 16)),
            ("上半", CollisionShape::rectangle(0, 0, 32, 16)),
            ("左半", CollisionShape::rectangle(0, 0, 16, 32)),
            ("右半", CollisionShape::rectangle(16, 0, 16, 32)),
        ] {
            if ui.small_button(label).clicked()
                && session.set_palette_collision(index, Some(preset.clone()))
            {
                shape = preset;
                tool.collision_drag = None;
            }
        }
    });

    let columns = session.doc.tileset_columns.max(1);
    let sheet = texture.map(|texture| texture.size_vec2());
    let side = ui.available_width().clamp(96.0, 180.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
    let tile_to_screen = |x: f32, y: f32| {
        egui::pos2(
            rect.min.x + x / level_format::TILE_PX as f32 * side,
            rect.min.y + y / level_format::TILE_PX as f32 * side,
        )
    };
    let screen_to_tile = |pos: egui::Pos2| {
        let x = ((pos.x - rect.min.x) / side * level_format::TILE_PX as f32)
            .clamp(0.0, level_format::TILE_PX as f32);
        let y = ((pos.y - rect.min.y) / side * level_format::TILE_PX as f32)
            .clamp(0.0, level_format::TILE_PX as f32);
        (x.round() as i32, y.round() as i32)
    };
    let nearest_vertex = |vertices: &[(i32, i32)], pos: egui::Pos2| -> Option<usize> {
        vertices
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| (i, tile_to_screen(x as f32, y as f32).distance(pos)))
            .filter(|(_, distance)| *distance <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };

    // 拖拽只改草稿；松开时一次提交（一个 undo 步骤，`08` §8.5.2）
    let displayed = tool
        .collision_drag
        .as_ref()
        .filter(|drag| drag.entry == index)
        .map(|drag| drag.vertices.clone())
        .unwrap_or_else(|| shape.vertices.clone());
    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(vertex) = nearest_vertex(&displayed, pos)
    {
        tool.collision_drag = Some(CollisionDrag {
            entry: index,
            vertex,
            vertices: displayed.clone(),
        });
    }
    if response.dragged()
        && let Some(drag) = tool.collision_drag.as_mut()
        && drag.entry == index
        && let Some(pos) = response.interact_pointer_pos()
    {
        drag.vertices[drag.vertex] = screen_to_tile(pos);
    }
    if response.drag_stopped()
        && let Some(drag) = tool.collision_drag.take()
        && drag.entry == index
        && drag.vertices != shape.vertices
        && session.set_palette_collision(
            index,
            Some(CollisionShape {
                vertices: drag.vertices.clone(),
            }),
        )
    {
        shape.vertices = drag.vertices;
    }

    let displayed = tool
        .collision_drag
        .as_ref()
        .filter(|drag| drag.entry == index)
        .map(|drag| drag.vertices.clone())
        .unwrap_or_else(|| shape.vertices.clone());

    // ---- 预览：贴图 + 8 px 参考网格 + 实际栅格碰撞块 + 多边形轮廓 ----
    if let (Some(texture), Some(sheet)) = (texture, sheet) {
        ui.painter().image(
            texture.id(),
            rect,
            cell_uv(entry.cell, columns, sheet),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::from_gray(36));
    }
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40);
        let stroke = egui::Stroke::new(1.0, color);
        let offset = side * t;
        ui.painter().line_segment(
            [
                egui::pos2(rect.min.x + offset, rect.min.y),
                egui::pos2(rect.min.x + offset, rect.max.y),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(rect.min.x, rect.min.y + offset),
                egui::pos2(rect.max.x, rect.min.y + offset),
            ],
            stroke,
        );
    }
    let preview_shape = CollisionShape {
        vertices: displayed.clone(),
    };
    let parts = preview_shape.raster_parts();
    for part in &parts {
        let min = tile_to_screen(part.x as f32, part.y as f32);
        let max = tile_to_screen((part.x + part.w) as f32, (part.y + part.h) as f32);
        ui.painter().rect_filled(
            egui::Rect::from_min_max(min, max),
            0.0,
            egui::Color32::from_rgba_unmultiplied(80, 220, 120, 70),
        );
    }
    if displayed.len() >= 2 {
        ui.painter().add(egui::Shape::closed_line(
            displayed
                .iter()
                .map(|&(x, y)| tile_to_screen(x as f32, y as f32))
                .collect(),
            egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 220, 255)),
        ));
    }
    for &(x, y) in &displayed {
        ui.painter().circle_filled(
            tile_to_screen(x as f32, y as f32),
            4.0,
            egui::Color32::from_rgb(255, 240, 130),
        );
    }

    // 单击空白处追加一个顶点（单击已有点不追加）
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && nearest_vertex(&displayed, pos).is_none()
        && shape.vertices.len() < MAX_COLLISION_VERTICES
    {
        let mut vertices = shape.vertices.clone();
        vertices.push(screen_to_tile(pos));
        // 下一帧会从 `doc` 重新读到新顶点
        session.set_palette_collision(index, Some(CollisionShape { vertices }));
    }

    if parts.is_empty() {
        ui.colored_label(
            egui::Color32::YELLOW,
            "当前多边形不覆盖任何像素中心 ⇒ 实际没有碰撞区域。",
        );
    }
    let geometry_errors = preview_shape.geometry_errors();
    if let Some(error) = geometry_errors.first() {
        ui.colored_label(egui::Color32::RED, error);
    }
    ui.label(format!(
        "顶点 {} / {}；实际碰撞块 {} 个",
        displayed.len(),
        MAX_COLLISION_VERTICES,
        parts.len()
    ));

    // ---- 顶点数值编辑 ----
    let mut pending_vertices: Option<Vec<(i32, i32)>> = None;
    let mut remove: Option<usize> = None;
    for (vertex, (x, y)) in shape.vertices.iter().copied().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("#{vertex}"));
            let mut vx = x;
            let mut vy = y;
            let changed_x = ui
                .add(egui::DragValue::new(&mut vx).range(0..=level_format::TILE_PX))
                .changed();
            let changed_y = ui
                .add(egui::DragValue::new(&mut vy).range(0..=level_format::TILE_PX))
                .changed();
            if changed_x || changed_y {
                let mut vertices = shape.vertices.clone();
                vertices[vertex] = (vx, vy);
                pending_vertices = Some(vertices);
            }
            // 少于 3 个点无法构成多边形：不允许再删
            if shape.vertices.len() > 3 && ui.small_button("删除").clicked() {
                remove = Some(vertex);
            }
        });
    }
    if let Some(vertex) = remove {
        let mut vertices = shape.vertices.clone();
        vertices.remove(vertex);
        pending_vertices = Some(vertices);
        tool.collision_drag = None;
    }
    if let Some(vertices) = pending_vertices {
        session.set_palette_collision(index, Some(CollisionShape { vertices }));
        tool.collision_drag = None;
    }
}

/// 新建地块表单（底部横向面板的右栏）。
fn new_tile_form(ui: &mut egui::Ui, session: &mut EditorSession, tool: &mut TileTool) {
    ui.heading("新建地块");
    // 编号自动生成：为空或已被占用时，每帧重算为"第一个空闲编号"
    let used = session
        .doc
        .palette()
        .iter()
        .any(|entry| entry.char == tool.new_char.chars().next().unwrap_or('\0'));
    if tool.new_char.is_empty() || used {
        tool.new_char = next_free_char(&session.doc).to_string();
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("编号（自动，可改）");
        ui.add(egui::TextEdit::singleline(&mut tool.new_char).desired_width(24.0));
        ui.label("名称");
        ui.text_edit_singleline(&mut tool.new_name);
    });
    ui.horizontal_wrapped(|ui| {
        for candidate in BEHAVIORS {
            if ui
                .selectable_label(tool.new_behavior == candidate, behavior_label(candidate))
                .clicked()
            {
                tool.new_behavior = candidate;
            }
        }
    });
    let mut cell = tool.new_cell;
    ui.horizontal(|ui| {
        ui.label("cell");
        if ui
            .add(egui::DragValue::new(&mut cell).range(0..=255))
            .changed()
        {
            tool.new_cell = cell;
        }
    });
    if ui.button("添加地块").clicked()
        && let mut chars = tool.new_char.chars()
        && let (Some(ch), None) = (chars.next(), chars.next())
    {
        let name = tool.new_name.trim().to_string();
        let added = session.add_palette_entry(PaletteEntry {
            char: ch,
            name: if name.is_empty() {
                format!("瓦片{ch}")
            } else {
                name
            },
            behavior: tool.new_behavior,
            cell: tool.new_cell,
            collision: None,
        });
        if added {
            tool.new_char.clear();
            tool.new_name.clear();
            session.brush_char = ch;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> EditorSession {
        let mut level = level_format::Level {
            version: level_format::VERSION,
            name: "t".into(),
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            grid: level_format::Grid { cols: 1, rows: 1 },
            start: level_format::RoomCoord { col: 0, row: 0 },
            rooms: vec![level_format::Room {
                name: None,
                reward: None,
                spawn: None,
                goals: vec![level_format::Goal::ReachExit],
                tiles: vec![".".repeat(16); 16],
                entities: vec![],
            }],
        };
        level.rooms[0].tiles[15] = "#".repeat(16);
        EditorSession::new(level, None)
    }

    #[test]
    fn next_free_char_skips_builtin_and_existing() {
        let mut session = session();
        // 内置调色板用 # = ^ * ⇒ 第一个空闲编号是 '1'
        assert_eq!(next_free_char(&session.doc), '1');
        session.add_palette_entry(PaletteEntry {
            char: '1',
            name: "瓦片1".into(),
            behavior: Behavior::Solid,
            cell: 4,
            collision: None,
        });
        assert_eq!(next_free_char(&session.doc), '2');
        // 用完 1..9 后进入字母段
        for ch in '2'..='9' {
            session.add_palette_entry(PaletteEntry {
                char: ch,
                name: String::new(),
                behavior: Behavior::Solid,
                cell: 4,
                collision: None,
            });
        }
        assert_eq!(next_free_char(&session.doc), 'a');
    }

    #[test]
    fn add_tile_is_validated_and_one_undo_step() {
        let mut session = session();
        // 内置调色板里已有 `#`，重复字符必须拒绝
        assert!(!session.add_palette_entry(PaletteEntry {
            char: '#',
            name: "dup".into(),
            behavior: Behavior::Solid,
            cell: 0,
            collision: None,
        }));
        // `.` 是空气保留
        assert!(!session.add_palette_entry(PaletteEntry {
            char: '.',
            name: "air".into(),
            behavior: Behavior::None,
            cell: 0,
            collision: None,
        }));
        // 新字符成功，且一个 undo 步骤可回退
        assert!(session.add_palette_entry(PaletteEntry {
            char: 'S',
            name: "stone".into(),
            behavior: Behavior::Solid,
            cell: 5,
            collision: None,
        }));
        assert_eq!(session.doc.behavior_of('S'), Some(Behavior::Solid));
        assert!(session.undo());
        assert_eq!(session.doc.behavior_of('S'), None);
    }

    #[test]
    fn palette_edits_change_behavior_and_cell() {
        let mut session = session();
        let palette = session.doc.palette();
        let index = palette.iter().position(|entry| entry.char == '#').unwrap();
        session.set_palette_behavior(index, Behavior::OneWay);
        session.set_palette_cell(index, 9);
        let entry = session.doc.entry_of('#').unwrap();
        assert_eq!(entry.behavior, Behavior::OneWay);
        assert_eq!(entry.cell, 9);
    }

    /// 碰撞范围编辑：一次交互一个 undo 步骤；重复写入同一值不产生步骤；可清回整格。
    #[test]
    fn palette_collision_edits_are_one_undo_step() {
        let mut session = session();
        let index = session
            .doc
            .palette()
            .iter()
            .position(|entry| entry.char == '#')
            .unwrap();
        let shape = CollisionShape::rectangle(0, 16, 32, 16);
        let undo_before = session.undo.len();
        assert!(session.set_palette_collision(index, Some(shape.clone())));
        assert_eq!(
            session.undo.len(),
            undo_before + 1,
            "必须恰好一个 undo 步骤"
        );
        assert_eq!(
            session.doc.entry_of('#').unwrap().collision,
            Some(shape.clone())
        );

        // 幂等：同一形状再写一次不产生步骤
        assert!(!session.set_palette_collision(index, Some(shape.clone())));
        assert_eq!(session.undo.len(), undo_before + 1);

        // 撤销回到整格
        assert!(session.undo());
        assert_eq!(session.doc.entry_of('#').unwrap().collision, None);

        // 再设一次，然后清回整格，也可整步撤销
        assert!(session.set_palette_collision(index, Some(shape.clone())));
        assert!(session.set_palette_collision(index, None));
        assert_eq!(session.doc.entry_of('#').unwrap().collision, None);
        assert!(session.undo());
        assert_eq!(session.doc.entry_of('#').unwrap().collision, Some(shape));
        // 越界下标安全
        assert!(!session.set_palette_collision(999, None));
    }

    #[test]
    fn painting_a_custom_char_writes_it() {
        let mut session = session();
        session.add_palette_entry(PaletteEntry {
            char: 'S',
            name: "stone".into(),
            behavior: Behavior::Solid,
            cell: 5,
            collision: None,
        });
        session.brush_char = 'S';
        session.begin_edit();
        session.set_tile_char(3, 2, session.brush_char);
        assert_eq!(session.tile_char(3, 2), Some('S'));
        // 写入的是字符本身，物理/渲染由调色板解释
        assert_eq!(session.doc.behavior_of('S'), Some(Behavior::Solid));
    }

    /// 列数随可用宽度单调增加；等宽单元格相加恰好填满（误差 < 0.01）。
    #[test]
    fn responsive_columns_fit_the_available_width() {
        let spacing = 8.0;
        let min = 72.0;
        for (available, expected_columns) in
            [(50.0, 1), (150.0, 1), (200.0, 2), (300.0, 3), (380.0, 4)]
        {
            let (columns, width) = responsive_columns(available, spacing, min);
            assert_eq!(columns, expected_columns, "available={available}");
            let used = width * columns as f32 + spacing * (columns - 1) as f32;
            assert!((used - available).abs() < 0.01, "available={available}");
        }
    }
}
