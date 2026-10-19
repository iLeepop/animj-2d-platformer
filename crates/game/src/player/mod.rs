//! 角色的 ECS 适配层。所有者文档：`docs/06-physics-and-movement.md` §6.1.5、§6.10。
//!
//! 只做三件事：取输入 → 调 [`crate::physics::step`] → 写回组件。**不含任何物理逻辑**。
//!
//! 多玩家：每个角色带 [`PlayerId`]，输入按 id 放在 [`PlayerInputs`] 的槽位里。
//! 物理按 id 升序依次推进，后推进的玩家会撞到先推进玩家的新位置（确定性、无推挤抖动）。

pub mod death;
pub mod decor;
pub mod input;
pub mod spawn;

use bevy::prelude::*;

use crate::config::net::MAX_PLAYERS;
use crate::physics::{Aabb, Body, StepInput, StepOutcome, step_with_players};
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
        app.add_plugins((input::PlayerInputPlugin, death::PlayerDeathPlugin))
            .init_resource::<PlayerInputs>()
            .init_resource::<decor::Decorations>()
            .add_systems(
                Update,
                (
                    update_player_facing.run_if(crate::net::not_client),
                    decor::sync_decoration_layers,
                    decor::sync_decoration_facing,
                )
                    .chain(),
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
    mut players: Query<(&PlayerId, &mut PlayerFacing)>,
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

/// 把一个固定步推进到**所有**角色实体上（按 `PlayerId` 升序，确定性）。
pub fn apply_player_physics(
    mut inputs: ResMut<PlayerInputs>,
    tiles: Option<Res<LevelTiles>>,
    mut players: Query<(Entity, &PlayerId, &mut PlayerMotion, &mut PlayerStepOutcome)>,
) {
    // 关卡尚未加载时没有瓦片数据：物理不推进（此时 `GameState` 还在 Loading，本系统不会被门控触发，
    // 但保持 Option 以防编辑器/测试场景只加了 PlayerPlugin）。
    let Some(tiles) = tiles else {
        return;
    };

    // 查询迭代顺序不稳定：先按 id 排序，保证同一输入序列在所有机器上得到同一结果。
    let mut order: Vec<(PlayerId, Entity)> = players
        .iter()
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    order.sort_by_key(|(id, _)| *id);

    let mut aabbs: Vec<(PlayerId, Aabb)> = players
        .iter()
        .map(|(_, id, motion, _)| (*id, motion.0.aabb))
        .collect();
    aabbs.sort_by_key(|(id, _)| *id);

    for (id, entity) in order {
        let input = inputs.get(id);
        // 其余玩家的**当前**位置是障碍；先推进的玩家已经更新过，因此顺序决定结果。
        let others: Vec<Aabb> = aabbs
            .iter()
            .filter(|(other, _)| *other != id)
            .map(|(_, aabb)| *aabb)
            .collect();
        let Ok((_, _, mut motion, mut outcome)) = players.get_mut(entity) else {
            continue;
        };
        let mut out = StepOutcome::default();
        step_with_players(
            &crate::config::physics::DEFAULT_PHYSICS,
            tiles.as_ref(),
            &mut motion.0,
            StepInput {
                move_x: input.move_x,
                jump_pressed: input.jump_pressed,
                jump_held: input.jump_held,
            },
            &others,
            &mut out,
        );
        outcome.0 = out;
        if let Some(entry) = aabbs.iter_mut().find(|(other, _)| *other == id) {
            entry.1 = motion.0.aabb;
        }
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

    /// ECS 适配层必须真的把其他玩家传给物理内核（不只是内核自己支持）：
    /// 两个玩家相邻，左边那个向右冲，一步后不得穿过右边那个（`06` §6.11）。
    #[test]
    fn ecs_adapter_blocks_players_against_each_other() {
        use bevy::ecs::system::RunSystemOnce;
        use level_format::{Grid, Level, Room, RoomCoord};

        let level = Level {
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
                goals: vec![],
                tiles: vec![".".repeat(level_format::ROOM_TILES); level_format::ROOM_TILES],
                entities: vec![],
            }],
        };
        let mut app = App::new();
        app.init_resource::<PlayerInputs>()
            .insert_resource(LevelTiles::from_level(&level));

        let mut left = Body::at_center(Vec2::new(100.0, 100.0));
        left.vel.x = crate::config::physics::MAX_RUN_SPEED;
        let right = Body::at_center(Vec2::new(121.0, 100.0));
        let left_entity = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(left),
                PlayerStepOutcome::default(),
            ))
            .id();
        let right_center = right.aabb.center();
        let right_entity = app
            .world_mut()
            .spawn((
                PlayerId(1),
                PlayerMotion(right),
                PlayerStepOutcome::default(),
            ))
            .id();
        app.world_mut().resource_mut::<PlayerInputs>().slots[0].move_x = 1;

        app.world_mut()
            .run_system_once(apply_player_physics)
            .expect("物理适配层应能运行");

        let left_aabb = app.world().get::<PlayerMotion>(left_entity).unwrap().0.aabb;
        let right_aabb = app
            .world()
            .get::<PlayerMotion>(right_entity)
            .unwrap()
            .0
            .aabb;
        assert_eq!(
            right_aabb.center().x,
            right_center.x,
            "静止玩家不该被水平推走"
        );
        assert!(
            left_aabb.max.x <= right_aabb.min.x + 1e-3,
            "左玩家必须被右玩家挡住：{left_aabb:?} vs {right_aabb:?}"
        );
        assert!(
            !app.world().resource::<PlayerInputs>().slots[0].jump_pressed,
            "物理步必须消费跳跃锁存"
        );
    }
}
