//! 角色的 ECS 适配层。所有者文档：`docs/06-physics-and-movement.md` §6.1.5、§6.10。
//!
//! 只做三件事：取输入 → 调 [`crate::physics::step`] → 写回组件。**不含任何物理逻辑**。
//!
//! 多玩家：每个角色带 [`PlayerId`]，输入按 id 放在 [`PlayerInputs`] 的槽位里。
//! **玩家之间不碰撞**（`06` §6.11）：每个角色只与世界瓦片碰撞，因此推进顺序不影响结果；
//! 这里仍按 id 升序处理，让日志与快照顺序稳定。

pub mod collision;
pub mod death;
pub mod input;
pub mod items;
pub mod spawn;

use bevy::prelude::*;

use crate::config::net::MAX_PLAYERS;
use crate::physics::{Body, StepInput, StepOutcome, step};
use crate::state::GameSet;
use crate::tiles::LevelTiles;

/// 玩家编号（`0` = 主机/单人；本地双人为 `0`、`1`；联机客户端由主机分配 `1..MAX_PLAYERS`）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u8);

impl PlayerId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// 角色的物理状态（**逻辑真相**，`06` §6.8.1）。
#[derive(Component, Debug)]
pub struct PlayerMotion(pub Body);

/// 一帧采样出的动作输入（`00` D16 的动作结构）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerInput {
    /// `-1` / `0` / `+1`。
    pub move_x: i8,
    /// **锁存**的"刚按下"：采样置位，物理步消费后清零（见 `input::sample_input`）。
    pub jump_pressed: bool,
    pub jump_held: bool,
    /// **锁存**的"刚按下 E"：采样置位，存档点系统消费后清零（见 `input::sample_input`）。
    pub interact_pressed: bool,
}

/// 按玩家编号索引的输入槽（`06` §6.10）。
///
/// 为什么是资源而不是组件：主机要给**远程**玩家的槽位写包内输入，而远程玩家此时可能
/// 还没有实体（刚加入）；资源让「输入先到、实体后到」不会丢包。
#[derive(Resource, Debug, Clone)]
pub struct PlayerInputs {
    pub slots: [PlayerInput; MAX_PLAYERS],
}

impl Default for PlayerInputs {
    fn default() -> Self {
        Self {
            slots: [PlayerInput::default(); MAX_PLAYERS],
        }
    }
}

impl PlayerInputs {
    pub fn get(&self, id: PlayerId) -> PlayerInput {
        self.slots.get(id.index()).copied().unwrap_or_default()
    }

    pub fn get_mut(&mut self, id: PlayerId) -> &mut PlayerInput {
        // `PlayerId` 由本模块与网络层分配，且被 `MAX_PLAYERS` 夹取；越界回落只是防御。
        let index = id.index().min(MAX_PLAYERS - 1);
        &mut self.slots[index]
    }
}

/// 本固定步的物理结果（表现层与 demo 用；语义见 `06` §6.1.4）。
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PlayerStepOutcome(pub StepOutcome);

/// 角色朝向（`true` = 朝右）。默认朝右（`03` §3.4 的出生点约定）。
///
/// 朝向是纯表现状态：只决定 sprite 是否水平翻转（`01` §1.3、`04` §4.5），
/// **不参与**碰撞与物理（逻辑真相仍是 `Aabb`）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerFacing(pub bool);

impl Default for PlayerFacing {
    fn default() -> Self {
        Self(true)
    }
}

/// 输入方向 ⇒ 朝向：`move_x == 0` 时保持原朝向（停下不翻面）。
pub fn next_facing(current: bool, move_x: i8) -> bool {
    match move_x.signum() {
        0 => current,
        sign => sign > 0,
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            input::PlayerInputPlugin,
            death::PlayerDeathPlugin,
            items::ItemPlugin,
        ))
        .insert_resource(collision::PlayerCollision::load())
        .init_resource::<PlayerInputs>()
        .add_systems(
            Update,
            (update_player_facing.run_if(crate::net::not_client),),
        )
        .add_systems(
            FixedUpdate,
            apply_player_physics
                .in_set(GameSet::Physics)
                .run_if(crate::net::not_client),
        );
    }
}

/// 输入 ⇒ [`PlayerFacing`]（表现层，`Update` 内；输入已在 `PreUpdate` 采样完成）。
fn update_player_facing(
    inputs: Res<PlayerInputs>,
    mut players: Query<(&PlayerId, &mut PlayerFacing), Without<death::Dying>>,
) {
    for (id, mut facing) in &mut players {
        let move_x = inputs.get(*id).move_x;
        if move_x == 0 {
            continue;
        }
        let next = next_facing(facing.0, move_x);
        if facing.0 != next {
            facing.0 = next;
        }
    }
}

/// 把一个固定步推进到**所有**角色实体上。
///
/// 玩家之间不碰撞（`06` §6.11）：每个角色只与世界瓦片碰撞，所以推进顺序不影响结果；
/// 按 `PlayerId` 升序只是让行为与日志可复现。
pub fn apply_player_physics(
    mut inputs: ResMut<PlayerInputs>,
    collision: Res<collision::PlayerCollision>,
    tiles: Option<Res<LevelTiles>>,
    mut players: Query<
        (
            Entity,
            &PlayerId,
            &mut PlayerMotion,
            &mut PlayerStepOutcome,
            &items::CarriedItem,
        ),
        Without<death::Dying>,
    >,
) {
    // 关卡尚未加载时没有瓦片数据：物理不推进（此时 `GameState` 还在 Loading，本系统不会被门控触发，
    // 但保持 Option 以防编辑器/测试场景只加了 PlayerPlugin）。
    let Some(tiles) = tiles else {
        return;
    };
    // 全局玩家碰撞尺寸覆盖默认常量；其余手感参数仍来自 `DEFAULT_PHYSICS`（`06` §6.2.4）。
    let base_cfg = {
        let mut cfg = crate::config::physics::DEFAULT_PHYSICS;
        cfg.player_size = collision.size_vec();
        cfg
    };

    // 查询迭代顺序不稳定：先按 id 排序，让顺序确定。
    let mut order: Vec<(PlayerId, Entity)> = players
        .iter()
        .map(|(entity, id, _, _, _)| (*id, entity))
        .collect();
    order.sort_by_key(|(id, _)| *id);

    for (id, entity) in order {
        let input = inputs.get(id);
        let Ok((_, _, mut motion, mut outcome, carried)) = players.get_mut(entity) else {
            continue;
        };
        // 道具只修正本玩家本步的参数副本（`06` §6.12）。
        let mut cfg = base_cfg;
        items::apply_item_to_physics(&mut cfg, carried.kind);
        let mut out = StepOutcome::default();
        step(
            &cfg,
            tiles.as_ref(),
            &mut motion.0,
            StepInput {
                move_x: input.move_x,
                jump_pressed: input.jump_pressed,
                jump_held: input.jump_held,
            },
            &mut out,
        );
        outcome.0 = out;
        // ★ 消费锁存：一个"按下"恰好被**一个**固定步看到。否则同帧两次固定步会把缓冲反复刷新，
        //   而没有固定步的帧会把按下吞掉（`06` §6.1.5）。
        inputs.get_mut(id).jump_pressed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 零输入保持原朝向；±1 按符号定向。看的是**朝向**，不是速度。
    #[test]
    fn facing_follows_input_but_keeps_direction_when_stopped() {
        assert!(next_facing(true, 0));
        assert!(!next_facing(false, 0));
        assert!(next_facing(false, 1));
        assert!(!next_facing(true, -1));
        assert!(next_facing(true, 1));
        assert!(!next_facing(false, -1));
    }

    /// 输入槽按玩家编号隔离：远程玩家的输入不能覆写本地玩家（`06` §6.10）。
    #[test]
    fn input_slots_are_isolated_per_player() {
        let mut inputs = PlayerInputs::default();
        inputs.get_mut(PlayerId(0)).move_x = -1;
        inputs.get_mut(PlayerId(2)).move_x = 1;
        assert_eq!(inputs.get(PlayerId(0)).move_x, -1);
        assert_eq!(inputs.get(PlayerId(1)).move_x, 0);
        assert_eq!(inputs.get(PlayerId(2)).move_x, 1);
    }

    /// ECS 适配层不再把其他玩家当障碍（`06` §6.11）：
    /// 两个玩家相向而行必须互相穿过，各自轨迹与单独推进一致。
    #[test]
    fn ecs_adapter_players_pass_through_each_other() {
        use bevy::ecs::system::RunSystemOnce;
        use level_format::{Grid, Level, Room, RoomCoord};

        let mut tiles = vec![".".repeat(level_format::ROOM_TILES); level_format::ROOM_TILES];
        *tiles.last_mut().expect("至少一行") = "#".repeat(level_format::ROOM_TILES);
        let level = Level {
            version: level_format::VERSION,
            name: "t".into(),
            grid: Grid { cols: 1, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            items: Vec::new(),
            rooms: vec![Room {
                name: None,
                spawn: None,
                tiles,
                entities: vec![],
            }],
        };
        let mut app = App::new();
        app.init_resource::<PlayerInputs>()
            .insert_resource(crate::player::collision::PlayerCollision::default())
            .insert_resource(LevelTiles::from_level(&level));

        let left = Body::at_center(Vec2::new(100.0, 100.0));
        let right = Body::at_center(Vec2::new(121.0, 100.0));
        let left_entity = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(left),
                PlayerStepOutcome::default(),
                items::CarriedItem::default(),
            ))
            .id();
        let right_entity = app
            .world_mut()
            .spawn((
                PlayerId(1),
                PlayerMotion(right),
                PlayerStepOutcome::default(),
                items::CarriedItem::default(),
            ))
            .id();
        {
            let mut inputs = app.world_mut().resource_mut::<PlayerInputs>();
            inputs.slots[0].move_x = 1;
            inputs.slots[1].move_x = -1;
            inputs.slots[0].jump_pressed = true;
        }

        for _ in 0..60 {
            app.world_mut()
                .run_system_once(apply_player_physics)
                .expect("物理适配层应能运行");
        }

        let left_aabb = app.world().get::<PlayerMotion>(left_entity).unwrap().0.aabb;
        let right_aabb = app
            .world()
            .get::<PlayerMotion>(right_entity)
            .unwrap()
            .0
            .aabb;
        assert!(
            left_aabb.min.x > right_aabb.max.x,
            "玩家必须互相穿过：{left_aabb:?} vs {right_aabb:?}"
        );
        assert!(
            !app.world().resource::<PlayerInputs>().slots[0].jump_pressed,
            "物理步必须消费跳跃锁存"
        );
    }
}
