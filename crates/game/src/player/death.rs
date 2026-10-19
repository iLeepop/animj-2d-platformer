//! Hazard 死亡与重生。所有者文档：`docs/06-physics-and-movement.md` §6.5.3、`docs/02-world-and-rooms.md` §2.5。
//!
//! 物理侧只负责**发出信号**并提供一个干净的 `Body`；重置行为的规范在 `02` §2.5：
//! 回到已激活的存档点；没有存档点时回到关卡出生点（`player_spawn` 所在房间）。

use bevy::prelude::*;

use super::{PlayerId, PlayerMotion, PlayerStepOutcome};
use crate::checkpoint::{ActiveCheckpoint, CheckpointPoint};
use crate::config::physics::HAZARD_INSET;
use crate::physics::{TileKind, overlaps_tile_kind};
use crate::state::GameSet;
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;

/// 角色死亡（buffered 消息，命名与载荷见 `05` §5.6）。
///
/// 多人时按玩家区分：只重置死亡的玩家，其他玩家继续（`01` §1.6）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayerDied {
    pub player_id: PlayerId,
    pub room_index: u32,
}

pub struct PlayerDeathPlugin;

impl Plugin for PlayerDeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayerDied>().add_systems(
            FixedUpdate,
            (check_hazards, respawn_on_death)
                .chain()
                .after(crate::player::apply_player_physics)
                .in_set(GameSet::Physics)
                .run_if(crate::net::not_client),
        );
    }
}

/// Hazard 传感器：角色 AABB **内缩** `HAZARD_INSET` 后与尖刺相交即死亡（`06` §6.5.3）。
///
/// 同一固定步只写一条消息（单角色场景天然满足）。
pub fn check_hazards(
    tiles: Option<Res<LevelTiles>>,
    loaded: Option<Res<LoadedLevel>>,
    players: Query<(&PlayerId, &PlayerMotion)>,
    mut died: MessageWriter<PlayerDied>,
) {
    let (Some(tiles), Some(loaded)) = (tiles, loaded) else {
        return;
    };
    let grid = (loaded.level.grid.cols, loaded.level.grid.rows);
    for (id, motion) in &players {
        if !overlaps_tile_kind(
            tiles.as_ref(),
            motion.0.aabb,
            -HAZARD_INSET,
            TileKind::Hazard,
        ) {
            continue;
        }
        // 死亡房间按**该玩家**当前位置算；多玩家可能分处不同房间。
        let room_index = crate::coords::world_to_room(motion.0.aabb.center(), grid.0, grid.1)
            .map(|pos| level_format::room_index(pos.col, pos.row, grid.0) as u32)
            .unwrap_or(0);
        died.write(PlayerDied {
            player_id: *id,
            room_index,
        });
    }
}

/// 死亡落点（`02` §2.5）：已激活的存档点优先，否则关卡出生点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RespawnPoint {
    /// AABB 中心（世界像素）。
    pub center: Vec2,
    /// 落点所在房间 `(col, row)`。
    pub room: (u32, u32),
    /// `true` = 使用了存档点；`false` = 回退到关卡出生点。
    pub from_checkpoint: bool,
}

/// 选死亡落点：有效存档点优先；否则**关卡出生点**（`player_spawn` 所在房间的重生点）。
///
/// 注意：不是"死亡时所在房间"的重生点——无论玩家死在哪个房间，无存档点时都回出生点（`02` §2.5）。
pub fn respawn_target(
    level: &level_format::Level,
    checkpoint: Option<CheckpointPoint>,
) -> RespawnPoint {
    if let Some(point) = checkpoint.filter(|point| crate::checkpoint::exists(level, *point)) {
        return RespawnPoint {
            center: point.world_center(level.grid.rows),
            room: (point.col, point.row),
            from_checkpoint: true,
        };
    }
    let (col, row, center) = crate::world::initial_spawn(level);
    RespawnPoint {
        center,
        room: (col, row),
        from_checkpoint: false,
    }
}

/// 收到死亡信号后把角色放回**存档点**（无则关卡出生点），并清空速度与计时器（`02` §2.5）。
pub fn respawn_on_death(
    mut died: MessageReader<PlayerDied>,
    loaded: Option<Res<LoadedLevel>>,
    mut current: Option<ResMut<CurrentRoom>>,
    active: Option<Res<ActiveCheckpoint>>,
    tiles: Option<Res<LevelTiles>>,
    mut players: Query<(&PlayerId, &mut PlayerMotion, &mut PlayerStepOutcome)>,
) {
    let dead: Vec<PlayerId> = died.read().map(|message| message.player_id).collect();
    if dead.is_empty() {
        return;
    }
    let (Some(loaded), Some(current)) = (loaded, current.as_mut()) else {
        return;
    };
    let now = (current.col, current.row);
    let checkpoint = active.and_then(|active| active.0);
    let target = respawn_target(&loaded.level, checkpoint);
    if target.room != now {
        // 存档点/出生点可能在别的房间：显式切房间，避免滞回判定让相机/裁剪滞后一帧以上。
        current.col = target.room.0;
        current.row = target.room.1;
        info!(
            "角色死亡：回到{}所在房间 ({}, {})",
            if target.from_checkpoint {
                "存档点"
            } else {
                "出生点"
            },
            target.room.0,
            target.room.1
        );
    }
    for id in &dead {
        if target.from_checkpoint {
            info!(
                "玩家 {} 死亡：回到存档点 ({:.0}, {:.0})",
                id.0, target.center.x, target.center.y
            );
        } else {
            info!(
                "玩家 {} 死亡：回到出生点 ({:.0}, {:.0})（房间 ({}, {})）",
                id.0, target.center.x, target.center.y, target.room.0, target.room.1
            );
        }
    }

    // 多人同时死亡时按 id 分列，避免两个盒子完全重叠。
    let count = dead.iter().map(|id| id.index()).max().unwrap_or(0) + 1;
    let centers = tiles
        .as_ref()
        .map(|tiles| {
            crate::player::spawn::player_spawn_centers(target.center, count, tiles.as_ref())
        })
        .unwrap_or_else(|| vec![target.center; count]);

    for (id, mut motion, mut outcome) in &mut players {
        if !dead.contains(id) {
            continue;
        }
        let center = centers.get(id.index()).copied().unwrap_or(target.center);
        // `Body::at_center` 保证「土狼时间用尽」，因此重生后不得凭空起跳（`06` §6.5.3 / T21）
        motion.0 = crate::physics::Body::at_center(center);
        outcome.0 = crate::physics::StepOutcome::default();
    }
}

#[cfg(test)]
mod tests {
    use level_format::{Grid, Room, RoomCoord};

    use super::*;
    use crate::checkpoint::CheckpointPoint;

    fn level_with_checkpoint(checkpoint_at: Option<(i32, i32)>) -> level_format::Level {
        let mut tiles = vec!["................".to_string(); level_format::ROOM_TILES];
        tiles[level_format::ROOM_TILES - 1] = "#".repeat(level_format::ROOM_TILES);
        let room = |player_spawn: bool, checkpoint_at: Option<(i32, i32)>| {
            let mut entities = Vec::new();
            if player_spawn {
                entities.push(level_format::EntitySpec {
                    kind: "player_spawn".to_string(),
                    at: (64, 464),
                    size: None,
                    id: None,
                });
            }
            if let Some(at) = checkpoint_at {
                entities.push(level_format::EntitySpec {
                    kind: crate::checkpoint::KIND.to_string(),
                    at,
                    size: None,
                    id: None,
                });
            }
            Room {
                name: None,
                tiles: tiles.clone(),
                goals: vec![level_format::Goal::ReachExit],
                reward: None,
                spawn: None,
                entities,
            }
        };
        level_format::Level {
            version: level_format::VERSION,
            name: "t".to_string(),
            grid: Grid { cols: 2, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![room(true, checkpoint_at), room(false, None)],
        }
    }

    fn level_spawn(level: &level_format::Level) -> (u32, u32, Vec2) {
        crate::world::initial_spawn(level)
    }

    /// 没有存档点 ⇒ 回**关卡出生点**，而不是死亡时所在房间的重生点（`02` §2.5）。
    #[test]
    fn no_checkpoint_falls_back_to_level_spawn_in_any_room() {
        let level = level_with_checkpoint(None);
        let (col, row, center) = level_spawn(&level);
        let target = respawn_target(&level, None);
        assert_eq!(target.center, center);
        assert_eq!(target.room, (col, row));
        assert_eq!(target.room, (0, 0), "落点必须是出生房间");
        assert!(!target.from_checkpoint);
    }

    /// 存档点可以在别的房间：必须回到它的世界位置与房间。
    #[test]
    fn checkpoint_wins_even_across_rooms() {
        let at = (100, 464);
        let level = level_with_checkpoint(Some(at));
        let point = CheckpointPoint { col: 0, row: 0, at };
        let target = respawn_target(&level, Some(point));
        assert_eq!(target.center, point.world_center(level.grid.rows));
        assert_eq!(target.room, (0, 0));
        assert!(target.from_checkpoint);
    }

    /// 存档点已从关卡里删掉（编辑/热重载）⇒ 回出生点，不能把角色放到不存在的位置。
    #[test]
    fn stale_checkpoint_falls_back_to_level_spawn() {
        let level = level_with_checkpoint(None);
        let point = CheckpointPoint {
            col: 0,
            row: 0,
            at: (100, 464),
        };
        let (col, row, center) = level_spawn(&level);
        let target = respawn_target(&level, Some(point));
        assert!(!target.from_checkpoint);
        assert_eq!(target.room, (col, row));
        assert_eq!(target.center, center);
    }

    /// 端到端：在**别的房间**死亡且没有存档点 ⇒ 角色与 `CurrentRoom` 都回到出生房间。
    #[test]
    fn dying_in_another_room_returns_to_the_level_spawn() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::physics::Body;

        let level = level_with_checkpoint(None);
        let (spawn_col, spawn_row, spawn_center) = level_spawn(&level);
        let mut app = App::new();
        app.add_message::<PlayerDied>()
            .init_resource::<ActiveCheckpoint>()
            .insert_resource(LoadedLevel {
                level: level.clone(),
                handle: Handle::default(),
            })
            .insert_resource(CurrentRoom { col: 1, row: 0 });
        let player = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(Body::at_center(Vec2::new(700.0, 40.0))),
                PlayerStepOutcome::default(),
            ))
            .id();
        app.world_mut().write_message(PlayerDied {
            player_id: PlayerId(0),
            room_index: 1,
        });
        app.world_mut()
            .run_system_once(respawn_on_death)
            .expect("死亡重置系统应能运行");

        let motion = &app.world().get::<PlayerMotion>(player).unwrap().0;
        assert_eq!(
            motion.aabb.center(),
            spawn_center,
            "无存档点时必须回关卡出生点，而不是死亡房间的重生点"
        );
        let current = app.world().resource::<CurrentRoom>();
        assert_eq!(
            (current.col, current.row),
            (spawn_col, spawn_row),
            "CurrentRoom 必须同步切回出生房间"
        );
    }
}
