//! 道具：拾取、覆盖与能力修正。所有者文档：`docs/01-game-design.md` §1.4、
//! `docs/06-physics-and-movement.md` §6.12、`docs/03-level-format.md` §3.4。
//!
//! 规则：**每个角色同时只携带一个道具**；再次拾取直接覆盖（不叠加、不排队）。
//! 道具实体拾取后销毁；死亡不清空道具（避免一次性道具被消耗后卡死关卡）。

use bevy::prelude::*;
use level_format::ItemKind;

use crate::config::physics::{
    ITEM_DOUBLE_JUMP_MAX_AIR_JUMPS, ITEM_DOUBLE_JUMP_VELOCITY_MULT, ITEM_SPEED_ACCEL_MULT,
    ITEM_SPEED_MAX_MULT, ITEM_SUPER_JUMP_VELOCITY_MULT, PICKUP_INFLATE, PhysicsConfig,
};
use crate::player::{PlayerId, PlayerMotion};
use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId, aabb_overlaps};

/// 角色当前携带的道具（`None` = 空手）。同一角色至多一个。
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CarriedItem(pub Option<ItemKind>);

impl CarriedItem {
    pub fn get(self) -> Option<ItemKind> {
        self.0
    }
}

/// 拾取了一个道具（HUD 提示 / 音效用）。
#[derive(Message, Debug, Clone, Copy)]
pub struct ItemPicked {
    pub player: PlayerId,
    pub kind: ItemKind,
    /// 被覆盖的旧道具（`None` = 之前空手）。
    pub replaced: Option<ItemKind>,
}

/// 道具的占位色块颜色（无美术资产阶段；`04` 的资产策略）。
pub fn item_color(kind: ItemKind) -> Color {
    match kind {
        ItemKind::SuperJump => Color::srgb(0.98, 0.55, 0.20),
        ItemKind::Speed => Color::srgb(0.35, 0.85, 0.95),
        ItemKind::DoubleJump => Color::srgb(0.85, 0.40, 0.95),
    }
}

/// 把道具效果施加到物理参数上（纯函数，可单测）。见 `06` §6.12。
///
/// 只改**本玩家本固定步**的 `cfg` 副本，不改全局常量。
pub fn apply_item_to_physics(cfg: &mut PhysicsConfig, item: Option<ItemKind>) {
    match item {
        None => {}
        Some(ItemKind::SuperJump) => {
            cfg.jump_velocity *= ITEM_SUPER_JUMP_VELOCITY_MULT;
        }
        Some(ItemKind::Speed) => {
            cfg.max_run_speed *= ITEM_SPEED_MAX_MULT;
            cfg.ground_accel *= ITEM_SPEED_ACCEL_MULT;
            cfg.ground_friction *= ITEM_SPEED_ACCEL_MULT;
            cfg.air_accel *= ITEM_SPEED_ACCEL_MULT;
            cfg.air_drag *= ITEM_SPEED_ACCEL_MULT;
        }
        Some(ItemKind::DoubleJump) => {
            cfg.max_air_jumps = ITEM_DOUBLE_JUMP_MAX_AIR_JUMPS;
            cfg.air_jump_velocity = cfg.jump_velocity * ITEM_DOUBLE_JUMP_VELOCITY_MULT;
        }
    }
}

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ItemPicked>().add_systems(
            FixedUpdate,
            pickup_items
                .after(crate::player::apply_player_physics)
                .in_set(crate::state::GameSet::Physics)
                .run_if(crate::net::not_client),
        );
    }
}

/// 玩家 AABB 与道具区域相交 ⇒ 拾取并覆盖当前道具，销毁道具实体。
///
/// 每个道具每个固定步只被一名玩家拾取（`find` 的第一名）；多人同时踩住不会复制。
fn pickup_items(
    mut commands: Commands,
    mut players: Query<(&PlayerId, &PlayerMotion, &mut CarriedItem)>,
    zones: Query<(Entity, &RoomZone, Option<&ZoneId>)>,
    mut picked_zones: ResMut<PickedZones>,
    mut picked: MessageWriter<ItemPicked>,
) {
    for (entity, zone, zone_id) in &zones {
        let RoomZoneKind::Item(kind) = zone.kind else {
            continue;
        };
        let Some((id, _, mut carried)) = players.iter_mut().find(|(_, motion, _)| {
            aabb_overlaps(&motion.0.aabb.inset(-PICKUP_INFLATE), &zone.aabb)
        }) else {
            continue;
        };
        let replaced = carried.0;
        carried.0 = Some(kind);
        if let Some(zone_id) = zone_id {
            picked_zones.0.insert(zone_id.0);
        }
        commands.entity(entity).despawn();
        picked.write(ItemPicked {
            player: *id,
            kind,
            replaced,
        });
        info!(
            "玩家 {} 拾取道具：{}（{}）",
            id.0,
            kind.id(),
            match replaced {
                Some(previous) => format!("覆盖 {}", previous.id()),
                None => "空手".to_string(),
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::physics::DEFAULT_PHYSICS;

    /// 空手不改变任何参数。
    #[test]
    fn empty_hand_changes_nothing() {
        let mut cfg = DEFAULT_PHYSICS;
        apply_item_to_physics(&mut cfg, None);
        assert_eq!(cfg, DEFAULT_PHYSICS);
    }

    #[test]
    fn super_jump_only_raises_jump_velocity() {
        let mut cfg = DEFAULT_PHYSICS;
        apply_item_to_physics(&mut cfg, Some(ItemKind::SuperJump));
        assert!(cfg.jump_velocity > DEFAULT_PHYSICS.jump_velocity);
        assert_eq!(cfg.max_run_speed, DEFAULT_PHYSICS.max_run_speed);
        assert_eq!(cfg.max_air_jumps, DEFAULT_PHYSICS.max_air_jumps);
    }

    #[test]
    fn speed_raises_run_speed_and_acceleration() {
        let mut cfg = DEFAULT_PHYSICS;
        apply_item_to_physics(&mut cfg, Some(ItemKind::Speed));
        assert!(cfg.max_run_speed > DEFAULT_PHYSICS.max_run_speed);
        assert!(cfg.ground_accel > DEFAULT_PHYSICS.ground_accel);
        assert_eq!(cfg.jump_velocity, DEFAULT_PHYSICS.jump_velocity);
    }

    #[test]
    fn double_jump_enables_exactly_one_air_jump() {
        let mut cfg = DEFAULT_PHYSICS;
        apply_item_to_physics(&mut cfg, Some(ItemKind::DoubleJump));
        assert_eq!(cfg.max_air_jumps, ITEM_DOUBLE_JUMP_MAX_AIR_JUMPS);
        assert!(cfg.air_jump_velocity > 0.0);
        assert_eq!(cfg.max_run_speed, DEFAULT_PHYSICS.max_run_speed);
    }

    /// 拾取写入角色组件、销毁道具实体；二次拾取覆盖而不是叠加。
    #[test]
    fn pickup_overwrites_the_single_carried_item() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::physics::{Aabb, Body};
        use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId};

        let mut app = App::new();
        app.init_resource::<PickedZones>()
            .add_message::<ItemPicked>();
        let center = Vec2::new(100.0, 100.0);
        let player = app
            .world_mut()
            .spawn((
                PlayerId(0),
                PlayerMotion(Body::at_center(center)),
                CarriedItem::default(),
            ))
            .id();
        let zone = |kind: ItemKind| RoomZone {
            room_index: 0,
            kind: RoomZoneKind::Item(kind),
            aabb: Aabb::from_center_size(center, Vec2::splat(12.0)),
            at: (100, 100),
        };
        let speed = app
            .world_mut()
            .spawn((zone(ItemKind::Speed), ZoneId(1)))
            .id();
        app.world_mut()
            .run_system_once(pickup_items)
            .expect("拾取系统");
        assert_eq!(
            app.world().get::<CarriedItem>(player).unwrap().0,
            Some(ItemKind::Speed)
        );
        assert!(app.world().get_entity(speed).is_err(), "道具实体应销毁");
        assert!(app.world().resource::<PickedZones>().0.contains(&1));

        // 第二个道具：直接覆盖（角色身上仍然只有一个道具）
        app.world_mut()
            .spawn((zone(ItemKind::DoubleJump), ZoneId(2)));
        app.world_mut()
            .run_system_once(pickup_items)
            .expect("拾取系统");
        assert_eq!(
            app.world().get::<CarriedItem>(player).unwrap().0,
            Some(ItemKind::DoubleJump)
        );
    }
}
