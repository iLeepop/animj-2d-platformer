//! Hazard 死亡与重生。所有者文档：`docs/06-physics-and-movement.md` §6.5.3、`docs/02-world-and-rooms.md` §2.5。
//!
//! 物理侧只负责**发出信号**并提供一个干净的 `Body`；重置行为的规范在 `02` §2.5：
//! 回到已激活的存档点；没有存档点时回到关卡出生点（`player_spawn` 所在房间）。

use bevy::color::Alpha;
use bevy::prelude::*;

use super::{PlayerId, PlayerInputs, PlayerMotion, PlayerStepOutcome};
use crate::checkpoint::{ActiveCheckpoint, CheckpointPoint};
use crate::config::physics::{FIXED_DT, HAZARD_INSET};
use crate::config::render::player_tint;
use crate::config::room::{DEATH_BLINK_SECS, DEATH_DELAY_STEPS, DEATH_FLASH_ALPHA};
use crate::physics::{Body, StepOutcome, TileKind, overlaps_tile_kind};
use crate::state::{GameSet, GameState};
use crate::tiles::LevelTiles;
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;

/// 角色死亡（buffered 消息，命名与载荷见 `05` §5.6）。
///
/// 在**死亡开始**时发一次（用于音效等一次性反馈）；位置重置由 [`Dying`] 倒计时驱动。
/// 多人时只影响死亡的玩家（`01` §1.6）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayerDied {
    pub player_id: PlayerId,
}

/// 死亡状态：冻结在死亡点、播放闪烁，倒计时到期后重生（`02` §2.5）。
///
/// `remaining` 是**主机权威**的剩余固定步；客户端从快照 `dead` 标记挂上它时只用于表现，
/// 不推进该字段。`elapsed` 是表现层时钟（`Update` 推进），让主机与客户端都能播完闪烁。
#[derive(Component, Debug, Clone, Copy)]
pub struct Dying {
    pub remaining: u32,
    pub elapsed: f32,
}

impl Dying {
    /// 主机：新死亡，开始 [`DEATH_DELAY_STEPS`] 个固定步的倒计时。
    pub fn new() -> Self {
        Self {
            remaining: DEATH_DELAY_STEPS,
            elapsed: 0.0,
        }
    }

    /// 客户端：仅表现；权威倒计时在主机，客户端靠快照移除它。
    pub fn visual() -> Self {
        Self {
            remaining: 0,
            elapsed: 0.0,
        }
    }
}

impl Default for Dying {
    fn default() -> Self {
        Self::new()
    }
}

/// 死亡闪烁 alpha（纯函数）：[`DEATH_BLINK_SECS`] 为半周期，交替满显与 [`DEATH_FLASH_ALPHA`]。
pub fn death_flash_alpha(elapsed: f32) -> f32 {
    let phase = (elapsed / DEATH_BLINK_SECS).floor().max(0.0) as u32;
    if phase.is_multiple_of(2) {
        1.0
    } else {
        DEATH_FLASH_ALPHA
    }
}

pub struct PlayerDeathPlugin;

impl Plugin for PlayerDeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayerDied>()
            .add_systems(
                FixedUpdate,
                (check_hazards, advance_deaths)
                    .chain()
                    .after(crate::player::apply_player_physics)
                    .in_set(GameSet::Physics)
                    .run_if(crate::net::not_client),
            )
            .add_systems(
                Update,
                (tick_death_visual, apply_death_flash)
                    .chain()
                    .in_set(GameSet::Fx)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// Hazard 传感器：角色 AABB **内缩** `HAZARD_INSET` 后与尖刺相交即死亡（`06` §6.5.3）。
///
/// 死亡中的玩家被冻结在死亡点，必须排除，否则每个固定步都会重复发消息（音效连播、倒计时重置）。
pub fn check_hazards(
    tiles: Option<Res<LevelTiles>>,
    players: Query<(&PlayerId, &PlayerMotion), Without<Dying>>,
    mut died: MessageWriter<PlayerDied>,
) {
    let Some(tiles) = tiles else {
        return;
    };
    for (id, motion) in &players {
        if !overlaps_tile_kind(
            tiles.as_ref(),
            motion.0.aabb,
            -HAZARD_INSET,
            TileKind::Hazard,
        ) {
            continue;
        }
        died.write(PlayerDied { player_id: *id });
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
    player_height: f32,
) -> RespawnPoint {
    if let Some(point) = checkpoint.filter(|point| crate::checkpoint::exists(level, *point)) {
        return RespawnPoint {
            center: point.world_center(level.grid.rows),
            room: (point.col, point.row),
            from_checkpoint: true,
        };
    }
    let (col, row, center) = crate::world::initial_spawn(level, player_height);
    RespawnPoint {
        center,
        room: (col, row),
        from_checkpoint: false,
    }
}

/// 死亡开始 + 倒计时 + 到期重生（`02` §2.5）。
///
/// 收到 `PlayerDied` 时**不移动**角色：挂 [`Dying`]（冻结在死亡点）、清道具 buff、清输入锁存；
/// 每固定步递减，归零后才把角色放回存档点/出生点。物理与 Hazard 传感器都排除 [`Dying`]，
/// 因此死亡期间不会重复触发，也不会继续下坠。
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn advance_deaths(
    mut commands: Commands,
    mut died: MessageReader<PlayerDied>,
    loaded: Option<Res<LoadedLevel>>,
    mut current: Option<ResMut<CurrentRoom>>,
    active: Option<Res<ActiveCheckpoint>>,
    tiles: Option<Res<LevelTiles>>,
    collision: Res<crate::player::collision::PlayerCollision>,
    mut inputs: ResMut<PlayerInputs>,
    mut players: Query<(
        Entity,
        &PlayerId,
        &mut PlayerMotion,
        &mut PlayerStepOutcome,
        &mut crate::player::items::CarriedItem,
        Option<&mut Dying>,
    )>,
) {
    // 没有关卡/房间资源时不消费消息：留给加载完成后的固定步。
    let (Some(loaded), Some(current)) = (loaded, current.as_mut()) else {
        return;
    };
    let started: Vec<PlayerId> = died.read().map(|message| message.player_id).collect();
    if started.is_empty() && players.iter().all(|(_, _, _, _, _, dying)| dying.is_none()) {
        return;
    }
    let checkpoint = active.and_then(|active| active.0);
    // 多人同时死亡时按 id 分列，避免两个盒子完全重叠（`02` §2.5）。
    let count = players
        .iter()
        .map(|(_, id, ..)| id.index())
        .max()
        .unwrap_or(0)
        + 1;

    for (entity, id, mut motion, mut outcome, mut item, dying) in &mut players {
        if started.contains(id) {
            if dying.is_none() {
                commands.entity(entity).insert(Dying::new());
                // 死亡期间物理步不消费锁存；死亡瞬间与重生时各清一次，防止按键带进复活。
                let slot = inputs.get_mut(*id);
                slot.jump_pressed = false;
                slot.interact_pressed = false;
                // 死亡清除道具 buff（`01` §1.5）。
                if item.kind.is_some() {
                    info!("玩家 {} 死亡：道具 buff 已清除", id.0);
                    item.clear();
                }
                info!(
                    "玩家 {} 死亡：{:.2} 秒后重生",
                    id.0,
                    DEATH_DELAY_STEPS as f32 * FIXED_DT
                );
            }
            continue;
        }

        let Some(mut dying) = dying else {
            continue;
        };
        if dying.remaining > 0 {
            dying.remaining -= 1;
        }
        if dying.remaining > 0 {
            continue;
        }

        let target = respawn_target(&loaded.level, checkpoint, collision.size.1);
        let center = tiles
            .as_ref()
            .and_then(|tiles| {
                crate::player::spawn::player_spawn_centers(
                    target.center,
                    count,
                    tiles.as_ref(),
                    collision.size_vec(),
                )
                .get(id.index())
                .copied()
            })
            .unwrap_or(target.center);
        // `Body::at_center_with` 保证「土狼时间用尽」，因此重生后不得凭空起跳（`06` §6.5.3 / T21）
        motion.0 = Body::at_center_with(center, collision.size_vec());
        outcome.0 = StepOutcome::default();
        let slot = inputs.get_mut(*id);
        slot.jump_pressed = false;
        slot.interact_pressed = false;
        commands.entity(entity).remove::<Dying>();

        if target.room != (current.col, current.row) {
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
}

/// 推进死亡表现时钟（`Update`，主机与客户端共用；暂停时随 [`GameSet::Fx`] 冻结）。
fn tick_death_visual(time: Res<Time>, mut dying: Query<&mut Dying>) {
    for mut dying in &mut dying {
        dying.elapsed += time.delta_secs();
    }
}

/// 死亡闪烁：在**玩家分色**基础上交替 alpha（`02` §2.5）。
///
/// 不在死亡中的玩家写回基准色（重生后恢复）；只在颜色变化时写组件，避免每帧触发变更检测。
fn apply_death_flash(mut players: Query<(&PlayerId, &mut Sprite, Option<&Dying>)>) {
    for (id, mut sprite, dying) in &mut players {
        let alpha = dying.map_or(1.0, |dying| death_flash_alpha(dying.elapsed));
        let color = player_tint(id.0).with_alpha(alpha);
        if sprite.color != color {
            sprite.color = color;
        }
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
                    item: None,
                    sprite: None,
                    sprite_cell: None,
                });
            }
            if let Some(at) = checkpoint_at {
                entities.push(level_format::EntitySpec {
                    kind: crate::checkpoint::KIND.to_string(),
                    at,
                    size: None,
                    id: None,
                    item: None,
                    sprite: None,
                    sprite_cell: None,
                });
            }
            Room {
                name: None,
                tiles: tiles.clone(),
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
            items: Vec::new(),
            rooms: vec![room(true, checkpoint_at), room(false, None)],
        }
    }

    fn level_spawn(level: &level_format::Level) -> (u32, u32, Vec2) {
        crate::world::initial_spawn(level, crate::config::physics::PLAYER_SIZE.y)
    }

    /// 没有存档点 ⇒ 回**关卡出生点**，而不是死亡时所在房间的重生点（`02` §2.5）。
    #[test]
    fn no_checkpoint_falls_back_to_level_spawn_in_any_room() {
        let level = level_with_checkpoint(None);
        let (col, row, center) = level_spawn(&level);
        let target = respawn_target(&level, None, crate::config::physics::PLAYER_SIZE.y);
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
        let target = respawn_target(&level, Some(point), crate::config::physics::PLAYER_SIZE.y);
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
        let target = respawn_target(&level, Some(point), crate::config::physics::PLAYER_SIZE.y);
        assert!(!target.from_checkpoint);
        assert_eq!(target.room, (col, row));
        assert_eq!(target.center, center);
    }

    /// 端到端：死亡后**冻结在死亡点**，倒计时到期才回关卡出生点（`02` §2.5）。
    #[test]
    fn death_freezes_until_the_delay_then_returns_to_spawn() {
        use crate::physics::Body;

        let level = level_with_checkpoint(None);
        let (spawn_col, spawn_row, spawn_center) = level_spawn(&level);
        let mut app = App::new();
        app.add_message::<PlayerDied>()
            .init_resource::<PlayerInputs>()
            .init_resource::<ActiveCheckpoint>()
            .insert_resource(crate::player::collision::PlayerCollision::default())
            .insert_resource(LoadedLevel {
                level: level.clone(),
                handle: Handle::default(),
            })
            .insert_resource(CurrentRoom { col: 1, row: 0 })
            .add_systems(Update, advance_deaths);
        let player = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(Body::at_center(Vec2::new(700.0, 40.0))),
                PlayerStepOutcome::default(),
                crate::player::items::CarriedItem {
                    kind: Some(level_format::ItemKind::Speed),
                    remaining: 8.0,
                },
            ))
            .id();
        app.world_mut().resource_mut::<PlayerInputs>().slots[0].jump_pressed = true;
        app.world_mut().write_message(PlayerDied {
            player_id: PlayerId(0),
        });

        app.update(); // 死亡开始：只挂 Dying，不移动
        let death_center = app
            .world()
            .get::<PlayerMotion>(player)
            .unwrap()
            .0
            .aabb
            .center();
        assert_eq!(death_center, Vec2::new(700.0, 40.0), "死亡当步不得瞬移");
        let dying = *app
            .world()
            .get::<Dying>(player)
            .expect("死亡中必须挂 Dying");
        assert_eq!(dying.remaining, DEATH_DELAY_STEPS);
        assert_eq!(
            app.world()
                .get::<crate::player::items::CarriedItem>(player)
                .unwrap()
                .kind,
            None,
            "死亡瞬间清除道具 buff"
        );
        assert!(
            !app.world().resource::<PlayerInputs>().slots[0].jump_pressed,
            "死亡瞬间清除输入锁存，避免复活自动起跳"
        );

        for _ in 0..DEATH_DELAY_STEPS - 1 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<PlayerMotion>(player)
                .unwrap()
                .0
                .aabb
                .center(),
            death_center,
            "倒计时未结束时必须停在死亡点"
        );
        assert_eq!(
            app.world().get::<Dying>(player).unwrap().remaining,
            1,
            "到期前最后一固定步仍应剩 1"
        );

        app.update(); // 到期
        assert!(
            app.world().get::<Dying>(player).is_none(),
            "重生后移除 Dying"
        );
        let motion = &app.world().get::<PlayerMotion>(player).unwrap().0;
        assert_eq!(motion.aabb.center(), spawn_center);
        assert!(!motion.grounded, "重生后不得凭空 grounded");
        let current = app.world().resource::<CurrentRoom>();
        assert_eq!(
            (current.col, current.row),
            (spawn_col, spawn_row),
            "CurrentRoom 必须同步切回出生房间"
        );
    }

    #[derive(Resource, Default)]
    struct DiedCount(u32);

    fn count_died(mut count: ResMut<DiedCount>, mut died: MessageReader<PlayerDied>) {
        count.0 += died.read().count() as u32;
    }

    /// 死亡中的玩家仍停在 Hazard 上：传感器必须排除 `Dying`，否则每步重发消息/音效连播。
    #[test]
    fn dying_players_are_not_sensed_again() {
        let mut level = level_with_checkpoint(None);
        let mut tiles = vec![".".repeat(level_format::ROOM_TILES); level_format::ROOM_TILES];
        tiles[level_format::ROOM_TILES - 1] = "#".repeat(level_format::ROOM_TILES);
        let mut spike_row = ".".repeat(level_format::ROOM_TILES);
        spike_row.replace_range(3..4, "^");
        tiles[level_format::ROOM_TILES - 2] = spike_row;
        level.rooms[0].tiles = tiles;

        let center =
            crate::coords::room_local_to_world(0, 0, level.grid.rows, Vec2::new(112.0, 460.0));
        let mut app = App::new();
        app.add_message::<PlayerDied>()
            .init_resource::<DiedCount>()
            .insert_resource(LevelTiles::from_level(&level))
            .add_systems(Update, (check_hazards, count_died).chain());
        let player = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(crate::physics::Body::at_center(center)),
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().resource::<DiedCount>().0,
            1,
            "第一次相交必须触发"
        );
        app.world_mut().entity_mut(player).insert(Dying::new());
        app.update();
        assert_eq!(
            app.world().resource::<DiedCount>().0,
            1,
            "死亡中不得重复触发 Hazard"
        );
    }

    /// 闪烁 alpha 由纯函数给出：只在两档之间交替，且不越界。
    #[test]
    fn death_flash_alpha_blinks_between_two_levels() {
        assert_eq!(death_flash_alpha(0.0), 1.0);
        assert_eq!(death_flash_alpha(DEATH_BLINK_SECS * 0.5), 1.0);
        assert_eq!(death_flash_alpha(DEATH_BLINK_SECS), DEATH_FLASH_ALPHA);
        assert_eq!(death_flash_alpha(DEATH_BLINK_SECS * 2.5), 1.0);
        for step in 0..=60 {
            let alpha = death_flash_alpha(step as f32 * 0.01);
            assert!(
                (DEATH_FLASH_ALPHA..=1.0).contains(&alpha),
                "alpha 越界：{alpha}"
            );
        }
    }

    /// 闪烁必须保留玩家分色，并在死亡结束后恢复基准色。
    #[test]
    fn death_flash_keeps_player_tint_and_restores_it() {
        let mut app = App::new();
        app.add_systems(Update, apply_death_flash);
        let player = app
            .world_mut()
            .spawn((
                PlayerId(1),
                Sprite::from_color(player_tint(1), Vec2::splat(32.0)),
                Dying {
                    remaining: 0,
                    elapsed: DEATH_BLINK_SECS * 1.5,
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(player).unwrap().color.alpha(),
            DEATH_FLASH_ALPHA
        );
        app.world_mut().entity_mut(player).remove::<Dying>();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(player).unwrap().color,
            player_tint(1),
            "重生后必须恢复玩家分色"
        );
    }
}
