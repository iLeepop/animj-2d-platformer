//! 存档点（`checkpoint`）。所有者文档：`docs/02-world-and-rooms.md` §2.5、
//! `docs/03-level-format.md` §3.4、`docs/05-architecture.md` §5.11。
//!
//! 关卡里放置 `checkpoint` 实体；玩家站在区域内按 `E` 即把该点设为存档点（立即写盘）。
//! 死亡后回到存档点；未设置时回退到关卡出生点（`02` §2.5）。

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::{PlayerInput, PlayerMotion};
use crate::room::{RoomZone, RoomZoneKind, aabb_overlaps};
use crate::state::GameSet;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;

/// 关卡实体 kind（`03` §3.4）。
pub const KIND: &str = "checkpoint";

/// 存档点在关卡中的位置：房间 + 房间局部 `at`（AABB 中心，与 `player_spawn` 同约定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPoint {
    pub col: u32,
    pub row: u32,
    pub at: (i32, i32),
}

impl CheckpointPoint {
    /// 世界像素坐标（AABB 中心）。
    pub fn world_center(self, grid_rows: u32) -> Vec2 {
        crate::coords::room_local_to_world(
            self.col,
            self.row,
            grid_rows,
            Vec2::new(self.at.0 as f32, self.at.1 as f32),
        )
    }

    /// 行主序房间下标。
    pub fn room_index(self, grid_cols: u32) -> u32 {
        level_format::room_index(self.col, self.row, grid_cols) as u32
    }
}

/// 当前生效的存档点（`None` = 尚未设置）。
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveCheckpoint(pub Option<CheckpointPoint>);

/// 存档点被设置（`05` §5.6）；存档插件据此立即写盘。
#[derive(Message, Debug, Clone, Copy)]
pub struct CheckpointActivated {
    pub point: CheckpointPoint,
}

/// 未激活的存档点标记颜色。
pub const ZONE_COLOR: Color = Color::srgb(0.35, 0.70, 0.95);
/// 当前生效的存档点标记颜色。
pub const ACTIVE_COLOR: Color = Color::srgb(0.98, 0.86, 0.35);

/// 存档点是否仍存在于关卡数据中（读档/热重载后校验，`05` §5.11）。
pub fn exists(level: &level_format::Level, point: CheckpointPoint) -> bool {
    if point.col >= level.grid.cols || point.row >= level.grid.rows {
        return false;
    }
    let index = level_format::room_index(point.col, point.row, level.grid.cols);
    level.rooms.get(index).is_some_and(|room| {
        room.entities
            .iter()
            .any(|entity| entity.kind == KIND && entity.at == point.at)
    })
}

pub struct CheckpointPlugin;

impl Plugin for CheckpointPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCheckpoint>()
            .add_message::<CheckpointActivated>()
            .add_systems(
                FixedUpdate,
                activate_checkpoint
                    .after(crate::player::apply_player_physics)
                    .before(crate::player::death::check_hazards)
                    .in_set(GameSet::Physics),
            )
            .add_systems(Update, sync_checkpoint_visual.in_set(GameSet::PostPhysics));
    }
}

/// 站在存档点区域内按 `E` ⇒ 设为当前存档点（`02` §2.5）。
///
/// `interact_pressed` 与跳跃一样是锁存量：这里消费一次，保证一个按下只被一个固定步看到。
fn activate_checkpoint(
    mut input: ResMut<PlayerInput>,
    loaded: Option<Res<LoadedLevel>>,
    current: Option<Res<CurrentRoom>>,
    mut active: ResMut<ActiveCheckpoint>,
    players: Query<&PlayerMotion>,
    zones: Query<&RoomZone>,
    mut activated: MessageWriter<CheckpointActivated>,
) {
    if !input.interact_pressed {
        return;
    }
    input.interact_pressed = false;
    let (Some(loaded), Some(current)) = (loaded, current) else {
        return;
    };
    let Some(motion) = players.iter().next() else {
        return;
    };
    let room_index =
        level_format::room_index(current.col, current.row, loaded.level.grid.cols) as u32;
    for zone in &zones {
        if zone.kind != RoomZoneKind::Checkpoint || zone.room_index != room_index {
            continue;
        }
        if !aabb_overlaps(&motion.0.aabb, &zone.aabb) {
            continue;
        }
        let point = CheckpointPoint {
            col: current.col,
            row: current.row,
            at: zone.at,
        };
        if active.0 != Some(point) {
            active.0 = Some(point);
            activated.write(CheckpointActivated { point });
            info!(
                "存档点已设置：房间 ({}, {}) at ({}, {})",
                point.col, point.row, point.at.0, point.at.1
            );
        }
        break;
    }
}

/// 当前存档点用高亮色标记（其它存档点保持默认色）。只在颜色变化时写组件。
fn sync_checkpoint_visual(
    active: Res<ActiveCheckpoint>,
    loaded: Option<Res<LoadedLevel>>,
    mut zones: Query<(&RoomZone, &mut Sprite)>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    for (zone, mut sprite) in &mut zones {
        if zone.kind != RoomZoneKind::Checkpoint {
            continue;
        }
        let active_here = active.0.is_some_and(|point| {
            point.room_index(loaded.level.grid.cols) == zone.room_index && point.at == zone.at
        });
        let color = if active_here {
            ACTIVE_COLOR
        } else {
            ZONE_COLOR
        };
        if sprite.color != color {
            sprite.color = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use level_format::{Grid, Level, ROOM_TILES, Room, RoomCoord};

    use super::*;
    use crate::physics::{Aabb, Body};
    use crate::room::RoomZoneKind;

    const CHECKPOINT_AT: (i32, i32) = (96, 464);

    fn level() -> Level {
        Level {
            version: level_format::VERSION,
            name: "t".to_string(),
            grid: Grid { cols: 1, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![Room {
                name: None,
                tiles: vec![".".repeat(ROOM_TILES); ROOM_TILES],
                goals: vec![level_format::Goal::ReachExit],
                reward: None,
                spawn: None,
                entities: vec![level_format::EntitySpec {
                    kind: KIND.to_string(),
                    at: CHECKPOINT_AT,
                    size: None,
                    id: None,
                }],
            }],
        }
    }

    fn world_center(at: (i32, i32)) -> Vec2 {
        crate::coords::room_local_to_world(0, 0, 1, Vec2::new(at.0 as f32, at.1 as f32))
    }

    fn app() -> App {
        let level = level();
        let mut app = App::new();
        app.init_resource::<PlayerInput>()
            .init_resource::<ActiveCheckpoint>()
            .add_message::<CheckpointActivated>()
            .insert_resource(LoadedLevel {
                level,
                handle: Handle::default(),
            })
            .insert_resource(CurrentRoom { col: 0, row: 0 });
        app
    }

    fn spawn_zone(app: &mut App, at: (i32, i32)) {
        let center = world_center(at);
        app.world_mut().spawn(RoomZone {
            room_index: 0,
            kind: RoomZoneKind::Checkpoint,
            aabb: Aabb::from_center_size(center, Vec2::splat(32.0)),
            at,
        });
    }

    fn spawn_player(app: &mut App, center: Vec2) {
        app.world_mut().spawn(PlayerMotion(Body::at_center(center)));
    }

    fn press_e(app: &mut App) {
        app.world_mut()
            .resource_mut::<PlayerInput>()
            .interact_pressed = true;
        app.world_mut()
            .run_system_once(activate_checkpoint)
            .expect("存档点系统应能运行");
    }

    /// 站在区域内按 E ⇒ 记录存档点、发消息、消费按键。
    #[test]
    fn pressing_e_inside_the_zone_activates_the_checkpoint() {
        let mut app = app();
        spawn_zone(&mut app, CHECKPOINT_AT);
        spawn_player(&mut app, world_center(CHECKPOINT_AT));
        press_e(&mut app);

        let point = CheckpointPoint {
            col: 0,
            row: 0,
            at: CHECKPOINT_AT,
        };
        assert_eq!(app.world().resource::<ActiveCheckpoint>().0, Some(point));
        assert!(
            !app.world().resource::<PlayerInput>().interact_pressed,
            "按键必须被消费"
        );
        let activated = app
            .world_mut()
            .resource_mut::<Messages<CheckpointActivated>>()
            .drain()
            .count();
        assert_eq!(activated, 1, "激活必须发一条消息（存档插件据此写盘）");
    }

    /// 不在区域内按 E ⇒ 不设置存档点，但按键仍然被消费。
    #[test]
    fn pressing_e_outside_the_zone_does_nothing() {
        let mut app = app();
        spawn_zone(&mut app, CHECKPOINT_AT);
        spawn_player(&mut app, world_center((300, 464)));
        press_e(&mut app);

        assert_eq!(app.world().resource::<ActiveCheckpoint>().0, None);
        assert!(!app.world().resource::<PlayerInput>().interact_pressed);
    }

    /// 存档点的世界坐标与房间下标遵循 `00` §0.3 的唯一换算入口。
    #[test]
    fn checkpoint_world_position_uses_the_shared_coordinate_helpers() {
        let point = CheckpointPoint {
            col: 0,
            row: 0,
            at: CHECKPOINT_AT,
        };
        assert_eq!(
            point.world_center(1),
            crate::coords::room_local_to_world(
                0,
                0,
                1,
                Vec2::new(CHECKPOINT_AT.0 as f32, CHECKPOINT_AT.1 as f32)
            )
        );
        assert_eq!(point.room_index(1), 0);
    }
}
