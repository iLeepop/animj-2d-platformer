//! 像素吸附。所有者文档：`docs/07-rendering.md` §7.6。
//!
//! 内层视口里 1 个 texel = 1 个世界单位，sprite 落在非整数坐标上会「像素爬行」。
//! 逻辑层继续用 `f32`，**只在渲染投影这一步**取整。

use bevy::prelude::*;

use crate::state::GameSet;

/// 需要吸附的可渲染实体标记。
///
/// 只挂在 **root** 实体上：带父实体的瓦片局部偏移本身是整像素，父级又是整像素，
/// 子级世界位置天然是整数（`07` §7.6.4）。
#[derive(Component)]
pub struct PixelSnap;

/// 本项目**唯一**的像素取整入口。禁止在别处手写 `round` / `floor`。
#[inline]
pub fn snap_px(v: f32) -> f32 {
    v.round()
}

pub struct SnapPlugin;

impl Plugin for SnapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (sync_player_transform, snap_render_transforms)
                .chain()
                .in_set(GameSet::Snap),
        );
    }
}

/// `Aabb` → `Transform` 的**单向投影**（`06` §6.8.2）：位置语义 = AABB 底边中点（脚底中点）。
///
/// 全世界只有这里与 [`snap_render_transforms`] 会写渲染位置；物理侧任何地方都不得读 `Transform`。
pub fn sync_player_transform(mut players: Query<(&crate::player::PlayerMotion, &mut Transform)>) {
    for (motion, mut transform) in &mut players {
        let aabb = motion.0.aabb;
        transform.translation.x = snap_px(aabb.center().x);
        transform.translation.y = snap_px(aabb.min.y);
    }
}

/// 通用兜底吸附：对带 [`PixelSnap`] 的实体取整 `translation.x/y`（幂等）。
///
/// 不动 `translation.z`、`scale`、`rotation`。
fn snap_render_transforms(mut query: Query<&mut Transform, With<PixelSnap>>) {
    for mut transform in &mut query {
        transform.translation.x = snap_px(transform.translation.x);
        transform.translation.y = snap_px(transform.translation.y);
    }
}
