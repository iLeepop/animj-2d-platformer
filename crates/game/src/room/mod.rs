//! 房间：当前房间判定与房间内实体的传感器。所有者文档：`docs/02-world-and-rooms.md` §2.2/§2.4、
//! `docs/01-game-design.md` §1.4（终点）/§1.5（道具）。
//!
//! 房间**不再承载通关目标**：终点（`goal`）是全关卡唯一的实体，触碰即由 [`crate::run`] 记录成绩。
//! 本模块只保留：房间切换（含滞回）、果实拾取、房间实体标识。

use std::collections::BTreeSet;

use bevy::prelude::*;
use level_format::ItemKind;

use crate::config::physics::PICKUP_INFLATE;
use crate::config::room::ROOM_SWITCH_HYSTERESIS;
use crate::coords::world_to_room;
use crate::physics::Aabb;
use crate::player::{PlayerId, PlayerMotion};
use crate::state::{GameSet, GameState};
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;
use crate::world::progress::LevelProgress;

// ---------- 消息（`05` §5.6）----------

/// 玩家进入了新房间（`02` §2.2）。
#[derive(Message, Debug, Clone, Copy)]
pub struct RoomEntered {
    pub room_index: u32,
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
    /// `goal` 区域：与玩家 AABB 相交即该玩家到达终点（`01` §1.4）。
    Goal,
    /// `item`：拾取后覆盖角色当前道具（`01` §1.5、`06` §6.12）。
    Item(ItemKind),
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

/// 房间实体的稳定编号（生成顺序）。联机快照用它告诉客户端「哪些实体已被拾取」
/// （`05` §5.15.3）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ZoneId(pub u32);

/// 已被拾取并销毁的房间实体编号。客户端据此销毁对应实体（`05` §5.15.3）。
#[derive(Resource, Debug, Default, Clone)]
pub struct PickedZones(pub BTreeSet<u32>);

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
            .add_message::<CollectiblePicked>()
            .init_resource::<PickedZones>()
            .add_systems(
                FixedUpdate,
                collect_fruit
                    .in_set(GameSet::Physics)
                    .run_if(in_state(GameState::Playing))
                    .run_if(crate::net::not_client),
            )
            .add_systems(
                Update,
                (track_current_room, emit_room_entered)
                    .chain()
                    .in_set(GameSet::Room)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// 用玩家 AABB 中心判定当前房间（带滞回），只在真正切换时写资源。
pub fn track_current_room(
    loaded: Option<Res<LoadedLevel>>,
    client: Option<Res<crate::net::client::ClientNet>>,
    players: Query<(&PlayerId, &PlayerMotion)>,
    mut current: ResMut<CurrentRoom>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    // 焦点玩家 = 客户端自己的编号；其它模式 = 编号最小者（主机/单人/P1）。
    let focus = client
        .as_ref()
        .and_then(|client| client.player_id)
        .map(PlayerId)
        .unwrap_or(PlayerId(0));
    let motion = players
        .iter()
        .find(|(id, _)| **id == focus)
        .or_else(|| players.iter().min_by_key(|(id, _)| *id))
        .map(|(_, motion)| motion);
    let Some(motion) = motion else {
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
    info!(
        "当前房间：({}, {}) → ({}, {})",
        now.0, now.1, next.0, next.1
    );
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
        room_index: level_format::room_index(current.col, current.row, loaded.level.grid.cols)
            as u32,
    });
}

/// 玩家 AABB 是否与区域相交（严格重叠）。见 `06` §6.5.4 / §6.5.5。
pub fn aabb_overlaps(a: &Aabb, b: &Aabb) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// 房间内实体传感器：拾取果实。道具由 `player::items`、终点由 `run` 各自处理。
pub fn collect_fruit(
    loaded: Option<Res<LoadedLevel>>,
    progress: Option<ResMut<LevelProgress>>,
    mut picked_zones: ResMut<PickedZones>,
    players: Query<&PlayerMotion>,
    zones: Query<(Entity, &RoomZone, Option<&ZoneId>)>,
    mut commands: Commands,
    mut picked: MessageWriter<CollectiblePicked>,
) {
    let (Some(loaded), Some(mut progress)) = (loaded, progress) else {
        return;
    };
    let grid = (loaded.level.grid.cols, loaded.level.grid.rows);
    let players: Vec<((u32, u32), Aabb)> = players
        .iter()
        .filter_map(|motion| {
            let pos = world_to_room(motion.0.aabb.center(), grid.0, grid.1)?;
            Some(((pos.col, pos.row), motion.0.aabb))
        })
        .collect();

    for (entity, zone, zone_id) in &zones {
        if zone.kind != RoomZoneKind::Fruit {
            continue;
        }
        let Some((col, row)) = players
            .iter()
            .find(|(_, aabb)| aabb_overlaps(&aabb.inset(-PICKUP_INFLATE), &zone.aabb))
            .map(|(coord, _)| *coord)
        else {
            continue;
        };
        let total = progress.collect_fruit(col, row);
        if let Some(zone_id) = zone_id {
            picked_zones.0.insert(zone_id.0);
        }
        picked.write(CollectiblePicked {
            room_index: zone.room_index,
        });
        commands.entity(entity).despawn();
        info!("收集果实：房间 ({}, {}) 累计 {total}", col, row);
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;
    use level_format::ROOM_PX;

    use super::*;
    use crate::config::room::ROOM_SWITCH_HYSTERESIS;
    use crate::coords::room_center;

    const GRID: (u32, u32) = (10, 3);

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
}
