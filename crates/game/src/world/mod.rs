//! 关卡加载与生成。所有者文档：`docs/02-world-and-rooms.md` §2.3、`docs/05-architecture.md` §5.3。

pub mod load;
pub mod progress;

use bevy::prelude::*;

use crate::state::GameState;

/// 当前房间（行主序之外的独立坐标，`row = 0` 是地图最上一行）。见 `02` §2.2。
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentRoom {
    pub col: u32,
    pub row: u32,
}

/// 房间重生点（世界像素，玩家 AABB 中心）。
///
/// 优先级：房间的 `spawn` 字段 > 引擎默认值（`config::room::DEFAULT_SPAWN_LOCAL`）。见 `02` §2.5。
pub fn room_spawn_center(level: &level_format::Level, col: u32, row: u32) -> Vec2 {
    let index = level_format::room_index(col, row, level.grid.cols);
    let local = level
        .rooms
        .get(index)
        .and_then(|room| room.spawn)
        .map(|(x, y)| Vec2::new(x as f32, y as f32))
        .unwrap_or(crate::config::room::DEFAULT_SPAWN_LOCAL);
    crate::coords::room_local_to_world(col, row, level.grid.rows, local)
}

/// 关卡**出生点**：`player_spawn` 所在房间 + 该房间的重生点（世界像素，AABB 中心）。
///
/// 这是没有存档点时的死亡回退位置，也是首次启动的出生位置（`02` §2.5）。
/// 若关卡缺少 `player_spawn`（校验会拦截），回退到 `start` 房间。
pub fn initial_spawn(level: &level_format::Level) -> (u32, u32, Vec2) {
    let spawn_room = level
        .rooms
        .iter()
        .enumerate()
        .find(|(_, room)| room.entities.iter().any(|spec| spec.kind == "player_spawn"))
        .map(|(index, _)| index as u32)
        .unwrap_or_else(|| {
            level_format::room_index(level.start.col, level.start.row, level.grid.cols) as u32
        });
    let (col, row) = (spawn_room % level.grid.cols, spawn_room / level.grid.cols);
    (col, row, room_spawn_center(level, col, row))
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentRoom>()
            .add_systems(OnEnter(GameState::Boot), load::request_level)
            .add_systems(
                Update,
                load::poll_level.run_if(in_state(GameState::Loading)),
            )
            .add_systems(
                Update,
                load::sync_room_entities.run_if(resource_exists_and_changed::<load::LoadedLevel>),
            )
            .add_systems(
                Update,
                load::hot_reload_level
                    .in_set(crate::state::GameSet::Room)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
