//! 装饰品：拥有集合、装备槽位与图层渲染。所有者文档：`docs/01-game-design.md` §1.5、
//! `docs/04-assets.md` §6、`docs/00-conventions.md` §0.4（Z 公式）。
//!
//! 规则（`04` §6.3）：某槽位未装备 ⇒ **不 spawn** 该层实体；首次装备创建；替换装备复用实体；
//! 卸下 despawn；存档里的未知 id 视为未装备（不报错、不生成占位）。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::{Deserialize, Serialize};

use crate::config::render::{DECORATION_LAYERS, z};
use crate::player::{PlayerFacing, PlayerMotion};

/// 装饰品元数据（`04` §6.2）：所属图层 + 相对脚底中点（`Anchor::BOTTOM_CENTER`）的偏移。
#[derive(Debug, Clone, Copy)]
pub struct DecorationSpec {
    pub id: &'static str,
    pub layer: usize,
    pub offset: Vec2,
}

/// 首批装饰品表。**id 的唯一来源仍是关卡 `reward` 字段**（`01` §1.5）；本表只补图层与偏移。
///
/// 新增 id 时必须同时：补 `assets/sprites/decorations/<id>.png`、在关卡里使用它。
/// 每个 id 的图层与相对脚底的偏移（`04` §6.2）。层序见 `04` §6.1；偏移按层给出占位值。
pub const DECORATION_SPECS: &[DecorationSpec] = &[
    DecorationSpec { id: "item_01", layer: 0, offset: Vec2::new(0.0, 6.0) },
    DecorationSpec { id: "item_02", layer: 1, offset: Vec2::new(0.0, 0.0) },
    DecorationSpec { id: "item_03", layer: 2, offset: Vec2::new(0.0, 10.0) },
    DecorationSpec { id: "item_04", layer: 3, offset: Vec2::new(0.0, 18.0) },
    DecorationSpec { id: "item_05", layer: 4, offset: Vec2::new(0.0, 22.0) },
    DecorationSpec { id: "item_06", layer: 5, offset: Vec2::new(0.0, 26.0) },
    DecorationSpec { id: "item_07", layer: 6, offset: Vec2::new(0.0, 20.0) },
    DecorationSpec { id: "item_08", layer: 0, offset: Vec2::new(0.0, 6.0) },
    DecorationSpec { id: "item_09", layer: 1, offset: Vec2::new(0.0, 0.0) },
    DecorationSpec { id: "item_10", layer: 2, offset: Vec2::new(0.0, 10.0) },
    DecorationSpec { id: "item_11", layer: 3, offset: Vec2::new(0.0, 18.0) },
    DecorationSpec { id: "item_12", layer: 4, offset: Vec2::new(0.0, 22.0) },
    DecorationSpec { id: "item_13", layer: 5, offset: Vec2::new(0.0, 26.0) },
    DecorationSpec { id: "item_14", layer: 6, offset: Vec2::new(0.0, 20.0) },
    DecorationSpec { id: "item_15", layer: 0, offset: Vec2::new(0.0, 6.0) },
    DecorationSpec { id: "item_16", layer: 1, offset: Vec2::new(0.0, 0.0) },
    DecorationSpec { id: "item_17", layer: 2, offset: Vec2::new(0.0, 10.0) },
    DecorationSpec { id: "item_18", layer: 3, offset: Vec2::new(0.0, 18.0) },
    DecorationSpec { id: "item_19", layer: 4, offset: Vec2::new(0.0, 22.0) },
    DecorationSpec { id: "item_20", layer: 5, offset: Vec2::new(0.0, 26.0) },
    DecorationSpec { id: "item_21", layer: 6, offset: Vec2::new(0.0, 20.0) },
    DecorationSpec { id: "item_22", layer: 0, offset: Vec2::new(0.0, 6.0) },
    DecorationSpec { id: "item_23", layer: 1, offset: Vec2::new(0.0, 0.0) },
    DecorationSpec { id: "item_24", layer: 2, offset: Vec2::new(0.0, 10.0) },
    DecorationSpec { id: "item_25", layer: 3, offset: Vec2::new(0.0, 18.0) },
    DecorationSpec { id: "item_26", layer: 4, offset: Vec2::new(0.0, 22.0) },
    DecorationSpec { id: "item_27", layer: 5, offset: Vec2::new(0.0, 26.0) },
    DecorationSpec { id: "item_28", layer: 6, offset: Vec2::new(0.0, 20.0) },
    DecorationSpec { id: "item_29", layer: 0, offset: Vec2::new(0.0, 6.0) },
    DecorationSpec { id: "item_30", layer: 1, offset: Vec2::new(0.0, 0.0) },
];

pub fn spec_of(id: &str) -> Option<&'static DecorationSpec> {
    DECORATION_SPECS.iter().find(|spec| spec.id == id)
}

/// 玩家拥有的装饰品 + 各槽位的装备。
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decorations {
    owned: Vec<String>,
    /// 长度恒为 [`DECORATION_LAYERS`]（从存档读入时归一化）。
    equipped: Vec<Option<String>>,
}

impl Decorations {
    pub fn owned(&self) -> &[String] {
        &self.owned
    }

    pub fn has(&self, id: &str) -> bool {
        self.owned.iter().any(|owned| owned == id)
    }

    pub fn len(&self) -> usize {
        self.owned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }

    /// 幂等授予：已拥有返回 `false`。
    pub fn grant(&mut self, id: impl Into<String>) -> bool {
        let id = id.into();
        if self.has(&id) {
            return false;
        }
        self.owned.push(id);
        true
    }

    /// 某个槽位上装备的 id。
    pub fn equipped_at(&self, layer: usize) -> Option<&str> {
        self.equipped
            .get(layer)
            .and_then(|slot| slot.as_deref())
    }

    /// 全部槽位（归一化到 `DECORATION_LAYERS`）。
    pub fn equipped(&self) -> Vec<Option<String>> {
        (0..DECORATION_LAYERS)
            .map(|layer| self.equipped_at(layer).map(str::to_string))
            .collect()
    }

    /// 装备到该 id 的图层（同层替换）。未拥有 / 未知 id ⇒ `false` 且不改状态。
    pub fn equip(&mut self, id: &str) -> bool {
        if !self.has(id) {
            return false;
        }
        let Some(spec) = spec_of(id) else {
            return false;
        };
        self.ensure_slots();
        self.equipped[spec.layer] = Some(id.to_string());
        true
    }

    /// 卸下某一层；本来就没装备返回 `false`。
    pub fn unequip(&mut self, layer: usize) -> bool {
        self.ensure_slots();
        self.equipped.get_mut(layer).is_some_and(|slot| slot.take().is_some())
    }

    /// 从存档恢复（未知 id 保留在"拥有"里，但装备位会被校验掉）。
    pub fn restore(owned: Vec<String>, equipped: Vec<Option<String>>) -> Self {
        let mut decorations = Self {
            owned,
            equipped,
        };
        decorations.ensure_slots();
        // 只保留「已拥有且元数据已知」的装备
        let owned = decorations.owned.clone();
        for slot in &mut decorations.equipped {
            let valid = slot
                .as_deref()
                .is_some_and(|id| owned.iter().any(|known| known == id) && spec_of(id).is_some());
            if !valid {
                *slot = None;
            }
        }
        decorations
    }

    fn ensure_slots(&mut self) {
        self.equipped.resize(DECORATION_LAYERS, None);
    }
}

/// 挂在角色身上的一层装饰品实体。
#[derive(Component, Debug, Clone, Copy)]
pub struct DecorationLayer(pub usize);

/// 装饰品图层实体的**局部** Z。
///
/// `00` §0.4 的 `41.0 + layer_index` 是**世界空间绝对值**；装饰品实体是角色的子实体
/// （`ChildOf`），`GlobalTransform` 会把父级 Z（玩家本体 `z::PLAYER`）叠加进来。因此局部值
/// 必须减掉父级 Z，否则实际落在 `81.0 + layer_index`，会盖住粒子（60）与前景瓦片（70）。
pub fn decoration_local_z(layer: usize) -> f32 {
    z::DECORATION_BASE + layer as f32 - z::PLAYER
}

/// 朝向镜像后的装饰品偏移（`04` §4.5 / §6.2）：朝左时 `x` 取反、`y` 不变。
pub fn mirrored_offset(offset: Vec2, facing_right: bool) -> Vec2 {
    if facing_right {
        offset
    } else {
        Vec2::new(-offset.x, offset.y)
    }
}

/// 装备变化 ⇒ 重建/替换/销毁图层实体（`04` §6.3）。
pub fn sync_decoration_layers(
    mut commands: Commands,
    decorations: Res<Decorations>,
    assets: Res<AssetServer>,
    players: Query<(Entity, &PlayerFacing), With<PlayerMotion>>,
    layers: Query<(Entity, &DecorationLayer)>,
) {
    if !decorations.is_changed() {
        return;
    }
    let Some((player, facing)) = players.iter().next() else {
        return;
    };
    let facing_right = facing.0;
    // 现有的层：layer -> entity
    let mut existing: Vec<Option<Entity>> = vec![None; DECORATION_LAYERS];
    for (entity, layer) in &layers {
        if layer.0 < DECORATION_LAYERS {
            existing[layer.0] = Some(entity);
        }
    }

    for layer in 0..DECORATION_LAYERS {
        match decorations.equipped_at(layer) {
            Some(id) => {
                let Some(spec) = spec_of(id) else {
                    // 未知 id 视为未装备（不报错、不生成占位）
                    if let Some(entity) = existing[layer].take() {
                        commands.entity(entity).despawn();
                    }
                    continue;
                };
                let image = assets.load(format!("sprites/decorations/{id}.png"));
                match existing[layer].take() {
                    Some(entity) => {
                        // 替换装备：复用实体，只换贴图（翻转由 `sync_decoration_facing` 维持）
                        commands.entity(entity).insert(Sprite::from_image(image));
                    }
                    None => {
                        let offset = mirrored_offset(spec.offset, facing_right);
                        commands.spawn((
                            Sprite::from_image(image),
                            Anchor::BOTTOM_CENTER,
                            Transform::from_xyz(offset.x, offset.y, decoration_local_z(spec.layer)),
                            DecorationLayer(layer),
                            ChildOf(player),
                        ));
                    }
                }
            }
            None => {
                if let Some(entity) = existing[layer].take() {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

/// 朝向变化/装备变化 ⇒ 装饰品同步镜像（`04` §4.5：`flip_x` 跟随角色、`offset.x` 取反）。
///
/// 只在值真的变化时写组件；7 个图层的逐帧比较成本可忽略。
pub fn sync_decoration_facing(
    decorations: Res<Decorations>,
    players: Query<(Entity, &PlayerFacing), With<PlayerMotion>>,
    mut layers: Query<(&DecorationLayer, &ChildOf, &mut Sprite, &mut Transform)>,
) {
    let Some((player, facing)) = players.iter().next() else {
        return;
    };
    let facing_right = facing.0;
    for (layer, parent, mut sprite, mut transform) in &mut layers {
        if parent.parent() != player {
            continue;
        }
        let Some(spec) = decorations.equipped_at(layer.0).and_then(spec_of) else {
            continue;
        };
        let flip_x = !facing_right;
        if sprite.flip_x != flip_x {
            sprite.flip_x = flip_x;
        }
        let x = mirrored_offset(spec.offset, facing_right).x;
        if transform.translation.x != x {
            transform.translation.x = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 镜像只改 `x` 符号、不改 `y`（`04` §4.5 / §6.2）。
    #[test]
    fn mirrored_offset_negates_x_only() {
        let offset = Vec2::new(3.0, 7.0);
        assert_eq!(mirrored_offset(offset, true), Vec2::new(3.0, 7.0));
        assert_eq!(mirrored_offset(offset, false), Vec2::new(-3.0, 7.0));
        for spec in DECORATION_SPECS {
            let mirrored = mirrored_offset(spec.offset, false);
            assert_eq!(mirrored.x, -spec.offset.x);
            assert_eq!(mirrored.y, spec.offset.y);
        }
    }

    /// 装饰品的**世界** Z 必须等于 `00` §0.4 的 `41.0 + layer`；子实体的局部 Z 需扣掉父级。
    #[test]
    fn decoration_world_z_matches_the_z_table() {
        for layer in 0..DECORATION_LAYERS {
            let world = z::PLAYER + decoration_local_z(layer);
            assert_eq!(world, z::DECORATION_BASE + layer as f32);
            assert!(world > z::PLAYER, "装饰品必须画在角色本体之上");
        }
    }

    #[test]
    fn grant_is_idempotent() {
        let mut decorations = Decorations::default();
        assert!(decorations.grant("item_01"));
        assert!(!decorations.grant("item_01"));
        assert_eq!(decorations.len(), 1);
    }

    /// 装备落在元数据声明的槽位；重复装备同一件是幂等的。
    /// 同层"替换"由 `equip_requires_ownership_and_known_metadata` + 图层赋值覆盖
    /// （当前只有一件装饰品有元数据，等第二件入库后再补替换用例）。
    #[test]
    fn equip_fills_its_declared_layer() {
        let mut decorations = Decorations::default();
        decorations.grant("item_01");
        assert!(decorations.equip("item_01"));
        let expected_layer = spec_of("item_01").unwrap().layer;
        assert_eq!(decorations.equipped_at(expected_layer), Some("item_01"));
        assert_eq!(decorations.equipped().len(), DECORATION_LAYERS);
        assert!(decorations.equip("item_01"), "重复装备是幂等的");
    }

    #[test]
    fn equip_requires_ownership_and_known_metadata() {
        let mut decorations = Decorations::default();
        assert!(!decorations.equip("item_01"), "未拥有不能装备");
        decorations.grant("mystery");
        assert!(!decorations.equip("mystery"), "未知 id 视为未装备");
        assert_eq!(decorations.equipped_at(5), None);
    }

    #[test]
    fn unequip_clears_only_that_layer() {
        let mut decorations = Decorations::default();
        decorations.grant("item_01");
        decorations.equip("item_01");
        let layer = spec_of("item_01").unwrap().layer;
        assert!(decorations.unequip(layer));
        assert!(!decorations.unequip(layer));
        assert_eq!(decorations.equipped_at(layer), None);
    }

    #[test]
    fn restore_normalizes_and_drops_invalid_equipment() {
        let item_01_layer = spec_of("item_01").unwrap().layer;
        let mut equipped = vec![None; DECORATION_LAYERS];
        equipped[item_01_layer] = Some("item_01".to_string());
        let decorations = Decorations::restore(vec!["item_01".to_string()], equipped.clone());
        assert_eq!(
            decorations.equipped_at(item_01_layer),
            Some("item_01")
        );
        assert_eq!(decorations.equipped().len(), DECORATION_LAYERS);
        // 未拥有 ⇒ 装备位被清空
        let decorations = Decorations::restore(vec![], equipped);
        assert_eq!(decorations.equipped_at(item_01_layer), None);
    }
}
