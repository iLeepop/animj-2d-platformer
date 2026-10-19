//! 关卡资产的请求、校验与生成。所有者文档：`docs/02-world-and-rooms.md` §2.3。
//!
//! 失败**不静默降级**：校验未通过就停在 `GameState::Loading` 并打印可定位的错误
//! （`docs/AGENTS.md` §3.4）。

use bevy::asset::{AssetEvent, LoadState};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::{Level, LevelIssue, Severity};

use crate::config::physics::PICKUP_SIZE;
use crate::config::render::z;
use crate::level_asset::LevelAsset;
use crate::physics::Aabb;
use crate::render::pixel_camera::PIXEL_PERFECT_LAYERS;
use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId};
use crate::state::{GameState, SessionMode};
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::progress::LevelProgress;

/// 默认关卡 id（编辑器直入与冒烟验证用）。玩家入口是主菜单的关卡选择（`05` §5.5）。
pub const DEFAULT_LEVEL_ID: &str = "world_01";

/// 关卡启动方式（`05` §5.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    /// 从零开始：忽略存档，进度与装饰品清空。
    NewGame,
    /// 读档：存档缺失/版本/地图不符 ⇒ 回主菜单并提示（不静默替换成新游戏）。
    Continue,
    /// 有则读档，没有则从零开始。编辑器与冒烟验证沿用旧行为。
    Auto,
}

/// 一次启动请求。由菜单写入，`OnEnter(GameState::Loading)` 消费（`05` §5.5）。
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct StartRequest {
    pub level_id: String,
    pub mode: LoadMode,
}

/// 菜单状态行的提示种类。菜单按当前语言渲染，不在资源里存具体文案。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    #[default]
    None,
    NoSave,
    VersionMismatch,
    LevelNotFound,
    LoadFailed,
    NetRejected,
    NetDisconnected,
    NetTimeout,
    NetLevelMismatch,
}

impl Notice {
    pub fn text(self, strings: &crate::i18n::Strings) -> &'static str {
        match self {
            Notice::None => "",
            Notice::NoSave => strings.no_save,
            Notice::VersionMismatch => strings.save_version_mismatch,
            Notice::LevelNotFound => strings.save_level_not_found,
            Notice::LoadFailed => strings.save_load_failed,
            Notice::NetRejected => strings.net_rejected,
            Notice::NetDisconnected => strings.net_disconnected,
            Notice::NetTimeout => strings.net_timeout,
            Notice::NetLevelMismatch => strings.net_level_mismatch,
        }
    }
}

/// 加载失败/菜单操作的可见提示。菜单读它并显示在状态行。
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoadNotice(pub Notice);

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
    pub mode: LoadMode,
    /// 失败/校验信息是否已经打印过：加载会重复进入本系统，日志只应出现一次。
    pub reported: bool,
}

pub(crate) fn request_level(
    mut commands: Commands,
    server: Res<AssetServer>,
    request: Option<Res<StartRequest>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(request) = request else {
        warn!("进入 Loading 但没有 StartRequest，返回主菜单");
        next.set(GameState::MainMenu);
        return;
    };
    let path = level_asset_path(&request.level_id);
    info!("加载关卡资产：{path}");
    let handle = server.load::<LevelAsset>(&path);
    commands.insert_resource(PendingLevel {
        handle,
        path,
        mode: request.mode,
        reported: false,
    });
    commands.remove_resource::<StartRequest>();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn poll_level(
    mut commands: Commands,
    mut pending: Option<ResMut<PendingLevel>>,
    assets: Res<Assets<LevelAsset>>,
    server: Res<AssetServer>,
    player_sheet: Res<crate::player::spawn::PlayerSheet>,
    anim: Res<crate::anim_data::ActiveAnim>,
    session: Res<SessionMode>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(pending) = pending.as_mut() else {
        return;
    };

    let Some(asset) = assets.get(&pending.handle) else {
        if !pending.reported
            && let Some(LoadState::Failed(err)) = server.get_load_state(&pending.handle)
        {
            error!("关卡加载失败 {}：{err}", pending.path);
            pending.reported = true;
        }
        return;
    };

    // 校验：一次拿到全部问题，逐条打印（含规则编号与字段路径）
    let issues: Vec<LevelIssue> = level_format::validate(&asset.level);
    let blocking: Vec<&LevelIssue> = issues
        .iter()
        .filter(|issue| issue.severity != Severity::Warning)
        .collect();
    if !pending.reported {
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
        if !pending.reported {
            error!(
                "关卡 {} 校验未通过（{} 条阻断性问题），停留在 Loading",
                asset.level.name,
                blocking.len()
            );
            pending.reported = true;
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
    let mode = pending.mode;
    // 读档必须在决定出生位置**之前**：存档点决定从哪个房间/位置开始（`02` §2.5、`05` §5.11）。
    let (progress, decorations, checkpoint) = match mode {
        LoadMode::NewGame => {
            info!("新游戏：忽略存档（关卡 {}）", level.name);
            (
                LevelProgress::from_level(&level),
                crate::player::decor::Decorations::default(),
                None,
            )
        }
        LoadMode::Continue | LoadMode::Auto => match crate::save::load_validated(&level) {
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
                (progress, decorations, checkpoint)
            }
            None if mode == LoadMode::Continue => {
                error!(
                    "读档失败：存档缺失或与关卡 {} 不匹配，返回主菜单",
                    level.name
                );
                commands.insert_resource(LoadNotice(Notice::LoadFailed));
                commands.remove_resource::<PendingLevel>();
                next.set(GameState::MainMenu);
                return;
            }
            None => {
                info!("无可用存档（首次启动或存档不匹配）");
                (
                    LevelProgress::from_level(&level),
                    crate::player::decor::Decorations::default(),
                    None,
                )
            }
        },
    };
    commands.insert_resource(progress);
    commands.insert_resource(decorations);
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
    // 先建瓦片再算出生位：多人出生点需要查询实心格（`02` §2.5）。
    let tiles = LevelTiles::from_level(&level);
    let count = session.local_players();
    let centers = crate::player::spawn::player_spawn_centers(center, count, &tiles);
    for (index, spawn_center) in centers.into_iter().enumerate() {
        crate::player::spawn::spawn_player(
            &mut commands,
            &player_sheet,
            &anim.set,
            crate::player::PlayerId(index as u8),
            spawn_center,
        );
    }
    info!("生成玩家：{count} 名（模式 {:?}）", *session);
    commands.insert_resource(tiles);
    commands.insert_resource(PickedZones::default());
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
    // 房间实体以 0 重新编号；旧编号会让客户端误删新果实（`05` §5.15.5）。
    commands.insert_resource(PickedZones::default());
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
    let mut zone_id = 0u32;
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
                ZoneId(zone_id),
                Sprite::from_color(color, size),
                Anchor::BOTTOM_CENTER,
                Transform::from_xyz(center.x, aabb.min.y, zone_z),
                PIXEL_PERFECT_LAYERS,
            ));
            zone_id += 1;
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
            background: None,
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
