//! 关卡加载与生成。所有者文档：`docs/02-world-and-rooms.md` §2.3、`docs/05-architecture.md` §5.3。

pub mod load;
pub mod progress;

use bevy::prelude::*;

use crate::checkpoint::ActiveCheckpoint;
use crate::player::PlayerMotion;
use crate::player::decor::Decorations;
use crate::render::tiles::RoomRoot;
use crate::room::RoomZone;
use crate::state::{GameState, StartupFlow};
use crate::tiles::LevelTiles;
use crate::world::load::{LoadedLevel, PendingLevel};
use crate::world::progress::LevelProgress;

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
            .init_resource::<load::LoadNotice>()
            .add_systems(OnEnter(GameState::Boot), start_flow)
            .add_systems(OnEnter(GameState::Loading), load::request_level)
            .add_systems(OnEnter(GameState::MainMenu), teardown_level)
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

/// `Boot` 只决定入口（`05` §5.5）：发布进程进主菜单，编辑器/冒烟直接请求默认关卡。
fn start_flow(
    flow: Res<StartupFlow>,
    mut commands: Commands,
    mut next: ResMut<NextState<GameState>>,
) {
    match *flow {
        StartupFlow::Menu => next.set(GameState::MainMenu),
        StartupFlow::Direct => {
            commands.insert_resource(load::StartRequest {
                level_id: load::DEFAULT_LEVEL_ID.to_string(),
                mode: load::LoadMode::Auto,
            });
            next.set(GameState::Loading);
        }
    }
}

/// 回到主菜单时销毁整局世界（玩家、瓦片、房间实体）并清掉关卡级资源。
///
/// 不这样做的话，下一次「开始游戏」会在旧实体之上再生成一套：两个玩家、两套瓦片、
/// 相机/房间判定随机命中其中之一。装饰品重置为默认值，避免上一局的装备泄漏到新游戏。
///
/// `RoomRoot` 与房间子实体是 `ChildOf` 关系；Bevy 0.20 的 `despawn` 会递归销毁子树
/// （`render/tiles.rs` 的热重载重建依赖同一行为）。
fn teardown_level(
    mut commands: Commands,
    roots: Query<Entity, With<RoomRoot>>,
    zones: Query<Entity, With<RoomZone>>,
    players: Query<Entity, With<PlayerMotion>>,
) {
    for entity in roots.iter().chain(zones.iter()).chain(players.iter()) {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<LoadedLevel>();
    commands.remove_resource::<LevelTiles>();
    commands.remove_resource::<LevelProgress>();
    commands.remove_resource::<PendingLevel>();
    commands.insert_resource(ActiveCheckpoint::default());
    commands.insert_resource(Decorations::default());
    commands.insert_resource(CurrentRoom::default());
}
