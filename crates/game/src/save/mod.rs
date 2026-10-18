//! 存档读档。所有者文档：`docs/05-architecture.md` §5.11、`docs/02-world-and-rooms.md` §2.3。
//!
//! 规则：版本不符 ⇒ 忽略；地图（`level_name` 或 `grid`）不符 ⇒ **进度与装饰品一并忽略**
//! （避免"有装饰品但没通关房间"的矛盾态）；写盘一律 `.tmp` + rename。

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use level_format::Level;
use serde::{Deserialize, Serialize};

use crate::checkpoint::{ActiveCheckpoint, CheckpointPoint};
use crate::player::decor::Decorations;
use crate::world::progress::LevelProgress;

/// 存档格式版本（`05` §5.11）。
pub const SAVE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub level_name: String,
    pub grid: (u32, u32),
    pub progress: LevelProgress,
    pub owned_decorations: Vec<String>,
    pub equipped: Vec<Option<String>>,
    /// 已激活的存档点（`02` §2.5）。旧存档缺省 ⇒ `None`（serde default）。
    #[serde(default)]
    pub checkpoint: Option<CheckpointPoint>,
}

impl SaveData {
    pub fn capture(
        progress: &LevelProgress,
        decorations: &Decorations,
        checkpoint: Option<CheckpointPoint>,
    ) -> Self {
        Self {
            version: SAVE_VERSION,
            level_name: progress.level_name.clone(),
            grid: progress.grid,
            progress: progress.clone(),
            owned_decorations: decorations.owned().to_vec(),
            equipped: decorations.equipped(),
            checkpoint,
        }
    }

    /// 校验并恢复。返回 `None` 表示整份存档被忽略（版本/地图不符）。
    ///
    /// 存档点若已不在关卡数据中（关卡被编辑/删除了该实体）⇒ 只丢弃存档点，
    /// 其它进度照常恢复（`02` §2.5）。
    pub fn restore(
        self,
        level: &Level,
    ) -> Option<(LevelProgress, Decorations, Option<CheckpointPoint>)> {
        if self.version != SAVE_VERSION {
            warn!(
                "存档版本 {} 与当前 {} 不符，忽略存档（05 §5.11）",
                self.version, SAVE_VERSION
            );
            return None;
        }
        if self.level_name != level.name
            || self.grid != (level.grid.cols, level.grid.rows)
            || self.progress.rooms.len() != level.rooms.len()
        {
            warn!(
                "存档地图 {}({}) 与当前 {}({}) 不符，忽略存档（含装饰品）",
                self.level_name,
                format!("{}x{}", self.grid.0, self.grid.1),
                level.name,
                format!("{}x{}", level.grid.cols, level.grid.rows)
            );
            return None;
        }
        let checkpoint = self.checkpoint.filter(|point| {
            let valid = crate::checkpoint::exists(level, *point);
            if !valid {
                warn!("存档点已不在关卡中（关卡被编辑），忽略存档点");
            }
            valid
        });
        let decorations = Decorations::restore(self.owned_decorations, self.equipped);
        Some((self.progress, decorations, checkpoint))
    }
}

/// 存档路径的**唯一**决定处（`05` §5.11）。
///
/// `ANIMJ_SAVE_DIR` 是自动化/CI 的覆盖入口（不改变发布行为）。
pub fn save_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("ANIMJ_SAVE_DIR") {
        return PathBuf::from(dir).join("save.ron");
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("animj").join("save.ron")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("animj").join("save.ron")
    }
}

pub fn to_text(data: &SaveData) -> String {
    let config = ron::ser::PrettyConfig::new()
        .indentor("    ")
        .new_line("\n");
    ron::ser::to_string_pretty(data, config).unwrap_or_default()
}

pub fn from_text(text: &str) -> Result<SaveData, String> {
    ron::from_str(text).map_err(|error| error.to_string())
}

/// 原子写：先写 `save.ron.tmp`，再 rename 覆盖。
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("ron.tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, path)
}

/// 读档并校验；文件不存在 / 解析失败 / 校验失败都返回 `None`（不 panic、不猜测）。
pub fn load_validated(
    level: &Level,
) -> Option<(LevelProgress, Decorations, Option<CheckpointPoint>)> {
    let path = save_path();
    let text = std::fs::read_to_string(&path).ok()?;
    match from_text(&text) {
        Ok(data) => data.restore(level),
        Err(error) => {
            warn!("存档解析失败（{}）：{error}", path.display());
            None
        }
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, save_on_progress)
            .add_systems(Update, save_on_checkpoint)
            .add_systems(Last, save_on_exit);
    }
}

/// 进度变化（通关 / 授奖）即存档：写盘量很小，且保证崩溃不丢进度。
fn save_on_progress(
    mut cleared: MessageReader<crate::room::RoomCleared>,
    mut granted: MessageReader<crate::room::DecorationGranted>,
    progress: Option<Res<LevelProgress>>,
    decorations: Option<Res<Decorations>>,
    checkpoint: Option<Res<ActiveCheckpoint>>,
) {
    let changed = cleared.read().count() + granted.read().count() > 0;
    if !changed {
        return;
    }
    let (Some(progress), Some(decorations)) = (progress, decorations) else {
        return;
    };
    persist_now(
        progress.as_ref(),
        decorations.as_ref(),
        checkpoint.and_then(|checkpoint| checkpoint.0),
    );
}

/// 设置存档点 ⇒ 立即写盘（`02` §2.5）。
fn save_on_checkpoint(
    mut activated: MessageReader<crate::checkpoint::CheckpointActivated>,
    progress: Option<Res<LevelProgress>>,
    decorations: Option<Res<Decorations>>,
    checkpoint: Option<Res<ActiveCheckpoint>>,
) {
    if activated.read().count() == 0 {
        return;
    }
    let (Some(progress), Some(decorations)) = (progress, decorations) else {
        return;
    };
    persist_now(
        progress.as_ref(),
        decorations.as_ref(),
        checkpoint.and_then(|checkpoint| checkpoint.0),
    );
}

/// 退出应用 ⇒ 存档（`01` §1.6）。
fn save_on_exit(
    mut exit: MessageReader<AppExit>,
    progress: Option<Res<LevelProgress>>,
    decorations: Option<Res<Decorations>>,
    checkpoint: Option<Res<ActiveCheckpoint>>,
) {
    if exit.read().count() == 0 {
        return;
    }
    let (Some(progress), Some(decorations)) = (progress, decorations) else {
        return;
    };
    persist_now(
        progress.as_ref(),
        decorations.as_ref(),
        checkpoint.and_then(|checkpoint| checkpoint.0),
    );
}

/// 立即写盘（暂停菜单的「保存/退出」按钮也用这一条路径）。
pub fn persist_now(
    progress: &LevelProgress,
    decorations: &Decorations,
    checkpoint: Option<CheckpointPoint>,
) {
    let data = SaveData::capture(progress, decorations, checkpoint);
    let path = save_path();
    match write_atomic(&path, &to_text(&data)) {
        Ok(()) => info!("存档已写：{}", path.display()),
        Err(error) => warn!("存档写入失败（{}）：{error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use level_format::{Grid, Level, Room, RoomCoord, ROOM_TILES};

    fn level() -> Level {
        Level {
            version: level_format::VERSION,
            name: "world_01".into(),
            grid: Grid { cols: 2, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            rooms: vec![
                Room {
                    name: None,
                    reward: None,
                    spawn: None,
                    goals: vec![level_format::Goal::ReachExit],
                    tiles: vec![".".repeat(ROOM_TILES); ROOM_TILES],
                    entities: vec![level_format::EntitySpec {
                        kind: crate::checkpoint::KIND.to_string(),
                        at: (96, 464),
                        size: None,
                        id: None,
                    }],
                },
                Room {
                    name: None,
                    reward: Some("item_01".into()),
                    spawn: None,
                    goals: vec![level_format::Goal::ReachExit],
                    tiles: vec![".".repeat(ROOM_TILES); ROOM_TILES],
                    entities: vec![],
                },
            ],
        }
    }

    #[test]
    fn round_trip_preserves_progress_and_decorations() {
        let level = level();
        let mut progress = LevelProgress::from_level(&level);
        progress.mark_cleared(0, 0);
        progress.collect_fruit(1, 0);
        let mut decorations = Decorations::default();
        decorations.grant("item_01");
        decorations.equip("item_01");

        let data = SaveData::capture(&progress, &decorations, None);
        let text = to_text(&data);
        let parsed = from_text(&text).expect("应能解析");
        assert_eq!(parsed, data);

        let (restored_progress, restored_decorations, checkpoint) =
            parsed.restore(&level).expect("应接受");
        assert_eq!(restored_progress, progress);
        assert_eq!(checkpoint, None);
        let layer = crate::player::decor::spec_of("item_01").unwrap().layer;
        assert_eq!(restored_decorations.equipped_at(layer), Some("item_01"));
    }

    /// 存档点随存档往返，并且必须与关卡里的实体逐字段一致。
    #[test]
    fn checkpoint_round_trips_and_validates_against_the_level() {
        let level = level();
        let progress = LevelProgress::from_level(&level);
        let point = CheckpointPoint {
            col: 0,
            row: 0,
            at: (96, 464),
        };
        let data = SaveData::capture(&progress, &Decorations::default(), Some(point));
        let parsed = from_text(&to_text(&data)).expect("应能解析");
        let (_, _, checkpoint) = parsed.restore(&level).expect("应接受");
        assert_eq!(checkpoint, Some(point));
    }

    /// 关卡被编辑、存档点实体已删除 ⇒ 只丢弃存档点，进度仍然恢复。
    #[test]
    fn stale_checkpoint_is_dropped_but_progress_survives() {
        let level = level();
        let progress = LevelProgress::from_level(&level);
        let point = CheckpointPoint {
            col: 0,
            row: 0,
            at: (123, 456),
        };
        let data = SaveData::capture(&progress, &Decorations::default(), Some(point));
        let (_, _, checkpoint) = data.restore(&level).expect("进度应接受");
        assert_eq!(checkpoint, None);
    }

    /// 旧存档（没有 checkpoint 字段）必须能读：缺省为 `None`（`05` §5.11）。
    #[test]
    fn old_save_without_checkpoint_field_is_still_accepted() {
        let level = level();
        let progress = LevelProgress::from_level(&level);
        let data = SaveData::capture(&progress, &Decorations::default(), None);
        let text = to_text(&data);
        let legacy = text.replace("    checkpoint: None,\n", "");
        assert!(!legacy.contains("checkpoint"));
        let parsed = from_text(&legacy).expect("旧存档必须可解析");
        let (_, _, checkpoint) = parsed.restore(&level).expect("旧存档必须被接受");
        assert_eq!(checkpoint, None);
    }

    #[test]
    fn version_mismatch_is_ignored() {
        let level = level();
        let progress = LevelProgress::from_level(&level);
        let mut data = SaveData::capture(&progress, &Decorations::default(), None);
        data.version = 99;
        assert!(data.restore(&level).is_none());
    }

    /// 地图不符 ⇒ 进度与装饰品**一并**忽略（`05` §5.11）。
    #[test]
    fn map_mismatch_drops_everything() {
        let level = level();
        let progress = LevelProgress::from_level(&level);
        let mut decorations = Decorations::default();
        decorations.grant("item_01");
        let data = SaveData::capture(&progress, &decorations, None);
        let mut other = level.clone();
        other.grid.cols = 3;
        other.rooms.push(other.rooms[0].clone());
        assert!(data.restore(&other).is_none());
    }

    #[test]
    fn atomic_write_creates_parent_and_replaces() {
        let dir = std::env::temp_dir().join("animj_save_atomic_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("save.ron");
        write_atomic(&path, "first").expect("首次写");
        write_atomic(&path, "second").expect("覆盖写");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        assert!(!path.with_extension("ron.tmp").exists(), "临时文件必须已 rename");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
