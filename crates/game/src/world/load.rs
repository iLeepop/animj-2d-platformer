//! 关卡资产的请求、校验与生成。所有者文档：`docs/02-world-and-rooms.md` §2.3。
//!
//! 失败**不静默降级**：校验未通过就停在 `GameState::Loading` 并打印可定位的错误
//! （`docs/AGENTS.md` §3.4）。

use bevy::asset::{AssetEvent, LoadState};
use bevy::sprite::Anchor;
use bevy::prelude::*;
use level_format::{Level, LevelIssue, Severity};

use crate::config::physics::PICKUP_SIZE;
use crate::config::render::z;
use crate::level_asset::LevelAsset;
use crate::physics::Aabb;
use crate::render::pixel_camera::PIXEL_PERFECT_LAYERS;
use crate::room::{RoomZone, RoomZoneKind};
use crate::state::GameState;
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::progress::LevelProgress;

/// M0 默认关卡 id。真正的关卡选择（多关卡/菜单）不在 M0 范围内。
pub const DEFAULT_LEVEL_ID: &str = "world_01";

/// 关卡资产的相对路径（`assets/` 之下）。
pub fn level_asset_path(id: &str) -> String {
    format!("levels/{id}.level.ron")
}

/// 热重载后保证当前房间仍在地图内；越界则回退到起始房间（`02` §2.3）。
pub fn clamp_room(current: (u32, u32), grid: (u32, u32), start: (u32, u32)) -> (u32, u32) {
    if current.0 >= grid.0 || current.1 >= grid.1 {
        start
    } else {
        current
    }
}

/// 关卡里的**实心瓦片数**（非 `.` 字符）。日志用它作为"加载/热重载真的生效了"的可观察证据。
pub fn tile_count(level: &Level) -> usize {
    level
        .rooms
        .iter()
        .map(|room| {
            room.tiles
                .iter()
                .map(|row| row.chars().filter(|c| *c != '.').count())
                .sum::<usize>()
        })
        .sum()
}

/// 已加载并校验通过的关卡。
///
/// 持有资产句柄：热重载时靠它把 `AssetEvent::Modified` 匹配到本关卡（`02` §2.3）。
#[derive(Resource)]
pub struct LoadedLevel {
    pub level: Level,
    pub handle: Handle<LevelAsset>,
}

/// 正在加载的关卡资产。
#[derive(Resource)]
pub struct PendingLevel {
    pub handle: Handle<LevelAsset>,
    pub path: String,
}

pub(crate) fn request_level(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut next: ResMut<NextState<GameState>>,
) {
    let path = level_asset_path(DEFAULT_LEVEL_ID);
    info!("加载关卡资产：{path}");
    let handle = server.load::<LevelAsset>(&path);
    commands.insert_resource(PendingLevel { handle, path });
    next.set(GameState::Loading);
}

pub(crate) fn poll_level(
    mut commands: Commands,
    pending: Option<Res<PendingLevel>>,
    assets: Res<Assets<LevelAsset>>,
    server: Res<AssetServer>,
    player_sheet: Res<crate::player::spawn::PlayerSheet>,
    anim: Res<crate::anim_data::ActiveAnim>,
    mut next: ResMut<NextState<GameState>>,
    mut reported: Local<bool>,
) {
    let Some(pending) = pending else {
        return;
    };

    let Some(asset) = assets.get(&pending.handle) else {
        if !*reported && let Some(LoadState::Failed(err)) = server.get_load_state(&pending.handle) {
            error!("关卡加载失败 {}：{err}", pending.path);
            *reported = true;
        }
        return;
    };

    // 校验：一次拿到全部问题，逐条打印（含规则编号与字段路径）
    let issues: Vec<LevelIssue> = level_format::validate(&asset.level);
    let blocking: Vec<&LevelIssue> = issues
        .iter()
        .filter(|issue| issue.severity != Severity::Warning)
        .collect();
    if !*reported {
        for issue in &issues {
            match issue.severity {
                Severity::Warning => warn!("[{}] {}", issue.path, issue.message),
                Severity::Error | Severity::Fatal => {
                    error!("[{}] {}", issue.path, issue.message)
                }
            }
        }
    }
    if !blocking.is_empty() {
        if !*reported {
            error!(
                "关卡 {} 校验未通过（{} 条阻断性问题），停留在 Loading",
                asset.level.name,
                blocking.len()
            );
            *reported = true;
        }
        return;
    }

    info!(
        "关卡 {} 校验通过：{}×{} 个房间，{} 个瓦片",
        asset.level.name,
        asset.level.grid.cols,
        asset.level.grid.rows,
        tile_count(&asset.level)
    );

    let level = asset.level.clone();
    // 读档必须在决定出生位置**之前**：存档点决定从哪个房间/位置开始（`02` §2.5、`05` §5.11）。
    let (progress, checkpoint) = match crate::save::load_validated(&level) {
        Some((progress, decorations, checkpoint)) => {
            info!(
                "读档：房间 {}/{} 已通关，装饰品 {} 件{}",
                progress.cleared_count(),
                progress.rooms.len(),
                decorations.len(),
                if checkpoint.is_some() {
                    "，存档点已恢复"
                } else {
                    ""
                }
            );
            commands.insert_resource(decorations);
            (progress, checkpoint)
        }
        None => {
            info!("无可用存档（首次启动或存档不匹配）");
            (LevelProgress::from_level(&level), None)
        }
    };
    commands.insert_resource(progress);
    commands.insert_resource(crate::checkpoint::ActiveCheckpoint(checkpoint));

    // 出生位置：有存档点 ⇒ 存档点所在房间/位置；否则 player_spawn 所在房间的重生点。
    let (spawn_col, spawn_row, center) =
        match checkpoint.filter(|point| crate::checkpoint::exists(&level, *point)) {
            Some(point) => (point.col, point.row, point.world_center(level.grid.rows)),
            // 无存档点 ⇒ 关卡出生点（`player_spawn` 所在房间的重生点，`02` §2.5）。
            None => crate::world::initial_spawn(&level),
        };
    commands.insert_resource(CurrentRoom {
        col: spawn_col,
        row: spawn_row,
    });
    info!(
        "出生位置：{} ({:.0}, {:.0})（房间 ({}, {})）",
        if checkpoint.is_some_and(|point| crate::checkpoint::exists(&level, point)) {
            "存档点"
        } else {
            "房间重生点"
        },
        center.x,
        center.y,
        spawn_col,
        spawn_row
    );
    crate::player::spawn::spawn_player(&mut commands, &player_sheet, &anim.set, center);
    commands.insert_resource(LevelTiles::from_level(&level));
    commands.insert_resource(LoadedLevel {
        level,
        handle: pending.handle.clone(),
    });
    commands.remove_resource::<PendingLevel>();
    next.set(GameState::Playing);
}

/// 热重载：`assets/levels/*.level.ron` 改动后重新校验并重建（`02` §2.3、`03` §3.1）。
///
/// 语义：
/// - **校验通过** ⇒ 替换关卡数据、重建瓦片、把越界的当前房间拉回起始房间；玩家位置不动（不打断操作）。
/// - **校验不通过** ⇒ 打印全部可定位问题，**继续使用旧关卡**（不把运行中的会话弄坏）。
///
/// 文件监听由 Bevy 的 `AssetPlugin` 提供（debug 构建默认开启）。
pub(crate) fn hot_reload_level(
    mut events: MessageReader<AssetEvent<LevelAsset>>,
    assets: Res<Assets<LevelAsset>>,
    mut loaded: ResMut<LoadedLevel>,
    mut current: ResMut<CurrentRoom>,
    mut commands: Commands,
) {
    let mut modified = false;
    for event in events.read() {
        debug!("关卡资产事件：{event:?}");
        if matches!(event, AssetEvent::Modified { id } if *id == loaded.handle.id()) {
            modified = true;
        }
    }
    if !modified {
        return;
    }
    let Some(asset) = assets.get(&loaded.handle) else {
        return;
    };

    let issues = level_format::validate(&asset.level);
    let blocking = issues
        .iter()
        .filter(|issue| issue.severity != Severity::Warning)
        .count();
    for issue in &issues {
        match issue.severity {
            Severity::Warning => warn!("[{}] {}", issue.path, issue.message),
            Severity::Error | Severity::Fatal => error!("[{}] {}", issue.path, issue.message),
        }
    }
    if blocking > 0 {
        error!("关卡热重载被拒绝：{blocking} 条阻断性问题，继续使用旧关卡");
        return;
    }

    let old_name = loaded.level.name.clone();
    let old_grid = (loaded.level.grid.cols, loaded.level.grid.rows);
    let old_room_count = loaded.level.rooms.len();
    loaded.level = asset.level.clone();
    if (loaded.level.grid.cols, loaded.level.grid.rows) != old_grid
        || loaded.level.rooms.len() != old_room_count
    {
        commands.insert_resource(LevelProgress::from_level(&loaded.level));
        warn!("热重载：关卡结构变化，进度已重置（`02` §2.3）");
    }
    commands.insert_resource(LevelTiles::from_level(&loaded.level));
    let grid = (loaded.level.grid.cols, loaded.level.grid.rows);
    let start = (loaded.level.start.col, loaded.level.start.row);
    let clamped = clamp_room((current.col, current.row), grid, start);
    if clamped != (current.col, current.row) {
        current.col = clamped.0;
        current.row = clamped.1;
        warn!("热重载后当前房间越界，已回退到起始房间");
    }
    info!(
        "关卡热重载：{old_name} → {}（{} 个房间，{} 个瓦片）",
        loaded.level.name,
        loaded.level.rooms.len(),
        tile_count(&loaded.level)
    );
}

/// 生成关卡实体（`exit` / `fruit`）：常驻，按房间持有世界 AABB（`02` §2.4）。
///
/// 视觉是占位色块，尺寸**等于判定尺寸**（拾取物 12×12、区域用声明尺寸/默认 32×32）。
fn spawn_room_entities(commands: &mut Commands, level: &Level) {
    let (cols, rows) = (level.grid.cols, level.grid.rows);
    for (index, room) in level.rooms.iter().enumerate() {
        let (col, row) = ((index as u32) % cols, (index as u32) / cols);
        for spec in &room.entities {
            let kind = match spec.kind.as_str() {
                "exit" => RoomZoneKind::Exit,
                "fruit" => RoomZoneKind::Fruit,
                crate::checkpoint::KIND => RoomZoneKind::Checkpoint,
                // `player_spawn` 只用于放置玩家；未知 kind 由校验拦截（V 规则）
                _ => continue,
            };
            let size = match kind {
                RoomZoneKind::Fruit => Vec2::splat(PICKUP_SIZE),
                RoomZoneKind::Exit | RoomZoneKind::Checkpoint => spec
                    .size
                    .map(|(w, h)| Vec2::new(w as f32, h as f32))
                    .unwrap_or(Vec2::splat(32.0)),
            };
            let local = Vec2::new(spec.at.0 as f32, spec.at.1 as f32);
            let center = crate::coords::room_local_to_world(col, row, rows, local);
            let aabb = Aabb::from_center_size(center, size);
            let (color, zone_z) = match kind {
                RoomZoneKind::Exit => (Color::srgb(0.30, 0.85, 0.42), z::PICKUP),
                RoomZoneKind::Fruit => (Color::srgb(0.92, 0.36, 0.55), z::PICKUP),
                // 存档点单独一层：可能与出口/果实重叠（`00` §0.4 禁止共享 Z）。
                RoomZoneKind::Checkpoint => (crate::checkpoint::ZONE_COLOR, z::CHECKPOINT),
            };
            commands.spawn((
                RoomZone {
                    room_index: index as u32,
                    kind,
                    aabb,
                    at: spec.at,
                },
                Sprite::from_color(color, size),
                Anchor::BOTTOM_CENTER,
                Transform::from_xyz(center.x, aabb.min.y, zone_z),
                PIXEL_PERFECT_LAYERS,
            ));
        }
    }
}

/// 关卡变化（首次加载 / 热重载）时重建房间实体：先销毁旧的、再按最新数据生成。
pub(crate) fn sync_room_entities(
    mut commands: Commands,
    loaded: Option<Res<LoadedLevel>>,
    old: Query<Entity, With<RoomZone>>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    for entity in &old {
        commands.entity(entity).despawn();
    }
    spawn_room_entities(&mut commands, &loaded.level);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 热重载把地图改小后，越界的房间必须回到起始房间（不能让相机/瓦片查空）。
    #[test]
    fn clamp_room_falls_back_to_start_when_out_of_bounds() {
        let start = (0, 0);
        // 在图内：原样返回（包括恰好是最后一行/列）
        assert_eq!(clamp_room((0, 0), (2, 1), start), (0, 0));
        assert_eq!(clamp_room((1, 0), (2, 1), start), (1, 0));
        // 越界：回退
        assert_eq!(clamp_room((2, 0), (2, 1), start), start);
        assert_eq!(clamp_room((0, 1), (2, 1), start), start);
        assert_eq!(clamp_room((9, 9), (2, 1), start), start);
    }

    /// 实心瓦片数只数非 `.` 字符（日志里「42 → 43」的可观察证据靠它）。
    #[test]
    fn tile_count_ignores_empty() {
        let level = level_format::Level {
            version: level_format::VERSION,
            name: "t".into(),
            grid: level_format::Grid { cols: 1, rows: 1 },
            start: level_format::RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            rooms: vec![level_format::Room {
                name: None,
                reward: None,
                spawn: None,
                goals: vec![],
                tiles: vec!["##..".into(), "....".into(), "=.=.".into()],
                entities: vec![],
            }],
        };
        assert_eq!(tile_count(&level), 4); // ##.. = 2，.... = 0，=.=. = 2
    }
}
