//! 双层相机像素完美管线。所有者文档：`docs/07-rendering.md` §7.2。
//!
//! 结构：内层相机把「恰好一个房间」画进 512×512 离屏纹理（1 世界单位 = 1 texel），
//! 外层相机把该纹理铺在原点并按**整数倍** `k` 放大到窗口，四周是清屏色（黑边）。

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::config::render::{
    CAMERA_Z, CANVAS_FORMAT, INNER_CAMERA_ORDER, MIN_CANVAS_SCALE, OUTER_CAMERA_ORDER, VIEWPORT_H,
    VIEWPORT_W, z,
};
use crate::render::snap::PixelSnap;

/// 内层相机只渲染这一层（所有游戏内 `Sprite` / `Text2d`）。
pub const PIXEL_PERFECT_LAYERS: RenderLayers = RenderLayers::layer(0);
/// 外层相机只渲染这一层（承载离屏纹理的那张 canvas sprite）。
pub const HIGH_RES_LAYERS: RenderLayers = RenderLayers::layer(1);

/// 渲染一个房间内容的内层相机。
#[derive(Component)]
pub struct InGameCamera;

/// 把离屏纹理放大到窗口的外层相机。
#[derive(Component)]
pub struct OuterCamera;

/// 承载离屏纹理的 sprite。
#[derive(Component)]
pub struct Canvas;

/// 当前整数放大倍数 `k`。HUD、dev 覆盖层、截图验证都读它。
#[derive(Resource, Default, Deref, DerefMut)]
pub struct CanvasScale(pub u32);

/// 离屏渲染目标句柄。截图验证（`Screenshot::image`）与调试需要它。
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct CanvasImage(pub Handle<Image>);

pub struct PixelCameraPlugin;

impl Plugin for PixelCameraPlugin {
    fn build(&self, app: &mut App) {
        // 房间是方形的：这是「一屏 = 一个房间」契约（00 D5/D13）的显式断言。
        assert_eq!(
            VIEWPORT_W, VIEWPORT_H,
            "视口必须是正方形（一个房间），见 docs/00-conventions.md §0.3"
        );

        app.init_resource::<CanvasScale>()
            // 黑边 = 清屏色；这样「视口外区域」只有一种颜色
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_pixel_camera)
            .add_systems(Update, fit_canvas);
    }
}

fn setup_pixel_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // ① 离屏渲染目标：恰好一个房间
    let mut canvas_image = Image::new_target_texture(VIEWPORT_W, VIEWPORT_H, CANVAS_FORMAT, None);
    // 允许 `Screenshot::image(canvas)` 把它拷回 CPU（M0 的像素完美验证依赖这条读回路径；
    // 用法与官方 example 为 `ImageCopier` 设置 `COPY_SRC` 的做法一致）。不改变显存布局，无运行期代价。
    canvas_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let canvas_image = images.add(canvas_image);
    commands.insert_resource(CanvasImage(canvas_image.clone()));

    // ② 内层相机：世界 → 离屏纹理，1 世界单位 = 1 texel
    commands.spawn((
        Camera2d,
        Camera {
            order: INNER_CAMERA_ORDER,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: VIEWPORT_W as f32,
                height: VIEWPORT_H as f32,
            },
            scale: 1.0,
            ..OrthographicProjection::default_2d()
        }),
        RenderTarget::Image(canvas_image.clone().into()),
        Msaa::Off,
        // 显式写 Transform：相机位置由 `camera::rig` 只改 x/y。
        // z 必须为 0.0 —— 见 `config::render::CAMERA_Z` 的说明（远景视差在 -100）。
        Transform::from_xyz(0.0, 0.0, CAMERA_Z),
        InGameCamera,
        crate::render::parallax::ParallaxCamera,
        PIXEL_PERFECT_LAYERS,
        PixelSnap,
        // HUD 走内层相机 ⇒ HUD 与游戏画面共用一个像素格（07 §7.10.1）
        IsDefaultUiCamera,
    ));

    // ③ 把离屏纹理铺进「高分辨率世界」
    commands.spawn((
        Sprite::from_image(canvas_image),
        Canvas,
        HIGH_RES_LAYERS,
        Transform::from_xyz(0.0, 0.0, z::CANVAS),
    ));

    // ④ 外层相机：离屏纹理 → 窗口（整数倍放大 + 黑边）
    commands.spawn((
        Camera2d,
        Camera {
            order: OUTER_CAMERA_ORDER,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::WindowSize,
            scale: 1.0, // 由 fit_canvas 写成 1/k
            ..OrthographicProjection::default_2d()
        }),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, CAMERA_Z),
        OuterCamera,
        HIGH_RES_LAYERS,
    ));
}

/// 计算整数放大倍数 `k = max(MIN_CANVAS_SCALE, floor(min(w / VIEWPORT_W, h / VIEWPORT_H)))`
/// 并写到外层相机的 `scale = 1 / k`。
///
/// `ScalingMode::WindowSize` + `scale = s` 时视图覆盖的世界范围 = 窗口逻辑尺寸 × `s`；
/// 要让 512 世界单位占满 `k × 512` 物理像素，就需要 `s = 1 / k`（`07` §7.2.2 推导）。
fn fit_canvas(
    mut resized: MessageReader<WindowResized>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut outer: Single<&mut Projection, With<OuterCamera>>,
    mut canvas_scale: ResMut<CanvasScale>,
) {
    // 优先用 resize 消息；启动首帧没有消息时回落到当前窗口尺寸
    let mut size = window.size();
    for ev in resized.read() {
        size = Vec2::new(ev.width, ev.height);
    }

    let k = (size.x / VIEWPORT_W as f32)
        .min(size.y / VIEWPORT_H as f32)
        .floor()
        .max(MIN_CANVAS_SCALE as f32) as u32;

    // 只在变化时写 Projection，避免每帧触发投影重算与 change detection 噪声
    if **canvas_scale == k {
        return;
    }
    canvas_scale.0 = k;

    let Projection::Orthographic(p) = &mut **outer else {
        return;
    };
    p.scale = 1.0 / k as f32;
}
