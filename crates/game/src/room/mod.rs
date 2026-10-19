//! 房间：当前房间判定、条件求值、通关与奖励。所有者文档：`docs/01-game-design.md` §1.4/§1.6、
//! `docs/02-world-and-rooms.md` §2.2/§2.3。
//!
//! 分工：
//! - [`goals`]：**纯函数**条件求值（`01` §1.4）。
//! - [`reward`]：`RoomCleared` → 幂等授予装饰品（`01` §1.5）。
//! - 本文件：房间判定（含滞回）、房间内实体的传感器、通关判定、全通检查。

pub mod goals;
pub mod reward;

use bevy::prelude::*;
use level_format::Room;

use crate::config::physics::PICKUP_INFLATE;
use crate::config::room::ROOM_SWITCH_HYSTERESIS;
use crate::coords::world_to_room;
use crate::physics::Aabb;
use crate::player::PlayerMotion;
use crate::state::{GameSet, GameState};
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;
use crate::world::progress::LevelProgress;

use goals::RoomRuntime;

// ---------- 消息（`05` §5.6）----------

/// 玩家进入了新房间（`02` §2.2）。
#[derive(Message, Debug, Clone, Copy)]
pub struct RoomEntered {
    pub room_index: u32,
}

/// 房间首次满足全部目标（单调，一个房间最多一次）。
#[derive(Message, Debug, Clone, Copy)]
pub struct RoomCleared {
    pub room_index: u32,
}

/// 装饰品被授予（幂等：已拥有的不会重复发）。
#[derive(Message, Debug, Clone)]
pub struct DecorationGranted {
    pub decoration_id: String,
}

/// 拾取了一颗果实（`06` §6.5.4）。
#[derive(Message, Debug, Clone, Copy)]
pub struct CollectiblePicked {
    pub room_index: u32,
}

// ---------- 房间内实体 ----------

/// 房间实体的种类（由 `world/load.rs` 生成）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomZoneKind {
    /// `exit` 区域：与玩家 AABB 相交即满足 `ReachExit`（`06` §6.5.5）。
    Exit,
    /// `fruit`：与玩家（外扩 `PICKUP_INFLATE`）相交即拾取（`06` §6.5.4）。
    Fruit,
    /// `checkpoint`：玩家在区域内按 `E` 设为存档点（`02` §2.5）。
    Checkpoint,
}

/// 关卡实体在**世界坐标**下的 AABB 与归属房间。
#[derive(Component, Debug, Clone, Copy)]
pub struct RoomZone {
    pub room_index: u32,
    pub kind: RoomZoneKind,
    pub aabb: Aabb,
    /// 房间局部坐标的 AABB 中心（与关卡文件 `at` 逐值相同；存档点用它记录位置）。
    pub at: (i32, i32),
}

/// 本固定步的出口探针结果（每步覆写；由 `check_pickups_and_zones` 写、`evaluate_goals` 读）。
///
/// 为什么用"每步覆写"而不是"进入即标记"：`ReachExit` 的语义是**玩家位于区域内**，
/// 与收集类条件必须**同时**成立（`01` §1.4），离开区域后不再满足。
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ExitProbe {
    pub room_index: u32,
    pub inside: bool,
}

// ---------- 当前房间判定 ----------

/// 房间坐标（`row = 0` = 地图最上一行）。
pub type RoomCoord = (u32, u32);

/// 判定是否切换房间（纯函数，便于单测）。见 `02` §2.2。
///
/// 返回 `Some(新房间)` 当且仅当玩家中心已进入与当前房间**不同**的房间，且深入距离
/// （到两房间共享那条边界的垂直距离）≥ `hysteresis`。
pub fn room_after_hysteresis(
    current: RoomCoord,
    center: Vec2,
    grid: (u32, u32),
    hysteresis: f32,
) -> Option<RoomCoord> {
    let pos = world_to_room(center, grid.0, grid.1)?;
    let target = (pos.col, pos.row);
    if target == current {
        return None;
    }
    let room_px = level_format::ROOM_PX as f32;
    // 进入方向决定"共享边界"在目标房间的哪一侧（坐标语义见 `00` §0.3）
    let depth_x = if pos.col > current.0 {
        pos.local.x
    } else {
        room_px - pos.local.x
    };
    let depth_y = if pos.row > current.1 {
        // row 增大 = 向下进入 ⇒ 共享边界是目标房间的上边
        pos.local.y
    } else {
        room_px - pos.local.y
    };
    let depth = match (pos.col != current.0, pos.row != current.1) {
        (true, false) => depth_x,
        (false, true) => depth_y,
        // 斜跨角落：取深入更多的那一轴
        (true, true) => depth_x.max(depth_y),
        (false, false) => return None,
    };
    (depth >= hysteresis).then_some(target)
}

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RoomEntered>()
            .add_message::<RoomCleared>()
            .add_message::<DecorationGranted>()
            .add_message::<CollectiblePicked>()
            .init_resource::<ExitProbe>()
            .add_systems(
                FixedUpdate,
                check_pickups_and_zones
                    .in_set(GameSet::Physics)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (track_current_room, emit_room_entered)
                    .chain()
                    .in_set(GameSet::Room)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    evaluate_goals,
                    reward::grant_rewards,
                    check_level_complete,
                )
                    .chain()
                    .in_set(GameSet::Room)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// 用玩家 AABB 中心判定当前房间（带滞回），只在真正切换时写资源。
pub fn track_current_room(
    loaded: Option<Res<LoadedLevel>>,
    player: Query<&PlayerMotion>,
    mut current: ResMut<CurrentRoom>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    let Some(motion) = player.iter().next() else {
        return;
    };
    let now = (current.col, current.row);
    let Some(next) = room_after_hysteresis(
        now,
        motion.0.aabb.center(),
        (loaded.level.grid.cols, loaded.level.grid.rows),
        ROOM_SWITCH_HYSTERESIS,
    ) else {
        return;
    };
    if next == now {
        return;
    }
    info!("当前房间：({}, {}) → ({}, {})", now.0, now.1, next.0, next.1);
    current.col = next.0;
    current.row = next.1;
}

/// 房间切换的那一帧发 `RoomEntered`（HUD/音效/调试用）。
fn emit_room_entered(
    loaded: Option<Res<LoadedLevel>>,
    current: Res<CurrentRoom>,
    mut entered: MessageWriter<RoomEntered>,
) {
    if !current.is_changed() {
        return;
    }
    let Some(loaded) = loaded else {
        return;
    };
    entered.write(RoomEntered {
        room_index: level_format::room_index(current.col, current.row, loaded.level.grid.cols) as u32,
    });
}

/// 玩家 AABB 是否与区域相交（严格重叠）。见 `06` §6.5.4 / §6.5.5。
pub fn aabb_overlaps(a: &Aabb, b: &Aabb) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// 房间内实体传感器：拾取果实、探测出口。只在**当前房间**内判定（`01` §1.4）。
pub fn check_pickups_and_zones(
    loaded: Option<Res<LoadedLevel>>,
    current: Option<Res<CurrentRoom>>,
    progress: Option<ResMut<LevelProgress>>,
    mut probe: ResMut<ExitProbe>,
    players: Query<&PlayerMotion>,
    zones: Query<(Entity, &RoomZone)>,
    mut commands: Commands,
    mut picked: MessageWriter<CollectiblePicked>,
) {
    let (Some(loaded), Some(current), Some(mut progress)) = (loaded, current, progress) else {
        return;
    };
    let Some(motion) = players.iter().next() else {
        return;
    };
    let room_index =
        level_format::room_index(current.col, current.row, loaded.level.grid.cols) as u32;
    if probe.room_index != room_index {
        probe.room_index = room_index;
        probe.inside = false;
    }
    let player = motion.0.aabb;

    for (entity, zone) in &zones {
        if zone.room_index != room_index {
            continue; // 离屏房间不判定
        }
        match zone.kind {
            RoomZoneKind::Exit => {
                if aabb_overlaps(&player, &zone.aabb) {
                    probe.inside = true;
                }
            }
            RoomZoneKind::Fruit => {
                // 拾取外扩 `PICKUP_INFLATE`：差一点也算吃到（`06` §6.5.4）
                if aabb_overlaps(&player.inset(-PICKUP_INFLATE), &zone.aabb) {
                    let total = progress.collect_fruit(current.col, current.row);
                    picked.write(CollectiblePicked { room_index });
                    commands.entity(entity).despawn();
                    info!(
                        "收集果实：房间 ({}, {}) 累计 {total}",
                        current.col, current.row
                    );
                }
            }
            // 存档点由 `checkpoint::activate_checkpoint` 按 `E` 激活（`02` §2.5）。
            RoomZoneKind::Checkpoint => {}
        }
    }
}

/// 房间内的果实总数（`CollectAll` 用）。
pub fn fruit_count(room: &Room) -> u32 {
    room.entities
        .iter()
        .filter(|entity| entity.kind == "fruit")
        .count() as u32
}

/// 当前房间的条件求值；满足且此前未通关 ⇒ 标记通关并发 `RoomCleared`（单调）。
pub fn evaluate_goals(
    loaded: Option<Res<LoadedLevel>>,
    current: Option<Res<CurrentRoom>>,
    probe: Res<ExitProbe>,
    progress: Option<ResMut<LevelProgress>>,
    mut cleared: MessageWriter<RoomCleared>,
) {
    let (Some(loaded), Some(current), Some(mut progress)) = (loaded, current, progress) else {
        return;
    };
    let room_index =
        level_format::room_index(current.col, current.row, loaded.level.grid.cols) as u32;
    // 探针还没覆盖本房间（刚切换）⇒ 本帧不判定，避免用上一房间的出口结果
    if probe.room_index != room_index {
        return;
    }
    let Some(room_data) = loaded.level.rooms.get(room_index as usize) else {
        return;
    };
    let Some(room_progress) = progress.room(current.col, current.row).copied() else {
        return;
    };
    if room_progress.cleared {
        return;
    }
    let runtime = RoomRuntime {
        at_exit: probe.inside,
        collected_fruit: room_progress.collected_fruit,
        total_fruit: fruit_count(room_data),
    };
    if !goals::goals_satisfied(&room_data.goals, &runtime) {
        return;
    }
    progress.mark_cleared(current.col, current.row);
    info!(
        "房间 ({}, {}) 通关（{}/{}）",
        current.col,
        current.row,
        progress.cleared_count(),
        progress.rooms.len()
    );
    cleared.write(RoomCleared { room_index });
}

/// 全部房间通关 ⇒ 进入结算（`01` §1.6）。
pub fn check_level_complete(
    progress: Option<Res<LevelProgress>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(progress) = progress else {
        return;
    };
    if progress.all_cleared() {
        info!(
            "全部房间通关（{} 个），进入结算",
            progress.rooms.len()
        );
        next.set(GameState::GameComplete);
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;
    use level_format::ROOM_PX;

    use super::*;
    use crate::coords::room_center;

    const GRID: (u32, u32) = (10, 3);

    fn runtime(at_exit: bool, collected: u32, total: u32) -> RoomRuntime {
        RoomRuntime {
            at_exit,
            collected_fruit: collected,
            total_fruit: total,
        }
    }

    /// 房间内任意位置都不触发切换。
    #[test]
    fn stays_inside_current_room() {
        let c = room_center(1, 1, GRID.1);
        assert_eq!(
            room_after_hysteresis((1, 1), c, GRID, ROOM_SWITCH_HYSTERESIS),
            None
        );
        let near_edge = Vec2::new(c.x + ROOM_PX as f32 / 2.0 - 0.5, c.y);
        assert_eq!(
            room_after_hysteresis((1, 1), near_edge, GRID, ROOM_SWITCH_HYSTERESIS),
            None
        );
    }

    /// 滞回边界：深入 < 8 px 不切换，≥ 8 px 才切换。
    #[test]
    fn hysteresis_boundary() {
        let center = room_center(1, 1, GRID.1);
        let half = ROOM_PX as f32 / 2.0;
        let boundary = center.x + half;
        for (depth, expect) in [
            (0.0, None),
            (1.0, None),
            (7.9, None),
            (8.0, Some((2, 1))),
            (20.0, Some((2, 1))),
        ] {
            let point = Vec2::new(boundary + depth, center.y);
            assert_eq!(
                room_after_hysteresis((1, 1), point, GRID, ROOM_SWITCH_HYSTERESIS),
                expect,
                "深入 {depth} px"
            );
        }
    }

    /// 四个方向都要能切换（上下左右），且切换后不会立刻切回。
    #[test]
    fn switches_in_all_four_directions() {
        let center = room_center(1, 1, GRID.1);
        let half = ROOM_PX as f32 / 2.0;
        let cases = [
            (Vec2::new(center.x + half + 10.0, center.y), (2, 1)),
            (Vec2::new(center.x - half - 10.0, center.y), (0, 1)),
            (Vec2::new(center.x, center.y + half + 10.0), (1, 0)),
            (Vec2::new(center.x, center.y - half - 10.0), (1, 2)),
        ];
        for (point, expect) in cases {
            assert_eq!(
                room_after_hysteresis((1, 1), point, GRID, ROOM_SWITCH_HYSTERESIS),
                Some(expect),
                "{point:?}"
            );
            assert_eq!(
                room_after_hysteresis(expect, point, GRID, ROOM_SWITCH_HYSTERESIS),
                None
            );
        }
    }

    #[test]
    fn outside_map_never_switches() {
        assert_eq!(
            room_after_hysteresis((0, 0), Vec2::new(-50.0, 50.0), GRID, ROOM_SWITCH_HYSTERESIS),
            None
        );
    }

    /// 区域相交是严格重叠：贴边不算。
    #[test]
    fn overlap_is_strict() {
        let a = Aabb::from_center_size(Vec2::new(0.0, 0.0), Vec2::splat(10.0));
        let touching = Aabb::from_center_size(Vec2::new(10.0, 0.0), Vec2::splat(10.0));
        let overlapping = Aabb::from_center_size(Vec2::new(9.0, 0.0), Vec2::splat(10.0));
        assert!(!aabb_overlaps(&a, &touching));
        assert!(aabb_overlaps(&a, &overlapping));
    }

    /// M4 全链路（无头、确定性）：拾取 → 通关 → 授奖 → 全通结算。
    ///
    /// 不用 `--demo` 验证这一段：脚本输入无法可靠地"跳过错落在地面的出口"或"从平台右缘
    /// 落进尖刺不被杀"，端到端必须靠确定性的系统级测试。
    #[test]
    fn m4_pipeline_pickup_clear_reward_complete() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::state::app::StatesPlugin;
        use level_format::{EntitySpec, Goal, Grid, Room, RoomCoord};

        use crate::config::physics::{PICKUP_SIZE, PLAYER_SIZE};
        use crate::player::decor::Decorations;
        use crate::physics::Body;
        use crate::player::PlayerStepOutcome;
        use crate::tiles::LevelTiles;
        use crate::world::progress::LevelProgress;

        fn spec(kind: &str, at: (i32, i32)) -> EntitySpec {
            EntitySpec {
                kind: kind.to_string(),
                at,
                size: None,
                id: None,
            }
        }

        // 1 个房间：ReachExit + Collect(1)，奖励 item_01；1 个果实 + 1 个出口
        let level = level_format::Level {
            version: level_format::VERSION,
            name: "m4".to_string(),
            grid: Grid { cols: 1, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![Room {
                name: None,
                reward: Some("item_01".to_string()),
                spawn: None,
                goals: vec![Goal::ReachExit, Goal::Collect { count: 1 }],
                tiles: vec!["................".to_string(); 16],
                entities: vec![spec("exit", (448, 464)), spec("fruit", (200, 464))],
            }],
        };
        let rows = level.grid.rows;
        let world_of = |at: (i32, i32)| {
            crate::coords::room_local_to_world(0, 0, rows, Vec2::new(at.0 as f32, at.1 as f32))
        };

        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .add_message::<CollectiblePicked>()
            .add_message::<RoomCleared>()
            .add_message::<DecorationGranted>()
            .init_resource::<ExitProbe>()
            .init_resource::<Decorations>()
            .init_state::<GameState>();
        app.insert_resource(LoadedLevel {
            level: level.clone(),
            handle: Handle::default(),
        })
        .insert_resource(LevelTiles::from_level(&level))
        .insert_resource(LevelProgress::from_level(&level))
        .insert_resource(CurrentRoom { col: 0, row: 0 });

        // ---- 1) 玩家站在果实上 ⇒ 拾取 ----
        let fruit_center = world_of((200, 464));
        let player = app
            .world_mut()
            .spawn((
                PlayerMotion(Body::at_center(fruit_center)),
                PlayerStepOutcome::default(),
            ))
            .id();
        let fruit = app
            .world_mut()
            .spawn(RoomZone {
                room_index: 0,
                kind: RoomZoneKind::Fruit,
                aabb: Aabb::from_center_size(fruit_center, Vec2::splat(PICKUP_SIZE)),
                at: (200, 464),
            })
            .id();
        app.world_mut()
            .run_system_once(check_pickups_and_zones)
            .expect("传感器");
        assert_eq!(
            app.world()
                .resource::<LevelProgress>()
                .room(0, 0)
                .unwrap()
                .collected_fruit,
            1,
            "果实应计入进度"
        );
        assert!(
            app.world().get_entity(fruit).is_err(),
            "拾取后实体应被销毁（幂等）"
        );

        // ---- 2) 玩家走到出口（条件此时才同时成立）⇒ 通关 ----
        let exit_center = world_of((448, 464));
        app.world_mut().spawn(RoomZone {
            room_index: 0,
            kind: RoomZoneKind::Exit,
            aabb: Aabb::from_center_size(exit_center, Vec2::splat(32.0)),
            at: (448, 464),
        });
        app.world_mut()
            .get_mut::<PlayerMotion>(player)
            .unwrap()
            .0
            .aabb = Aabb::from_center_size(exit_center, PLAYER_SIZE);
        app.world_mut()
            .run_system_once(check_pickups_and_zones)
            .expect("传感器");
        app.world_mut()
            .run_system_once(evaluate_goals)
            .expect("条件求值");
        assert!(
            app.world()
                .resource::<LevelProgress>()
                .room(0, 0)
                .unwrap()
                .cleared,
            "两个条件同时满足 ⇒ 房间通关"
        );

        // ---- 3) 授奖（幂等）----
        app.world_mut()
            .run_system_once(reward::grant_rewards)
            .expect("授奖");
        assert!(
            app.world()
                .resource::<Decorations>()
                .has("item_01"),
            "通关应授予房间奖励"
        );

        // ---- 4) 全部房间通关 ⇒ 进入结算 ----
        app.world_mut()
            .run_system_once(check_level_complete)
            .expect("全通检查");
        match app.world().resource::<NextState<GameState>>() {
            NextState::Pending(GameState::GameComplete) => {}
            other => panic!("全部房间 cleared 应进入结算，实际 {other:?}"),
        }
    }

    /// 组合条件：必须先收够果实、再站进出口区域（`01` §1.4）。
    #[test]
    fn combined_goals_require_both_at_the_same_time() {
        let goals = [level_format::Goal::ReachExit, level_format::Goal::Collect { count: 2 }];
        assert!(!goals::goals_satisfied(&goals, &runtime(false, 3, 3)));
        assert!(goals::goals_satisfied(&goals, &runtime(true, 2, 3)));
    }
}
