//! 房间与关卡进度。所有者文档：`docs/02-world-and-rooms.md` §2.3、`docs/01-game-design.md` §1.6。
//!
//! **单调性**是硬契约：`cleared` 与 `collected_fruit` 只增不减；死亡、重进房间、热重载都不重置。
//! 这里只放**纯数据 + 纯方法**，读写时机由 `room/` 与 `world/load.rs` 决定。

use bevy::prelude::*;
use level_format::{Level, room_index};
use serde::{Deserialize, Serialize};

/// 单个房间的进度。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomProgress {
    /// 目标条件**曾**全部满足。单调，不可回退。
    pub cleared: bool,
    /// 该房间已收集的 `fruit` 数量。单调。
    pub collected_fruit: u32,
}

/// 整关进度。`rooms` 按**行主序**排列（`row * cols + col`，`00` §0.3）。
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelProgress {
    /// 与 `.level.ron` 的 `name` 一致（读档时用来校验是否同一张图）。
    pub level_name: String,
    pub grid: (u32, u32),
    pub rooms: Vec<RoomProgress>,
}

impl LevelProgress {
    pub fn from_level(level: &Level) -> Self {
        Self {
            level_name: level.name.clone(),
            grid: (level.grid.cols, level.grid.rows),
            rooms: vec![RoomProgress::default(); level.rooms.len()],
        }
    }

    pub fn room(&self, col: u32, row: u32) -> Option<&RoomProgress> {
        self.rooms.get(room_index(col, row, self.grid.0))
    }

    pub fn room_mut(&mut self, col: u32, row: u32) -> Option<&mut RoomProgress> {
        let index = room_index(col, row, self.grid.0);
        self.rooms.get_mut(index)
    }

    /// 标记房间通关；返回**是否发生了 false→true 的跃变**（用于"奖励只发一次"）。
    pub fn mark_cleared(&mut self, col: u32, row: u32) -> bool {
        match self.room_mut(col, row) {
            Some(room) if !room.cleared => {
                room.cleared = true;
                true
            }
            _ => false,
        }
    }

    /// 记一次果实收集（单调）。
    pub fn collect_fruit(&mut self, col: u32, row: u32) -> u32 {
        match self.room_mut(col, row) {
            Some(room) => {
                room.collected_fruit = room.collected_fruit.saturating_add(1);
                room.collected_fruit
            }
            None => 0,
        }
    }

    pub fn cleared_count(&self) -> usize {
        self.rooms.iter().filter(|room| room.cleared).count()
    }

    pub fn all_cleared(&self) -> bool {
        !self.rooms.is_empty() && self.rooms.iter().all(|room| room.cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use level_format::{Grid, Room, RoomCoord};

    fn level(cols: u32, rows: u32) -> Level {
        let room = |i: usize| Room {
            name: Some(format!("r{i}")),
            reward: None,
            spawn: None,
            goals: vec![level_format::Goal::ReachExit],
            tiles: vec!["................".to_string(); 16],
            entities: vec![],
        };
        Level {
            version: level_format::VERSION,
            name: "t".into(),
            grid: Grid { cols, rows },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: (0..cols * rows).map(|i| room(i as usize)).collect(),
        }
    }

    #[test]
    fn clears_are_monotonic_and_report_the_transition() {
        let mut progress = LevelProgress::from_level(&level(2, 2));
        assert!(!progress.all_cleared());
        assert!(progress.mark_cleared(1, 1));
        assert!(progress.room(1, 1).unwrap().cleared);
        // 第二次不再跃变（奖励幂等的依据）
        assert!(!progress.mark_cleared(1, 1));
        assert_eq!(progress.cleared_count(), 1);
    }

    #[test]
    fn fruits_are_monotonic() {
        let mut progress = LevelProgress::from_level(&level(2, 1));
        assert_eq!(progress.collect_fruit(0, 0), 1);
        assert_eq!(progress.collect_fruit(0, 0), 2);
        assert_eq!(progress.room(0, 0).unwrap().collected_fruit, 2);
        // 越界房间不 panic、不产生进度
        assert_eq!(progress.collect_fruit(9, 9), 0);
    }

    #[test]
    fn all_cleared_requires_every_room() {
        let mut progress = LevelProgress::from_level(&level(2, 2));
        for (col, row) in [(0, 0), (1, 0), (0, 1)] {
            progress.mark_cleared(col, row);
        }
        assert!(!progress.all_cleared());
        progress.mark_cleared(1, 1);
        assert!(progress.all_cleared());
    }

    #[test]
    fn row_major_index_matches_level_layout() {
        let progress = LevelProgress::from_level(&level(3, 2));
        // (col=1, row=1) ⇒ index = 1*3 + 1 = 4
        progress.rooms[4].cleared;
        let mut progress = progress;
        assert!(progress.mark_cleared(1, 1));
        assert!(progress.rooms[4].cleared);
    }
}
