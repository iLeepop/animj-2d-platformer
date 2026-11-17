//! 道具：拾取、覆盖与能力修正。所有者文档：`docs/01-game-design.md` §1.4、
//! `docs/06-physics-and-movement.md` §6.12、`docs/03-level-format.md` §3.4。
//!
//! 规则：**每个角色同时只携带一个道具**；再次拾取直接覆盖（不叠加、不排队）。
//! 道具实体拾取后销毁；死亡不清空道具（避免一次性道具被消耗后卡死关卡）。

use bevy::prelude::*;
use level_format::ItemKind;

use crate::config::item::ITEM_RESPAWN_SECS;
use crate::config::physics::{
    FIXED_DT, ITEM_DOUBLE_JUMP_MAX_AIR_JUMPS, ITEM_DOUBLE_JUMP_VELOCITY_MULT,
    ITEM_SPEED_ACCEL_MULT, ITEM_SPEED_MAX_MULT, ITEM_SUPER_JUMP_VELOCITY_MULT, PICKUP_INFLATE,
    PhysicsConfig,
};
use crate::player::{PlayerId, PlayerMotion};
use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId, aabb_overlaps};

/// 角色当前携带的道具（`kind` = `None` 表示空手）与剩余 buff 时间。
///
/// 同一角色至多一个道具；再次拾取覆盖并按关卡 `items` 配置重置持续时间。
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct CarriedItem {
    pub kind: Option<ItemKind>,
    /// 剩余 buff 时间（秒）；`kind == None` 时无意义。
    pub remaining: f32,
}

impl CarriedItem {
    pub fn get(self) -> Option<ItemKind> {
        self.kind
    }

    /// 拾取 / 覆盖道具，并把持续时间重置为该道具在关卡里的配置值。
    pub fn pick(&mut self, kind: ItemKind, duration_secs: f32) {
        self.kind = Some(kind);
        self.remaining = duration_secs.max(0.01);
    }

    /// 清除 buff（死亡或超时）。
    pub fn clear(&mut self) {
        self.kind = None;
        self.remaining = 0.0;
    }
}

/// 道具生成器生成的道具：被拾取后隐藏，`ITEM_RESPAWN_SECS` 后重新激活。
///
/// 只有 `item_spawner` 生成的道具有这个组件；普通 `item` 实体拾取后直接销毁。
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SpawnerItem {
    pub active: bool,
    /// 剩余重生时间（秒）；仅在 `active == false` 时有意义。
    pub respawn_left: f32,
}

impl Default for SpawnerItem {
    fn default() -> Self {
        Self {
            active: true,
            respawn_left: 0.0,
        }
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
            (pickup_items, tick_item_buffs, tick_item_spawners)
                .chain()
                .after(crate::player::apply_player_physics)
                .in_set(crate::state::GameSet::Physics)
                .run_if(crate::net::not_client),
        );
    }
}

/// 玩家 AABB 与道具区域相交 ⇒ 拾取并覆盖当前道具，销毁道具实体。
///
/// 每个道具每个固定步只被一名玩家拾取（`find` 的第一名）；多人同时踩住不会复制。
#[allow(clippy::type_complexity)]
fn pickup_items(
    mut commands: Commands,
    loaded: Option<Res<crate::world::load::LoadedLevel>>,
    mut players: Query<
        (&PlayerId, &PlayerMotion, &mut CarriedItem),
        Without<crate::player::death::Dying>,
    >,
    mut zones: Query<(
        Entity,
        &RoomZone,
        Option<&ZoneId>,
        Option<&mut SpawnerItem>,
        Option<&mut Visibility>,
    )>,
    mut picked_zones: ResMut<PickedZones>,
    mut picked: MessageWriter<ItemPicked>,
) {
    for (entity, zone, zone_id, mut spawner, mut visibility) in &mut zones {
        let RoomZoneKind::Item(kind) = zone.kind else {
            continue;
        };
        // 生成器道具在重生倒计时期间不可拾取。
        if spawner.as_ref().is_some_and(|spawner| !spawner.active) {
            continue;
        }
        let Some((id, _, mut carried)) = players.iter_mut().find(|(_, motion, _)| {
            aabb_overlaps(&motion.0.aabb.inset(-PICKUP_INFLATE), &zone.aabb)
        }) else {
            continue;
        };
        let replaced = carried.kind;
        let duration = loaded
            .as_ref()
            .map(|loaded| loaded.level.item_config(kind).duration_secs)
            .unwrap_or(level_format::DEFAULT_ITEM_DURATION_SECS);
        carried.pick(kind, duration);
        match spawner.as_mut() {
            Some(spawner) => {
                // 生成器：隐藏并开始 1.5 s 重生倒计时，不销毁实体。
                spawner.active = false;
                spawner.respawn_left = ITEM_RESPAWN_SECS;
                if let Some(visibility) = visibility.as_mut() {
                    **visibility = Visibility::Hidden;
                }
            }
            None => {
                // 一次性道具：销毁并记录，供客户端同步。
                if let Some(zone_id) = zone_id {
                    picked_zones.0.insert(zone_id.0);
                }
                commands.entity(entity).despawn();
            }
        }
        picked.write(ItemPicked {
            player: *id,
            kind,
            replaced,
        });
        info!(
            "玩家 {} 拾取道具：{}（{}）{}",
            id.0,
            kind.id(),
            match replaced {
                Some(previous) => format!("覆盖 {}", previous.id()),
                None => "空手".to_string(),
            },
            if spawner.is_some() {
                format!("，生成器 {ITEM_RESPAWN_SECS} 秒后重生")
            } else {
                String::new()
            }
        );
    }
}

/// buff 持续时间倒计时：耗尽后自动清除（固定步，主机权威）。
fn tick_item_buffs(mut players: Query<(&PlayerId, &mut CarriedItem)>) {
    for (id, mut carried) in &mut players {
        let Some(kind) = carried.kind else {
            continue;
        };
        carried.remaining -= FIXED_DT;
        if carried.remaining > 0.0 {
            continue;
        }
        carried.clear();
        info!("玩家 {} 道具效果结束：{}", id.0, kind.id());
    }
}

/// 生成器道具的 1.5 s 重生倒计时（固定步，主机权威）。
fn tick_item_spawners(mut spawners: Query<(&mut SpawnerItem, &mut Visibility, &RoomZone)>) {
    for (mut spawner, mut visibility, zone) in &mut spawners {
        if spawner.active {
            continue;
        }
        spawner.respawn_left -= FIXED_DT;
        if spawner.respawn_left > 0.0 {
            continue;
        }
        spawner.active = true;
        spawner.respawn_left = 0.0;
        *visibility = Visibility::Inherited;
        if let RoomZoneKind::Item(kind) = zone.kind {
            info!("道具生成器：重新生成 {}", kind.id());
        }
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

    /// 生成器道具被拾取后隐藏并进入倒计时；倒计时结束后重新激活。
    #[test]
    fn spawner_item_respawns_after_the_configured_delay() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::physics::{Aabb, Body};
        use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId};

        let mut app = App::new();
        app.init_resource::<PickedZones>()
            .add_message::<ItemPicked>();
        let center = Vec2::new(100.0, 100.0);
        app.world_mut().spawn((
            PlayerId(0),
            PlayerMotion(Body::at_center(center)),
            CarriedItem::default(),
        ));
        let zone = app
            .world_mut()
            .spawn((
                RoomZone {
                    room_index: 0,
                    kind: RoomZoneKind::Item(ItemKind::Speed),
                    aabb: Aabb::from_center_size(center, Vec2::splat(12.0)),
                    at: (100, 100),
                },
                ZoneId(3),
                SpawnerItem::default(),
                Visibility::Inherited,
            ))
            .id();
        app.world_mut()
            .run_system_once(pickup_items)
            .expect("拾取系统");
        assert!(app.world().get_entity(zone).is_ok(), "生成器道具不销毁");
        assert!(
            !app.world().get::<SpawnerItem>(zone).unwrap().active,
            "拾取后进入未激活"
        );
        assert_eq!(
            app.world().get::<SpawnerItem>(zone).unwrap().respawn_left,
            ITEM_RESPAWN_SECS
        );
        assert!(!app.world().resource::<PickedZones>().0.contains(&3));

        // 倒计时结束：重新激活 + 可见
        {
            let mut entity = app.world_mut().entity_mut(zone);
            entity.get_mut::<SpawnerItem>().unwrap().respawn_left = FIXED_DT * 0.5;
        }
        app.world_mut()
            .run_system_once(tick_item_spawners)
            .expect("重生系统");
        assert!(app.world().get::<SpawnerItem>(zone).unwrap().active);
        assert_ne!(
            *app.world().get::<Visibility>(zone).unwrap(),
            Visibility::Hidden
        );
    }

    /// 拾取写入角色组件、销毁道具实体；二次拾取覆盖而不是叠加。
    #[test]
    fn pickup_overwrites_the_single_carried_item() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::physics::{Aabb, Body};
        use crate::room::{PickedZones, RoomZone, RoomZoneKind, ZoneId};

        let mut app = App::new();
        app.init_resource::<PickedZones>()
            .add_message::<ItemPicked>()
            .insert_resource(crate::world::load::LoadedLevel {
                level: level_format::Level {
                    version: level_format::VERSION,
                    name: "t".into(),
                    grid: level_format::Grid { cols: 1, rows: 1 },
                    start: level_format::RoomCoord { col: 0, row: 0 },
                    tileset: None,
                    tileset_columns: 8,
                    palette: Vec::new(),
                    background: None,
                    items: vec![level_format::ItemDef {
                        kind: ItemKind::Speed,
                        sprite: None,
                        sprite_cell: None,
                        duration_secs: Some(3.0),
                    }],
                    rooms: Vec::new(),
                },
                handle: Default::default(),
            });
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
            app.world().get::<CarriedItem>(player).unwrap().kind,
            Some(ItemKind::Speed)
        );
        assert_eq!(
            app.world().get::<CarriedItem>(player).unwrap().remaining,
            3.0,
            "持续时间来自关卡 items 配置"
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
            app.world().get::<CarriedItem>(player).unwrap().kind,
            Some(ItemKind::DoubleJump)
        );
        assert_eq!(
            app.world().get::<CarriedItem>(player).unwrap().remaining,
            level_format::DEFAULT_ITEM_DURATION_SECS,
            "覆盖拾取重置 buff 时间"
        );
    }

    /// buff 到期后自动清除。
    #[test]
    fn item_buff_expires_after_its_duration() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        app.world_mut().spawn((
            PlayerId(0),
            CarriedItem {
                kind: Some(ItemKind::Speed),
                remaining: FIXED_DT * 0.5,
            },
        ));
        app.world_mut()
            .run_system_once(tick_item_buffs)
            .expect("buff 倒计时");
        let mut query = app.world_mut().query::<&CarriedItem>();
        let item = query.single(app.world()).unwrap();
        assert_eq!(item.kind, None, "到期后必须清除道具");
        assert_eq!(item.remaining, 0.0);
    }
}
