//! 2D 视差背景。所有者文档：`docs/07-rendering.md` §7.13。
//!
//! 每层只是**一张平铺 sprite**：贴图按自身像素尺寸重复铺满视口，位置相位由
//! `scroll_x/scroll_y` 百分比驱动（`0` = 屏幕固定，`100` = 与世界同速）。
//! 不滚动世界、不移动相机，只在 `PostUpdate` 写 sprite 的 `Transform`。

use bevy::prelude::*;
use bevy::sprite::SpriteImageMode;

use crate::config::render::z;
use bevy::camera::visibility::RenderLayers;

use crate::render::pixel_camera::{MAX_VIEWS, PARALLAX_LAYER_BASE, PlayerView, ViewLayout};
use crate::world::load::LoadedLevel;

/// 视差背景使用的相机。游戏内层相机与编辑器的 `EditorCamera` 都插这个标记。
#[derive(Component)]
pub struct ParallaxCamera;

/// 编辑器预览覆盖：未落盘的 `doc.background`（只有编辑器插入）。
///
/// 有它时忽略 `LoadedLevel.background`，让编辑器画布实时反映编辑缓冲；
/// 游戏 bin 不插入该资源。
#[derive(Resource, Default, Clone)]
pub struct BackgroundOverride(pub Option<level_format::Background>);

/// 一层背景。`period` = 贴图像素尺寸（世界单位），用于平铺与相位取模；
/// `index` 用于每帧从关卡数据读取最新的滚动百分比（改数值不需要重建贴图）。
#[derive(Component, Debug, Clone)]
pub struct ParallaxLayer {
    /// 该份背景属于哪个玩家视图（本地双人时两套背景的相位不同）。
    view: u8,
    index: usize,
    period: Vec2,
}

/// 贴图身份键：路径 + mtime。任一变化 ⇒ 重新解码并重建实体（`04` §5.6）。
type ImageKey = (u8, Vec<(String, Option<std::time::SystemTime>)>);

/// 背景相机查询；`Without<ParallaxLayer>` 与层查询的 `Transform` 访问互斥（B0001）。
type ParallaxCameraQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Camera,
        &'static Projection,
        &'static Transform,
        Option<&'static PlayerView>,
    ),
    (With<ParallaxCamera>, Without<ParallaxLayer>),
>;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_parallax.before(bevy::transform::TransformSystems::Propagate),
        );
    }
}

/// 纯函数：给出一层的 sprite 中心与所需尺寸（覆盖视口 + 两个周期）。
///
/// 推导（`07` §7.13.2）：贴图原点在屏幕上的偏移 = `(-camera * scroll) mod period`，
/// sprite 的贴图原点在其左上角，所以中心 = `view_center + offset`，
/// 尺寸取 `view_size + 2 * period` 保证任意相位都能盖满视口。
pub fn parallax_placement(
    view_center: Vec2,
    view_size: Vec2,
    scroll: Vec2,
    period: Vec2,
) -> (Vec2, Vec2) {
    let offset = Vec2::new(
        wrap_offset(period.x, -view_center.x * scroll.x),
        wrap_offset(period.y, -view_center.y * scroll.y),
    );
    let size = (view_size + period * 2.0).round().max(Vec2::ONE);
    ((view_center + offset).round(), size)
}

/// `value mod period`，`period <= 0` 时视为 0（贴图为空时上层会跳过该层）。
fn wrap_offset(period: f32, value: f32) -> f32 {
    if period > 0.0 {
        value.rem_euclid(period)
    } else {
        0.0
    }
}

/// 相机视图：中心 = 相机 `Transform` 的 xy（游戏内没有相机旋转），
/// 世界尺寸 = 逻辑视口 × 正交 `scale`。直接读 `Transform` 而不是 `GlobalTransform`：
/// 后者在 `PostUpdate` 才传播，晚一帧会让背景在平移时拖影。
fn camera_view(
    camera: &Camera,
    projection: &Projection,
    transform: &Transform,
) -> Option<(Vec2, Vec2)> {
    let logical = camera.logical_viewport_size()?;
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => return None,
    };
    Some((transform.translation.truncate(), logical * scale))
}

#[allow(clippy::too_many_arguments)]
fn update_parallax(
    mut commands: Commands,
    loaded: Option<Res<LoadedLevel>>,
    preview: Option<Res<BackgroundOverride>>,
    layout: Option<Res<ViewLayout>>,
    mut images: ResMut<Assets<Image>>,
    mut layers: Query<(Entity, &ParallaxLayer, &mut Transform, &mut Sprite)>,
    camera: ParallaxCameraQuery,
    mut last_images: Local<Option<ImageKey>>,
) {
    let background = match preview.as_ref() {
        Some(preview) => preview.0.clone(),
        None => loaded
            .as_ref()
            .and_then(|loaded| loaded.level.background.clone()),
    };
    let Some(background) = background else {
        if last_images.is_some() {
            for (entity, ..) in &layers {
                commands.entity(entity).despawn();
            }
            *last_images = None;
        }
        return;
    };

    let views = layout
        .as_ref()
        .map(|layout| layout.views.min(MAX_VIEWS as u8))
        .unwrap_or(1);

    // 贴图身份变化才重建；滚动百分比每帧实时读取，拖 DragValue 不会反复解码 PNG。
    // 视图数量也是 key 的一部分：进入/退出本地双人时补齐/销毁另一套背景。
    let mut paths = Vec::with_capacity(background.layers.len());
    for layer in &background.layers {
        let path = level_format::normalize_asset_path(&layer.image).unwrap_or_default();
        let stamp = crate::assets::texture_stamp(&path);
        paths.push((path, stamp));
    }
    let key: ImageKey = (views, paths);

    // 每个视图的相机视图：索引 = `PlayerView`（编辑器相机没有该组件 ⇒ 视图 0）。
    let mut view_cameras: [Option<(Vec2, Vec2)>; MAX_VIEWS] = [None; MAX_VIEWS];
    for (camera, projection, transform, player_view) in camera.iter() {
        if !camera.is_active {
            continue;
        }
        let view = player_view.map(|view| view.0 as usize).unwrap_or(0);
        if view >= views as usize {
            continue;
        }
        if let Some(found) = camera_view(camera, projection, transform) {
            view_cameras[view] = Some(found);
        }
    }
    if view_cameras[0].is_none() {
        return;
    }

    if last_images.as_ref() != Some(&key) {
        *last_images = Some(key);
        for (entity, ..) in &layers {
            commands.entity(entity).despawn();
        }
        for view in 0..views {
            let Some((view_center, view_size)) = view_cameras[view as usize] else {
                continue;
            };
            for (index, layer) in background.layers.iter().enumerate() {
                let Some(path) = level_format::normalize_asset_path(&layer.image) else {
                    warn!("背景图层 {index}：路径为空，已跳过");
                    continue;
                };
                let Some(handle) = crate::render::tiles::load_image(&path, &mut images) else {
                    warn!("背景图层 {index}：贴图 `{path}` 加载失败，已跳过");
                    continue;
                };
                let Some(image) = images.get(&handle) else {
                    continue;
                };
                let period = image.size_f32();
                if period.min_element() <= 0.0 {
                    warn!("背景图层 {index}：贴图 `{path}` 尺寸为 0，已跳过");
                    continue;
                }
                let (center, size) =
                    parallax_placement(view_center, view_size, scroll_of(layer), period);
                commands.spawn((
                    ParallaxLayer {
                        view,
                        index,
                        period,
                    },
                    Sprite {
                        image: handle,
                        image_mode: SpriteImageMode::Tiled {
                            tile_x: true,
                            tile_y: true,
                            stretch_value: 1.0,
                        },
                        custom_size: Some(size),
                        ..default()
                    },
                    Transform::from_xyz(center.x, center.y, z::PARALLAX + index as f32),
                    RenderLayers::layer(PARALLAX_LAYER_BASE + view as usize),
                ));
            }
        }
    }

    // 每帧写位置：相机移动与滚动百分比变化都即时生效。
    for (_, layer, mut transform, mut sprite) in layers.iter_mut() {
        let Some(view_camera) = view_cameras[layer.view as usize] else {
            continue;
        };
        let Some(definition) = background.layers.get(layer.index) else {
            continue;
        };
        let (center, size) = parallax_placement(
            view_camera.0,
            view_camera.1,
            scroll_of(definition),
            layer.period,
        );
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        sprite.custom_size = Some(size);
    }
}

fn scroll_of(layer: &level_format::BackgroundLayer) -> Vec2 {
    Vec2::new(layer.scroll_x as f32, layer.scroll_y as f32) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: Vec2 = Vec2::new(64.0, 32.0);
    const VIEW: Vec2 = Vec2::new(512.0, 512.0);

    /// 任意滚动百分比/相机位置下，平铺 sprite 必须覆盖整个视口。
    #[test]
    fn placement_always_covers_view() {
        for camera in [
            Vec2::ZERO,
            Vec2::new(123.4, -77.0),
            Vec2::new(2000.0, 3000.0),
            Vec2::new(-4000.5, 900.25),
        ] {
            for scroll in [Vec2::ZERO, Vec2::splat(0.3), Vec2::splat(0.5), Vec2::ONE] {
                let (center, size) = parallax_placement(camera, VIEW, scroll, PERIOD);
                let min = center - size / 2.0;
                let max = center + size / 2.0;
                let view_min = camera - VIEW / 2.0;
                let view_max = camera + VIEW / 2.0;
                assert!(
                    min.x <= view_min.x
                        && min.y <= view_min.y
                        && max.x >= view_max.x
                        && max.y >= view_max.y,
                    "camera={camera:?} scroll={scroll:?} center={center:?} size={size:?}"
                );
            }
        }
    }

    /// `0%` = 屏幕固定：sprite 中心跟随相机（相对屏幕不动）。
    /// `100%` = 与世界同速：相位相对世界固定。
    #[test]
    fn scroll_extremes_are_screen_locked_and_world_locked() {
        let camera = Vec2::new(300.0, 200.0);
        let (screen, _) = parallax_placement(camera, VIEW, Vec2::ZERO, PERIOD);
        assert_eq!(screen, camera);

        // 世界固定：向右移动恰好一个周期，相位必须回到同一屏幕位置。
        let (a, _) = parallax_placement(camera, VIEW, Vec2::ONE, PERIOD);
        let moved = camera + Vec2::new(PERIOD.x, PERIOD.y);
        let (b, _) = parallax_placement(moved, VIEW, Vec2::ONE, PERIOD);
        let wrap = |value: f32, period: f32| value.rem_euclid(period);
        assert_eq!(
            wrap(a.x - camera.x, PERIOD.x),
            wrap(b.x - moved.x, PERIOD.x)
        );
        assert_eq!(
            wrap(a.y - camera.y, PERIOD.y),
            wrap(b.y - moved.y, PERIOD.y)
        );
    }

    /// 零尺寸贴图不能让相位计算产生 NaN。
    #[test]
    fn zero_period_is_finite() {
        let (center, size) = parallax_placement(Vec2::new(10.0, 10.0), VIEW, Vec2::ONE, Vec2::ZERO);
        assert!(center.is_finite());
        assert!(size.is_finite());
        assert_eq!(center, Vec2::new(10.0, 10.0));
    }
}
