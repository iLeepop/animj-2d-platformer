//! 动画编辑工具：编辑 `assets/anim/player.anim.ron`（`04` §5.5、`08` §8.5.7 A）。
//!
//! 纪律与关卡编辑一致：**单一序列化入口**、**保存前必校验**、**`.tmp` + rename 原子写**。
//! 本模块只做数据与面板；预览与游戏共用同一份数据（保存后热重载即生效）。

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::egui;
use game::anim_data::{PlayerAnimSet, StateAnim};

/// 动画数据的相对路径（`assets/` 之下，与 `game::anim_data::ANIM_PATH` 一致）。
pub const ANIM_REL_PATH: &str = "anim/player.anim.ron";

/// 已注册到 egui 的角色图集纹理（每帧复用，避免重复上传）。
#[derive(Resource, Default)]
pub struct SheetTexture(pub Option<egui::TextureHandle>);

/// 用 bevy 的 `Image` 数据在 egui 里注册纹理（最近邻采样，像素风）。
pub fn ensure_sheet_texture(
    ctx: &egui::Context,
    sheets: &Assets<Image>,
    handle: &Handle<Image>,
    store: &mut SheetTexture,
) {
    if store.0.is_some() {
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
    store.0 = Some(ctx.load_texture("player_sheet", color, egui::TextureOptions::NEAREST));
}

/// 编辑器里的动画工作副本。
#[derive(Resource, Debug, Clone)]
pub struct AnimEdit {
    pub doc: PlayerAnimSet,
    pub path: PathBuf,
    pub dirty: bool,
    pub status: String,
    /// 预览：当前选中的状态下标与累计时间。
    pub preview_state: usize,
    pub preview_elapsed: f32,
}

impl AnimEdit {
    pub fn from_active(active: &PlayerAnimSet, asset_root: &str) -> Self {
        Self {
            doc: active.clone(),
            path: std::path::Path::new(asset_root).join(ANIM_REL_PATH),
            dirty: false,
            status: String::new(),
            preview_state: 0,
            preview_elapsed: 0.0,
        }
    }

    /// 序列化（唯一入口；4 空格缩进 / LF）。
    pub fn to_text(&self) -> String {
        let config = ron::ser::PrettyConfig::new()
            .indentor("    ")
            .new_line("\n");
        let mut text = ron::ser::to_string_pretty(&self.doc, config).unwrap_or_default();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }

    /// 追加一帧（图集索引）到某状态。
    pub fn push_frame(&mut self, state: usize, frame: usize) {
        if let Some(state) = self.doc.states.get_mut(state) {
            state.frames.push(frame);
            self.dirty = true;
        }
    }

    /// 删除某状态里的第 `slot` 帧。
    pub fn remove_frame(&mut self, state: usize, slot: usize) -> bool {
        let Some(state) = self.doc.states.get_mut(state) else {
            return false;
        };
        if slot >= state.frames.len() {
            return false;
        }
        state.frames.remove(slot);
        self.dirty = true;
        true
    }

    pub fn set_frames(&mut self, state: usize, frames: Vec<usize>) {
        if let Some(state) = self.doc.states.get_mut(state) {
            state.frames = frames;
            self.dirty = true;
        }
    }

    /// 保存：校验 → `.tmp` → rename。校验失败**拒绝写盘**（与关卡同一纪律）。
    pub fn save(&mut self) -> Result<(), String> {
        let issues = self.doc.validate();
        if !issues.is_empty() {
            self.status = format!("拒绝保存：{}", issues.join("；"));
            return Err(self.status.clone());
        }
        let text = self.to_text();
        let tmp = self.path.with_extension("ron.tmp");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
        }
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("写临时文件失败：{e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("替换失败：{e}"))?;
        self.dirty = false;
        self.status = format!("已保存：{}", self.path.display());
        Ok(())
    }
}

/// 直接从磁盘读取动画数据；缺失/解析失败/校验失败都回退内置默认。
///
/// 为什么编辑器要自己读盘：`init_anim_edit` 在 `Update` 首次运行时，`ActiveAnim`
/// 可能仍是插件初始化时的内置默认值（资产加载是异步的）。若此时复制 `active.set`，
/// 编辑器会永久停留在默认 `cat_sheet.png`，即使游戏随后已加载自定义动画。
fn load_anim_set(asset_root: &str) -> PlayerAnimSet {
    let path = std::path::Path::new(asset_root).join(ANIM_REL_PATH);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return PlayerAnimSet::builtin();
    };
    match ron::from_str::<PlayerAnimSet>(&text) {
        Ok(set) if set.validate().is_empty() => set,
        _ => PlayerAnimSet::builtin(),
    }
}

/// 资产加载完成后同步工作副本；用户有未保存改动时**不得**覆盖。
fn adopt_active_if_clean(edit: &mut AnimEdit, active: &game::anim_data::ActiveAnim) -> bool {
    if edit.dirty || active.source.is_none() || edit.doc == active.set {
        return false;
    }
    edit.doc = active.set.clone();
    edit.preview_state = edit
        .preview_state
        .min(edit.doc.states.len().saturating_sub(1));
    edit.preview_elapsed = 0.0;
    true
}

/// 首次拿到生效动画数据时建立工作副本。
///
/// 首次运行时若资产尚未加载（`active.source == None`），**直接读盘**而不是复制内置默认值；
/// 之后资产加载完成且用户未编辑时，自动同步到实际生效的数据。
pub fn init_anim_edit(
    mut commands: Commands,
    active: Option<Res<game::anim_data::ActiveAnim>>,
    root: Option<Res<game::assets::AssetRoot>>,
    mut existing: Option<ResMut<AnimEdit>>,
) {
    let (Some(active), Some(root)) = (active, root) else {
        return;
    };
    if let Some(edit) = existing.as_mut() {
        if adopt_active_if_clean(edit, &active) {
            edit.status = "已同步动画数据".to_string();
        }
        return;
    }
    let set = if active.source.is_some() {
        active.set.clone()
    } else {
        load_anim_set(&root.0)
    };
    commands.insert_resource(AnimEdit::from_active(&set, &root.0));
}

/// 面板：状态列表 + 帧序列 + 帧网格（`columns × 4`）+ 时长/循环 + 保存。
pub fn anim_panel(
    ui: &mut egui::Ui,
    edit: &mut AnimEdit,
    texture: Option<&egui::TextureHandle>,
    picker: &mut crate::tile_tool::AssetPickerState,
) {
    let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);

    // 顶行：标题 + 脏标记 + 保存/重载；不再把按钮堆在面板最底部。
    ui.horizontal(|ui| {
        ui.heading("角色动画");
        if edit.dirty {
            ui.colored_label(egui::Color32::YELLOW, "未保存 *");
        }
        ui.separator();
        if ui.button("保存动画").clicked() {
            let result = edit.save();
            if let Err(error) = result {
                error!("{error}");
            }
        }
        if ui.button("从磁盘重载").clicked()
            && let Ok(text) = std::fs::read_to_string(&edit.path)
            && let Ok(set) = ron::from_str::<PlayerAnimSet>(&text)
        {
            edit.doc = set;
            edit.dirty = false;
            edit.status = "已从磁盘重载".to_string();
        }
        if !edit.status.is_empty() {
            ui.separator();
            ui.label(&edit.status);
        }
    });
    ui.separator();

    let mut pending: Option<(usize, usize, bool)> = None;

    // 横向分区：左 = 状态设置 + 预览，右 = 从贴图选帧 + 状态/帧列表。
    ui.columns(2, |panels| {
        // ---------------- 左栏：当前状态设置 + 预览 ----------------
        let ui = &mut panels[0];
        // 每个状态独立贴图：编辑**当前选中状态**的路径与列数
        let selected = edit
            .preview_state
            .min(edit.doc.states.len().saturating_sub(1));
        if let Some(state) = edit.doc.states.get_mut(selected) {
            ui.label(format!("状态 `{}` 的图集路径（相对 assets/）", state.name));
            let width = ui.available_width();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut state.sheet)
                        .desired_width(width)
                        .hint_text("sprites/player/cat_sheet.png"),
                )
                .changed()
            {
                edit.dirty = true;
            }
            if let Some(picked) = crate::tile_tool::asset_picker(ui, picker, "anim") {
                state.sheet = picked;
                edit.dirty = true;
            }
            // 列数由该状态贴图宽度决定（每格 32×32）
            if let Some(texture) = texture {
                let columns = (texture.size_vec2().x / 32.0).round().max(1.0) as u32;
                if columns != state.columns {
                    state.columns = columns;
                    edit.dirty = true;
                }
                let size = texture.size_vec2();
                ui.label(format!(
                    "列数：{}（贴图 {:.0}×{:.0}，每格 32×32）",
                    state.columns, size.x, size.y
                ));
            } else {
                ui.label("贴图尚未加载：检查该状态的图集路径");
            }
        }

        // ---- 预览：按当前参数循环播放（`08` §8.5.7 A）----
        let columns = edit
            .doc
            .states
            .get(edit.preview_state)
            .map(|state| state.columns.max(1) as usize)
            .unwrap_or(1);
        if let Some(state) = edit.doc.states.get(edit.preview_state).cloned()
            && let Some(texture) = texture
            && !state.frames.is_empty()
        {
            edit.preview_elapsed += dt;
            let per_frame = state.frame_secs.max(0.01);
            let cursor = (edit.preview_elapsed / per_frame) as usize;
            let slot = if state.looping {
                cursor % state.frames.len()
            } else {
                cursor.min(state.frames.len() - 1)
            };
            let frame = state.frames[slot];
            let (col, row) = (frame % columns, frame / columns);
            let cell = 32.0;
            let sheet = texture.size_vec2();
            let uv = egui::Rect::from_min_size(
                egui::pos2((col as f32 * cell) / sheet.x, (row as f32 * cell) / sheet.y),
                egui::vec2(cell / sheet.x, cell / sheet.y),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(texture.id(), sheet))
                        .uv(uv)
                        .fit_to_exact_size(egui::vec2(96.0, 96.0))
                        // 这里显示的是整张图集里裁出的一个 32×32 格；若保留纹理宽高比，
                        // 18 列图集会按 576:32 压扁成一条 → 预览被压扁。
                        .maintain_aspect_ratio(false),
                );
                ui.label(format!(
                    "预览 {}：第 {frame} 帧（{slot} / {}）  {:.2} 秒/帧",
                    state.name,
                    state.frames.len(),
                    state.frame_secs
                ));
            });
        }
        ui.separator();
        ui.label("预览状态");
        for (index, candidate) in edit.doc.states.iter().enumerate() {
            if ui
                .selectable_label(edit.preview_state == index, &candidate.name)
                .clicked()
            {
                edit.preview_state = index;
                edit.preview_elapsed = 0.0;
            }
        }

        // ---------------- 右栏：从贴图选帧 + 状态/帧列表 ----------------
        let ui = &mut panels[1];

        // ---- 从贴图中框选帧（32×32 吸附）：点击/拖动追加到当前选中的状态 ----
        if let Some(texture) = texture {
            let sheet = texture.size_vec2();
            let cell = 32.0;
            let cols = (sheet.x / cell).round().max(1.0) as usize;
            // 按当前列宽缩放整张图集，避免在横向分栏里溢出
            let available = (ui.available_width() - 8.0).max(32.0);
            let scale = (available / sheet.x).clamp(0.25, 4.0);
            ui.label(format!(
                "从贴图选帧（点击或拖动，32×32）→ 追加到状态 `{}`",
                edit.doc
                    .states
                    .get(edit.preview_state)
                    .map(|s| s.name.as_str())
                    .unwrap_or("?")
            ));
            let response = ui.add(
                egui::Image::new(egui::load::SizedTexture::new(texture.id(), sheet))
                    .fit_to_exact_size(egui::vec2(sheet.x * scale, sheet.y * scale))
                    .sense(egui::Sense::click_and_drag()),
            );
            if let Some(pos) = response.interact_pointer_pos()
                && (response.clicked() || response.dragged())
            {
                let local = ((pos - response.rect.min) / scale).max(egui::Vec2::ZERO);
                let col = ((local.x / cell).floor() as isize).max(0) as usize;
                let row = ((local.y / cell).floor() as isize).max(0) as usize;
                let index = (row * cols + col).min(255) as u32;
                pending = Some((edit.preview_state, index as usize, true));
                ui.label(format!(
                    "指针：像素 ({:.0}, {:.0}) → 帧 {index}（列 {col} / 行 {row}）",
                    local.x, local.y
                ));
            }
        }
        ui.separator();

        ui.heading("状态与帧");
        for (state_index, state) in edit.doc.states.iter_mut().enumerate() {
            let mut clear = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(state.name.clone());
                ui.add(
                    egui::DragValue::new(&mut state.frame_secs)
                        .speed(0.01)
                        .range(0.02..=2.0),
                );
                ui.checkbox(&mut state.looping, "循环");
                if ui.small_button("清空帧").clicked() {
                    clear = true;
                }
            });
            if clear {
                state.frames.clear();
                edit.dirty = true;
            }
            // 当前帧序列：点一帧删除
            ui.horizontal_wrapped(|ui| {
                for (slot, frame) in state.frames.clone().into_iter().enumerate() {
                    if ui.small_button(format!("{frame}")).clicked() {
                        pending = Some((state_index, slot, false));
                    }
                }
            });
            ui.separator();
        }
    });

    match pending {
        Some((state, slot, true)) => edit.push_frame(state, slot),
        Some((state, slot, false)) => {
            edit.remove_frame(state, slot);
        }
        None => {}
    }
}

/// 便于测试：把 `StateAnim` 的帧序列压成一行文本（UI/日志用）。
pub fn frames_summary(state: &StateAnim) -> String {
    state
        .frames
        .iter()
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit() -> AnimEdit {
        AnimEdit::from_active(&PlayerAnimSet::builtin(), "assets")
    }

    #[test]
    fn save_round_trips_and_is_readable_by_the_game_loader() {
        let mut edit = edit();
        let dir = std::env::temp_dir().join("animj_anim_save");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        edit.path = dir.join("player.anim.ron");
        edit.push_frame(0, 2);
        edit.save().expect("应保存成功");
        let text = std::fs::read_to_string(&edit.path).unwrap();
        let parsed: PlayerAnimSet = ron::from_str(&text).expect("游戏加载器必须能解析");
        assert_eq!(parsed, edit.doc);
        assert!(text.ends_with('\n'));
        assert!(!edit.dirty);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_data_is_rejected_and_not_written() {
        let mut edit = edit();
        let dir = std::env::temp_dir().join("animj_anim_reject");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        edit.path = dir.join("player.anim.ron");
        edit.set_frames(0, Vec::new()); // 空帧序列 ⇒ 校验失败
        let error = edit.save().expect_err("空帧必须被拒绝");
        assert!(error.contains("帧序列为空"), "{error}");
        assert!(!edit.path.exists(), "校验失败时不得写盘");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frame_edits_are_tracked() {
        let mut edit = edit();
        let before = edit.doc.states[1].frames.clone();
        edit.push_frame(1, 99);
        assert!(edit.dirty);
        assert_eq!(edit.doc.states[1].frames.len(), before.len() + 1);
        assert!(edit.remove_frame(1, before.len()));
        assert_eq!(edit.doc.states[1].frames, before);
        assert!(!edit.remove_frame(1, 999));
    }

    /// 回归：资产还在异步加载时，编辑器必须读盘而不是快照内置默认值。
    #[test]
    fn initial_load_reads_the_configured_sheet() {
        let dir = std::env::temp_dir().join("animj_anim_init");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("anim")).unwrap();
        let mut set = PlayerAnimSet::builtin();
        set.states[0].sheet = "sprites/player/w/l.png".to_string();
        let text = AnimEdit::from_active(&set, dir.to_str().unwrap()).to_text();
        std::fs::write(dir.join(ANIM_REL_PATH), text).unwrap();

        let loaded = load_anim_set(dir.to_str().unwrap());
        assert_eq!(loaded.states[0].sheet, "sprites/player/w/l.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 缺文件 / 非法数据 ⇒ 回退内置默认（与资产加载器一致）。
    #[test]
    fn missing_or_invalid_animation_file_falls_back_to_builtin() {
        let dir = std::env::temp_dir().join("animj_anim_init_bad");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("anim")).unwrap();
        std::fs::write(dir.join(ANIM_REL_PATH), "(not ron").unwrap();
        assert_eq!(
            load_anim_set(dir.to_str().unwrap()),
            PlayerAnimSet::builtin()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 资产加载完成后可同步工作副本；用户有未保存改动时不得覆盖。
    #[test]
    fn active_animation_sync_respects_dirty_edits() {
        let mut active = game::anim_data::ActiveAnim {
            set: PlayerAnimSet::builtin(),
            source: Some(Handle::default()),
            revision: 1,
        };
        let mut edit = AnimEdit::from_active(&PlayerAnimSet::builtin(), "assets");
        active.set.states[0].sheet = "sprites/player/w/l.png".to_string();
        assert!(adopt_active_if_clean(&mut edit, &active));
        assert_eq!(edit.doc.states[0].sheet, "sprites/player/w/l.png");

        active.set.states[0].sheet = "sprites/player/other.png".to_string();
        edit.dirty = true;
        assert!(!adopt_active_if_clean(&mut edit, &active));
        assert_eq!(edit.doc.states[0].sheet, "sprites/player/w/l.png");
    }

    /// 回归：从非方形图集裁出的单格必须在预览里保持方形。
    /// 若保留 `maintain_aspect_ratio`，576×32 的图集会按整图宽高比把 96×96 预览压成一条。
    #[test]
    fn cropped_preview_cell_stays_square() {
        let mut size = egui::Vec2::ZERO;
        egui::__run_test_ui(|ui| {
            let response = ui.add(
                egui::Image::new(egui::load::SizedTexture::new(
                    egui::TextureId::Managed(0),
                    egui::vec2(576.0, 32.0),
                ))
                .uv(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(32.0 / 576.0, 1.0),
                ))
                .fit_to_exact_size(egui::vec2(96.0, 96.0))
                .maintain_aspect_ratio(false),
            );
            size = response.rect.size();
        });
        assert!(
            (size.x - 96.0).abs() < 0.5 && (size.y - 96.0).abs() < 0.5,
            "预览格被压扁：{size:?}"
        );
    }

    /// 回归：首次 `Update` 时资产可能尚未加载（`source == None`），不能把内置默认值写进工作副本。
    #[test]
    fn init_does_not_snapshot_builtin_before_asset_load() {
        use bevy::ecs::system::RunSystemOnce;
        use game::anim_data::ActiveAnim;

        let dir = std::env::temp_dir().join("animj_anim_init_system");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("anim")).unwrap();
        let mut set = PlayerAnimSet::builtin();
        set.states[0].sheet = "sprites/player/w/l.png".to_string();
        let text = AnimEdit::from_active(&set, dir.to_str().unwrap()).to_text();
        std::fs::write(dir.join(ANIM_REL_PATH), text).unwrap();

        let mut app = App::new();
        app.insert_resource(ActiveAnim::default()) // source = None：模拟资产尚未加载
            .insert_resource(game::assets::AssetRoot(dir.to_string_lossy().into_owned()));
        app.world_mut()
            .run_system_once(init_anim_edit)
            .expect("系统应成功运行");

        let edit = app.world().resource::<AnimEdit>();
        assert_eq!(
            edit.doc.states[0].sheet, "sprites/player/w/l.png",
            "必须读盘，而不是快照内置 cat_sheet"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
