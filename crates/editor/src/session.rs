//! 编辑会话：`doc` 是唯一真相 + 全量快照撤销 + 原子保存 + 校验结果。
//! 所有者文档：`docs/08-level-editor.md` §8.5.2、§8.7.1、§8.8。
//!
//! **纪律**：任何对 `doc` 的修改都必须先调 [`EditorSession::begin_edit`]（push 快照、标脏），
//! 不允许在 UI/工具代码里直接改字段；撤销/重做只搬快照，不重放操作。

use std::path::PathBuf;

use level_format::{Level, LevelIssue, ROOM_TILES, Room, Severity, TileKind, room_index};
use bevy::prelude::*;

/// 撤销栈上限（`08` §8.5.2）。
pub const UNDO_LIMIT: usize = 128;
/// 拖拽期间两次全校验的最小间隔（秒，`08` §8.8.1）。
pub const VALIDATE_MIN_INTERVAL: f32 = 0.1;

/// 编辑工具（`08` §8.5.1 的 1–6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    TileBrush,
    Eraser,
    RectFill,
    Picker,
    PlaceEntity,
    MoveEntity,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::TileBrush,
        Tool::Eraser,
        Tool::RectFill,
        Tool::Picker,
        Tool::PlaceEntity,
        Tool::MoveEntity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::TileBrush => "1 瓦片笔刷",
            Tool::Eraser => "2 橡皮",
            Tool::RectFill => "3 矩形填充",
            Tool::Picker => "4 吸管",
            Tool::PlaceEntity => "5 放实体",
            Tool::MoveEntity => "6 移动实体",
        }
    }
}

/// 编辑会话（唯一真相）。
#[derive(Resource, Debug, Clone)]
pub struct EditorSession {
    pub path: Option<PathBuf>,
    pub doc: Level,
    pub undo: Vec<Level>,
    pub redo: Vec<Level>,
    pub dirty: bool,
    /// 最后一次加载/写盘的 RON 文本（热重载回环抑制与"是否真的改过"的依据）。
    pub saved_text: Option<String>,
    pub file_issues: Vec<LevelIssue>,
    pub current_room: (u32, u32),
    pub tool: Tool,
    /// 当前笔刷字符（来自关卡调色板；`.` 等价擦除）。
    pub brush_char: char,
    /// 当前放置的实体类型（`PlaceEntity` 工具）。
    pub entity_kind: String,
    /// 面板里选中的房间实体下标。
    pub selected_entity: Option<usize>,
    /// 状态栏文字（保存结果、拦截原因等）。
    pub status: String,
    /// `doc` 的修订号：只有它变化才需要重建投影。
    pub doc_revision: u64,
    /// 另存为/保存路径输入框（`08` §8.7.1）。
    pub save_path_input: String,
    /// 请求相机重新对焦到当前房间（错误列表点击定位用）。
    pub focus_camera: bool,
    /// 更精确的对焦点（世界像素，例如错误所在格）；优先于房间中心。
    pub focus_at: Option<Vec2>,
    /// 等待用户确认的外部关卡改动（`08` §8.7.4）。
    pub pending_external: Option<Level>,
    /// 已被用户忽略的外部文本（避免同一内容反复弹窗）。
    pub ignored_text: Option<String>,
    last_validate: f32,
}

impl EditorSession {
    pub fn new(level: Level, path: Option<PathBuf>) -> Self {
        let saved_text = Some(level_format::write(&level));
        let mut session = Self {
            path,
            doc: level,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            saved_text,
            file_issues: Vec::new(),
            current_room: (0, 0),
            tool: Tool::default(),
            brush_char: '#',
            entity_kind: "fruit".to_string(),
            selected_entity: None,
            status: String::new(),
            doc_revision: 0,
            save_path_input: String::new(),
            focus_camera: false,
            focus_at: None,
            pending_external: None,
            ignored_text: None,
            last_validate: -1.0,
        };
        session.save_path_input = session
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        session.file_issues = level_format::validate(&session.doc);
        session
    }

    // ---------- 索引与房间 ----------

    /// **唯一**的瓦片索引入口（`08` 检查清单 #4）。
    pub fn tile_index(file_row: usize, tx: usize) -> usize {
        file_row * ROOM_TILES + tx
    }

    pub fn room_index(&self) -> usize {
        room_index(self.current_room.0, self.current_room.1, self.doc.grid.cols)
    }

    pub fn room(&self) -> &Room {
        &self.doc.rooms[self.room_index()]
    }

    pub fn room_mut(&mut self) -> &mut Room {
        let index = self.room_index();
        &mut self.doc.rooms[index]
    }

    /// 房间局部（`ty` 自底向上）的瓦片字符。
    pub fn tile_char(&self, tx: usize, ty: usize) -> Option<char> {
        let file_row = ROOM_TILES - 1 - ty;
        self.room()
            .tiles
            .get(file_row)?
            .chars()
            .nth(tx)
    }

    // ---------- 事务 ----------

    /// 开始一次编辑：快照入栈、清空 redo、标脏、推进修订号。
    pub fn begin_edit(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    /// 取消**最近一次**编辑（拖拽被取消时用）：恢复快照、丢弃 redo、重算脏标记。
    ///
    /// 与 `undo` 的区别：`undo` 会给用户一次"重做"的机会，取消拖拽不应该留下这种痕迹。
    pub fn cancel_last_edit(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.doc = previous;
        self.redo.clear();
        self.dirty = self
            .saved_text
            .as_ref()
            .map(|text| level_format::write(&self.doc).replace("\r\n", "\n") != text.replace("\r\n", "\n"))
            .unwrap_or(true);
        self.doc_revision = self.doc_revision.wrapping_add(1);
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.doc, previous));
        self.dirty = true;
        self.doc_revision = self.doc_revision.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.doc, next));
        self.dirty = true;
        self.doc_revision = self.doc_revision.wrapping_add(1);
        true
    }

    // ---------- 瓦片写入（唯一入口） ----------

    pub fn set_tile(&mut self, tx: usize, ty: usize, kind: TileKind) {
        self.set_tile_char(tx, ty, kind.to_char());
    }

    /// 按**字符**写瓦片（v2 自定义地块用；`03` §3.9）。
    pub fn set_tile_char(&mut self, tx: usize, ty: usize, ch: char) {
        let file_row = ROOM_TILES - 1 - ty;
        let Some(row) = self.room_mut().tiles.get_mut(file_row) else {
            return;
        };
        if tx >= row.len() {
            return;
        }
        let mut chars: Vec<char> = row.chars().collect();
        if chars.get(tx) == Some(&ch) {
            return; // 幂等
        }
        chars[tx] = ch;
        *row = chars.into_iter().collect();
        // 拖拽期间每一格都必须推进修订号，否则投影只在按下的那一帧重建（`08` §8.5.3）。
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    // ---------- 调色板（v2 新增地块，`03` §3.9）----------

    pub fn add_palette_entry(&mut self, entry: level_format::PaletteEntry) -> bool {
        if entry.char == '.' || self.doc.entry_of(entry.char).is_some() {
            return false;
        }
        self.begin_edit();
        // **先物化内置调色板**：`palette` 为空时 `palette()` 才回退内置；
        // 直接 push 会把内置的 4 种瓦片整体顶掉（实机：新增一块后原有瓦片全部无法选中）。
        self.doc.palette = self.doc.palette();
        self.doc.palette.push(entry);
        true
    }

    pub fn remove_palette_entry(&mut self, index: usize) -> bool {
        let palette = self.doc.palette();
        let Some(entry) = palette.get(index).cloned() else {
            return false;
        };
        // 内置调色板（palette 为空）不可删；先物化再删
        self.begin_edit();
        self.doc.palette = palette;
        self.doc.palette.retain(|candidate| candidate.char != entry.char);
        true
    }

    pub fn set_palette_behavior(&mut self, index: usize, behavior: level_format::Behavior) {
        let mut palette = self.doc.palette();
        if let Some(entry) = palette.get_mut(index) {
            self.begin_edit();
            entry.behavior = behavior;
            self.doc.palette = palette;
        }
    }

    pub fn set_palette_cell(&mut self, index: usize, cell: u32) {
        let mut palette = self.doc.palette();
        if let Some(entry) = palette.get_mut(index) {
            self.begin_edit();
            entry.cell = cell;
            self.doc.palette = palette;
        }
    }

    // ---------- 房间属性 ----------

    pub fn set_reward(&mut self, reward: Option<String>) {
        self.room_mut().reward = reward;
    }

    pub fn set_goals(&mut self, goals: Vec<level_format::Goal>) {
        self.room_mut().goals = goals;
    }

    // ---------- 实体（工具 6） ----------

    /// 房间里每个实体的世界 AABB 中心与尺寸（占位尺寸规则与投影一致）。
    pub fn entity_boxes(&self) -> Vec<(usize, (u32, u32), Vec2, Vec2)> {
        let rows = self.doc.grid.rows;
        let cols = self.doc.grid.cols;
        let (col, row) = self.current_room;
        let mut out = Vec::new();
        let Some(room) = self.doc.rooms.get(self.room_index()) else {
            return out;
        };
        for (index, spec) in room.entities.iter().enumerate() {
            let size = if spec.kind == "fruit" {
                Vec2::splat(game::config::physics::PICKUP_SIZE)
            } else {
                spec.size
                    .map(|(w, h)| Vec2::new(w as f32, h as f32))
                    .unwrap_or(Vec2::splat(32.0))
            };
            let local = Vec2::new(spec.at.0 as f32, spec.at.1 as f32);
            let center = game::coords::room_local_to_world(col, row, rows, local);
            let _ = cols;
            out.push((index, (col, row), center, size));
        }
        out
    }

    /// 把实体的 `at` 设为世界点对应的房间局部坐标（工具 6 的落点）。
    pub fn set_entity_at_world(&mut self, index: usize, world: Vec2, snap_to_tile: bool) {
        let rows = self.doc.grid.rows;
        let cols = self.doc.grid.cols;
        let Some(pos) = game::coords::world_to_room(world, cols, rows) else {
            return;
        };
        if (pos.col, pos.row) != self.current_room {
            return; // 只在同一房间内可拖动（跨房间 ⇒ 调用方取消）
        }
        let local = if snap_to_tile {
            let tile = level_format::TILE_PX as f32;
            Vec2::new(
                (pos.local.x / tile).floor() * tile + tile / 2.0,
                (pos.local.y / tile).floor() * tile + tile / 2.0,
            )
        } else {
            Vec2::new(pos.local.x.round(), pos.local.y.round())
        };
        if let Some(spec) = self.room_mut().entities.get_mut(index) {
            let at = (local.x as i32, local.y as i32);
            if spec.at != at {
                spec.at = at;
                // 拖动时不经过 `begin_edit`，必须自己推进修订号才能实时重绘投影
                self.doc_revision = self.doc_revision.wrapping_add(1);
            }
        }
    }

    /// 修复「误放多个单例实体」：`player_spawn` 与 `exit` 都只保留第一个（`03` §3.7 V12/V8）。
    ///
    /// 返回被删除的数量；0 表示本来就没有重复。
    /// 删除任意房间里的任意实体（一个 undo 步骤）。返回是否真的删了。
    pub fn delete_entity(&mut self, room_index: usize, entity_index: usize) -> bool {
        let exists = self
            .doc
            .rooms
            .get(room_index)
            .is_some_and(|room| entity_index < room.entities.len());
        if !exists {
            return false;
        }
        self.begin_edit();
        let kind = self.doc.rooms[room_index].entities.remove(entity_index).kind;
        self.selected_entity = None;
        info!("编辑器：删除实体 rooms[{room_index}].entities[{entity_index}] ({kind})");
        true
    }

    /// 对焦到世界坐标（错误定位到格）。
    pub fn focus_world(&mut self, world: Vec2) {
        self.focus_at = Some(world);
        self.focus_camera = true;
    }

    /// 把当前房间切到 `(col,row)` 并请求相机对焦（UI 房间选择器用）。
    /// 切换当前房间（右栏房间选择器）：**只改编辑目标，不移动相机**——
    /// 相机移动会打断"看全图、连续放置"的操作流。
    pub fn goto_room(&mut self, col: u32, row: u32) {
        if col < self.doc.grid.cols && row < self.doc.grid.rows {
            self.current_room = (col, row);
            // 实体下标是房间局部的：切房间后旧下标会指向别的实体
            self.selected_entity = None;
            info!("编辑器：切换当前房间 → ({col}, {row})");
        }
    }

    pub fn dedupe_singletons(&mut self) -> usize {
        let cols = self.doc.grid.cols;
        let start = (self.doc.start.col, self.doc.start.row);
        // 第一遍：**只计算**要删的下标（不碰 doc，保证 undo 快照是"改之前"）
        let mut spawn_kept = false;
        let mut plan: Vec<(usize, Vec<usize>)> = Vec::new();
        for (index, room) in self.doc.rooms.iter().enumerate() {
            let (col, row) = (index as u32 % cols, index as u32 / cols);
            let mut exit_kept = false;
            let mut doomed = Vec::new();
            for (entity_index, spec) in room.entities.iter().enumerate() {
                let keep = match spec.kind.as_str() {
                    // V12：全关唯一且在起始房间
                    "player_spawn" => {
                        let keep = (col, row) == start && !spawn_kept;
                        if keep {
                            spawn_kept = true;
                        }
                        keep
                    }
                    // V8：每房唯一
                    "exit" => {
                        let keep = !exit_kept;
                        exit_kept = true;
                        keep
                    }
                    _ => true,
                };
                if !keep {
                    doomed.push(entity_index);
                }
            }
            if !doomed.is_empty() {
                plan.push((index, doomed));
            }
        }
        let removed: usize = plan.iter().map(|(_, doomed)| doomed.len()).sum();
        if removed == 0 {
            return 0;
        }
        // 第二遍：先开事务（快照 = 改之前），再按下标从后往前删
        self.begin_edit();
        for (room_index, doomed) in plan {
            let entities = &mut self.doc.rooms[room_index].entities;
            for index in doomed.into_iter().rev() {
                entities.remove(index);
            }
        }
        self.selected_entity = None;
        self.status = format!("全关卡清理：删除 {removed} 个重复的单例实体");
        info!("编辑器：dedupe 删除 {removed} 个单例实体");
        removed
    }



    /// 接受外部改动（`08` §8.7.4）：整份替换，清空 undo/redo，记账。
    pub fn accept_external(&mut self) {
        let Some(level) = self.pending_external.take() else {
            return;
        };
        self.undo.clear();
        self.redo.clear();
        self.doc = level;
        self.saved_text = Some(level_format::write(&self.doc));
        self.dirty = false;
        self.doc_revision = self.doc_revision.wrapping_add(1);
        self.file_issues = level_format::validate(&self.doc);
        self.status = "已接受外部改动".to_string();
    }

    /// 忽略外部改动：记住该文本，避免同一内容反复提示。
    pub fn ignore_external(&mut self) {
        if let Some(level) = self.pending_external.take() {
            self.ignored_text = Some(level_format::write(&level));
            self.status = "已忽略外部改动".to_string();
        }
    }

    // ---------- 校验 ----------

    pub fn validate_now(&mut self, now: f32) {
        self.last_validate = now;
        self.file_issues = level_format::validate(&self.doc);
    }

    pub fn error_count(&self) -> usize {
        self.file_issues
            .iter()
            .filter(|issue| issue.severity != Severity::Warning)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.file_issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
            .count()
    }

    pub fn has_fatal(&self) -> bool {
        self.file_issues
            .iter()
            .any(|issue| issue.severity == Severity::Fatal)
    }

    /// 试玩闸门：存在 error/fatal 时拒绝（`08` §8.6.1 第 2 步）。
    pub fn playable(&self) -> bool {
        self.error_count() == 0
    }

    // ---------- 保存 ----------

    /// 序列化（唯一路径）+ 校验闸门 + `.ron.tmp` 原子写（`08` §8.7.1）。
    pub fn save(&mut self) -> Result<(), String> {
        // 另存为：输入框路径优先（`08` §8.7.1 的 Ctrl+Shift+S）
        if !self.save_path_input.trim().is_empty() {
            self.path = Some(std::path::PathBuf::from(self.save_path_input.trim()));
        }
        let Some(path) = self.path.clone() else {
            return Err("当前关卡没有路径".to_string());
        };
        let issues = level_format::validate(&self.doc);
        self.file_issues = issues.clone();
        if issues
            .iter()
            .any(|issue| issue.severity == Severity::Fatal)
        {
            return Err("存在 Fatal 级问题，拒绝写盘（避免覆盖损坏数据）".to_string());
        }
        let mut text = level_format::write(&self.doc).replace("\r\n", "\n");
        if !text.ends_with('\n') {
            text.push('\n');
        }
        let tmp = path.with_extension("ron.tmp");
        std::fs::write(&tmp, text.as_bytes()).map_err(|err| format!("写临时文件失败：{err}"))?;
        std::fs::rename(&tmp, &path).map_err(|err| format!("替换目标文件失败：{err}"))?;
        self.saved_text = Some(text);
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use level_format::{Goal, Grid, RoomCoord};

    fn sample_level() -> Level {
        Level {
            version: level_format::VERSION,
            name: "t".into(),
            grid: Grid { cols: 1, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![Room {
                name: None,
                reward: None,
                spawn: None,
                goals: vec![Goal::ReachExit],
                tiles: vec![".".repeat(ROOM_TILES); ROOM_TILES],
                entities: vec![],
            }],
        }
    }

    #[test]
    fn adding_a_tile_keeps_builtin_entries() {
        let mut session = EditorSession::new(sample_level(), None);
        assert!(session.doc.palette.is_empty(), "初始为空 ⇒ 回退内置");
        assert!(session.add_palette_entry(level_format::PaletteEntry {
            char: 'S',
            name: "stone".into(),
            behavior: level_format::Behavior::Solid,
            cell: 4,
        }));
        // 内置 4 种必须仍在调色板里（这也是左栏能继续选中它们的前提）
        let palette = session.doc.palette();
        assert_eq!(palette.len(), 5, "内置 4 种 + 新增 1 种");
        for ch in ['#', '=', '^', '*'] {
            assert!(session.doc.entry_of(ch).is_some(), "内置瓦片 {ch} 不应消失");
        }
        assert!(session.doc.entry_of('S').is_some());
        // 一步撤销即可回到"空 = 内置"状态
        assert!(session.undo());
        assert!(session.doc.palette.is_empty());
    }

    #[test]
    fn tile_index_is_row_major_in_file_order() {
        assert_eq!(EditorSession::tile_index(0, 0), 0);
        assert_eq!(EditorSession::tile_index(0, 15), 15);
        assert_eq!(EditorSession::tile_index(1, 0), ROOM_TILES);
    }

    /// 画瓦片 → 撤销 → 重做：撤销栈是**全量快照**，回到编辑前逐字节相同。
    #[test]
    fn undo_redo_restore_exact_level() {
        let mut session = EditorSession::new(sample_level(), None);
        let before = session.doc.clone();
        session.begin_edit();
        session.set_tile(3, 0, TileKind::Solid); // ty = 0 = 房间底行
        assert_ne!(session.doc, before);
        assert!(session.dirty);
        assert!(session.undo());
        assert_eq!(session.doc, before);
        assert!(session.redo());
        assert!(session.tile_char(3, 0) == Some('#'));
    }

    /// 写入是幂等的：同一格写同一值不产生新修订。
    #[test]
    fn writing_same_tile_is_idempotent() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        let revision = session.doc_revision;
        session.set_tile(1, 1, TileKind::Air);
        assert_eq!(session.doc_revision, revision);
    }

    /// 瓦片坐标：`ty = 0` 是房间底行（文件第 `ROOM_TILES - 1` 行）。
    #[test]
    fn bottom_tile_row_maps_to_last_file_row() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        session.set_tile(0, 0, TileKind::Spike);
        let file_row = session.room().tiles[ROOM_TILES - 1].chars().next();
        assert_eq!(file_row, Some('^'));
        // 顶行是文件第 0 行
        session.set_tile(0, ROOM_TILES - 1, TileKind::Decor);
        assert_eq!(session.room().tiles[0].chars().next(), Some('*'));
    }

    /// 实体按房间管理：列表/命中只覆盖当前房间，切房间后清空选中下标。
    #[test]
    fn entities_are_scoped_to_the_current_room() {
        let mut level = sample_level();
        level.grid = Grid { cols: 2, rows: 1 };
        level.rooms[0].entities.push(level_format::EntitySpec {
            kind: "fruit".into(),
            at: (100, 100),
            size: None,
            id: None,
        });
        let mut second = level.rooms[0].clone();
        second.entities[0].at = (200, 200);
        level.rooms.push(second);
        let mut session = EditorSession::new(level, None);

        assert_eq!(session.entity_boxes().len(), 1);
        let first_room_center = session.entity_boxes()[0].2;
        session.selected_entity = Some(0);
        session.goto_room(1, 0);
        assert_eq!(session.selected_entity, None, "切房间必须清空局部下标");
        assert_eq!(session.entity_boxes().len(), 1);
        assert_ne!(session.entity_boxes()[0].2, first_room_center);
    }

    /// 长按拖动时每写入一格都必须推进修订号；否则投影只在按下帧重建（实机 bug）。
    #[test]
    fn dragging_tiles_bumps_revision_per_cell() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        let mut revision = session.doc_revision;
        for ty in 0..3 {
            session.set_tile(1, ty, TileKind::Solid);
            assert!(
                session.doc_revision > revision,
                "第 {ty} 格写入后修订号必须增长"
            );
            revision = session.doc_revision;
        }
    }

    #[test]
    fn dragging_an_entity_bumps_revision_when_it_moves() {
        let mut level = sample_level();
        level.rooms[0].entities.push(level_format::EntitySpec {
            kind: "fruit".into(),
            at: (100, 100),
            size: None,
            id: None,
        });
        let mut session = EditorSession::new(level, None);
        session.begin_edit();
        let center = session.entity_boxes()[0].2;
        let revision = session.doc_revision;
        session.set_entity_at_world(0, center + Vec2::new(20.0, 0.0), true);
        assert!(session.doc_revision > revision, "实体位置变化必须重建投影");
    }

    #[test]
    fn goals_and_reward_edits_go_through_snapshot() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        session.set_goals(vec![Goal::ReachExit, Goal::Collect { count: 3 }]);
        session.set_reward(Some("item_01".into()));
        assert_eq!(session.room().goals.len(), 2);
        assert_eq!(session.room().reward.as_deref(), Some("item_01"));
        assert!(session.undo());
        assert_eq!(session.room().goals, vec![Goal::ReachExit]);
        assert_eq!(session.room().reward, None);
    }

    /// 工具 6：实体的世界 AABB 命中测试与吸附位置。
    #[test]
    fn entity_hit_test_and_snapped_move() {
        let mut level = sample_level();
        level.rooms[0].entities.push(level_format::EntitySpec {
            kind: "fruit".into(),
            at: (100, 100),
            size: None,
            id: None,
        });
        let mut session = EditorSession::new(level, None);
        let boxes = session.entity_boxes();
        assert_eq!(boxes.len(), 1);
        let (index, _room, center, size) = boxes[0];
        assert_eq!(size, Vec2::splat(game::config::physics::PICKUP_SIZE));
        // 点中实体
        assert_eq!(crate::tools::entity_at(&session, center), Some(index));
        // 点不中
        assert_eq!(crate::tools::entity_at(&session, center + Vec2::new(200.0, 0.0)), None);
        // 拖动吸附到瓦片中心（房间局部）
        session.begin_edit();
        session.set_entity_at_world(index, center + Vec2::new(20.0, 0.0), true);
        let at = session.room().entities[index].at;
        // 瓦片中心 ⇒ 局部坐标模 32 应为 16
        assert_eq!(((at.0 % 32) + 32) % 32, 16);
    }

    /// 外部改动必须**显式确认**：接受才替换 `doc` 并清空 undo；忽略则记住文本避免反复提示。
    #[test]
    fn external_changes_require_explicit_apply_or_ignore() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        session.set_tile(0, 0, level_format::TileKind::Solid);
        let edited = session.doc.clone();

        let mut external = sample_level();
        external.rooms[0].tiles[0] = "################".to_string();
        session.pending_external = Some(external.clone());
        // 未确认前 doc 不变
        assert_eq!(session.doc, edited);
        session.accept_external();
        assert_eq!(session.doc, external);
        assert!(session.undo.is_empty());
        assert!(!session.dirty);
        assert!(session.pending_external.is_none());

        // 忽略：保留编辑内容，并记住该文本
        session.pending_external = Some(external.clone());
        let edited = session.doc.clone();
        session.ignore_external();
        assert_eq!(session.doc, edited);
        assert_eq!(
            session.ignored_text.as_deref(),
            Some(level_format::write(&external).as_str())
        );
        assert!(session.pending_external.is_none());
    }

    /// 另存为：输入框里的路径优先于原路径。
    #[test]
    fn save_as_uses_the_path_field() {
        let mut session = EditorSession::new(sample_level(), None);
        let dir = std::env::temp_dir().join("animj_editor_saveas");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("copy.level.ron");
        session.save_path_input = target.display().to_string();
        session.save().expect("另存为应成功");
        assert!(target.exists());
        assert_eq!(session.path.as_deref(), Some(target.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 误放多个 `player_spawn` / `exit` 时：`player_spawn` **全关唯一且在起始房间**，
    /// `exit` **每房唯一**（V12 / V8）。去重后一个 undo 步骤可整步回退。
    #[test]
    fn dedupe_keeps_one_player_spawn_level_wide_and_one_exit_per_room() {
        let mut level = sample_level();
        level.grid = level_format::Grid { cols: 2, rows: 1 };
        level.rooms.push(level.rooms[0].clone());
        let spec = |kind: &str, at: (i32, i32)| level_format::EntitySpec {
            kind: kind.to_string(),
            at,
            size: None,
            id: None,
        };
        // 起始房间：2 个 spawn、2 个 exit、1 个 fruit
        level.rooms[0].entities = vec![
            spec("player_spawn", (64, 464)),
            spec("player_spawn", (96, 464)),
            spec("exit", (448, 464)),
            spec("exit", (416, 464)),
            spec("fruit", (336, 432)),
        ];
        // 非起始房间误放了一个 spawn + 两个 exit
        level.rooms[1].entities = vec![
            spec("player_spawn", (64, 464)),
            spec("exit", (448, 464)),
            spec("exit", (416, 464)),
        ];
        let mut session = EditorSession::new(level, None);
        // 数量是实现细节，契约只看**结果**：全关 1 个 spawn、每房 1 个 exit
        assert!(session.dedupe_singletons() > 0);
        let kinds = |room: usize| -> Vec<&str> {
            session.doc.rooms[room]
                .entities
                .iter()
                .map(|spec| spec.kind.as_str())
                .collect()
        };
        assert_eq!(kinds(0), ["player_spawn", "exit", "fruit"]);
        assert_eq!(kinds(1), ["exit"]);
        // 一个 undo 步骤整步回退
        assert!(session.undo());
        assert_eq!(session.doc.rooms[0].entities.len(), 5);
        assert_eq!(session.doc.rooms[1].entities.len(), 3);
        // 无重复时不再产生编辑
        session.dedupe_singletons();
        assert_eq!(session.dedupe_singletons(), 0);
    }


    /// 任意房间实体删除：一个 undo 步骤、可整步回退，越界安全。
    #[test]
    fn delete_entity_works_across_rooms_with_undo() {
        let mut level = sample_level();
        level.grid = level_format::Grid { cols: 2, rows: 1 };
        level.rooms.push(level.rooms[0].clone());
        level.rooms[1].entities.push(level_format::EntitySpec {
            kind: "player_spawn".into(),
            at: (64, 464),
            size: None,
            id: None,
        });
        let mut session = EditorSession::new(level, None);
        let before = session.doc.rooms[1].entities.len();
        assert!(session.delete_entity(1, before - 1));
        assert_eq!(session.doc.rooms[1].entities.len(), before - 1);
        assert!(session.undo());
        assert_eq!(session.doc.rooms[1].entities.len(), before);
        assert!(!session.delete_entity(9, 0));
        assert!(!session.delete_entity(0, 999));
    }

    /// 保存 = 序列化 → 校验 → `.ron.tmp` → rename；读回与 `doc` 等价。
    #[test]
    fn save_round_trips_through_the_single_serializer() {
        let mut session = EditorSession::new(sample_level(), None);
        session.begin_edit();
        session.set_tile(2, 2, TileKind::OneWay);
        session.set_goals(vec![Goal::ReachExit, Goal::Collect { count: 2 }]);
        let path = std::env::temp_dir().join("animj_editor_save_test.level.ron");
        session.path = Some(path.clone());
        session.save().expect("保存应成功");
        let text = std::fs::read_to_string(&path).expect("文件应存在");
        assert!(text.ends_with('\n'), "文件必须以 LF 结尾");
        assert!(!text.contains("\r\n"), "必须是 LF 换行");
        let read_back = level_format::read(&text).expect("应能解析");
        assert_eq!(read_back, session.doc, "加载→保存必须是恒等变换");
        assert!(!session.dirty);
        assert_eq!(session.saved_text.as_deref(), Some(text.as_str()));
        let _ = std::fs::remove_file(path);
    }

    /// Fatal 级问题拒绝写盘（`08` §8.8.3）：这里伪造「瓦片行数不对」。
    #[test]
    fn fatal_issues_block_saving() {
        let mut session = EditorSession::new(sample_level(), None);
        session.path = Some(std::env::temp_dir().join("animj_should_not_exist.level.ron"));
        session.doc.rooms[0].tiles.pop(); // V5：行数 ≠ ROOM_TILES
        let error = session.save().expect_err("Fatal 必须阻断保存");
        assert!(error.contains("Fatal"), "{error}");
        assert!(!session.dirty || session.path.is_some());
        let _ = std::fs::remove_file(session.path.clone().unwrap());
    }
}
