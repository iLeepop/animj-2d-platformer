//! 双层相机像素完美管线。所有者文档：`docs/07-rendering.md` §7.2。
//!
//! 结构：内层相机把**一屏视野**（`VIEWPORT_W × VIEWPORT_H`，小于一个房间）画进同尺寸离屏纹理
//! （1 世界单位 = 1 texel），视口随玩家在地图上移动；外层相机把该纹理铺在原点并按**整数倍** `k`
//! 放大到窗口，四周是清屏色（黑边）。

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode, Viewport};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::ui::UiScale;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::config::render::{
    CAMERA_Z, CANVAS_FORMAT, INNER_CAMERA_ORDER, MIN_CANVAS_SCALE, OUTER_CAMERA_ORDER,
    UI_CAMERA_ORDER, VIEWPORT_H, VIEWPORT_W, z,
};
use crate::render::snap::PixelSnap;

/// 内层相机只渲染这一层（所有游戏内 `Sprite` / `Text2d`）。
pub const PIXEL_PERFECT_LAYERS: RenderLayers = RenderLayers::layer(0);
/// 外层相机只渲染这一层（承载离屏纹理的那张 canvas sprite）。
pub const HIGH_RES_LAYERS: RenderLayers = RenderLayers::layer(1);

/// 渲染一屏内容的游戏内层相机（跟随玩家，见 `02` §2.2）。
#[derive(Component)]
pub struct InGameCamera;

/// 把离屏纹理放大到窗口的外层相机。
#[derive(Component)]
pub struct OuterCamera;

/// 只渲染屏幕空间 UI 的相机（目标 = 主窗口）。
///
/// 为什么 UI 不跟内层相机走：内层目标是 320×320 离屏纹理，UI 会被整数倍放大，文字先按 12 px
/// 光栅化再放大 ⇒ 看起来就是「低分辨率文本放大」。更关键的是 `bevy_picking` 的 UI 后端只把
/// **与窗口同目标**的相机纳入命中测试；内层相机目标是一张图片，指针事件永远匹配不上 ⇒ 按钮点不动。
/// 因此 UI 单独用一台窗口相机，并按 `k` 设置 `UiScale`，既保持 320 坐标布局，又按原生分辨率光栅化。
#[derive(Component)]
pub struct UiCamera;

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
        // 视口必须是正方形：像素完美管线的整数缩放按单一边长推导（07 §7.1.1）。
        assert_eq!(
            VIEWPORT_W, VIEWPORT_H,
            "视口必须是正方形，见 docs/07-rendering.md §7.1.1"
        );

        app.init_resource::<CanvasScale>()
            // 黑边 = 清屏色；这样「视口外区域」只有一种颜色
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_pixel_camera)
            .add_systems(Update, (fit_canvas, sync_ui_scale).chain());
    }
}

fn setup_pixel_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // ① 离屏渲染目标：一屏视野（小于一个房间）
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

    // ⑤ UI 相机：目标 = 主窗口（不是离屏纹理）。
    //    这样 `bevy_picking` 才能把指针事件匹配到 UI 所属相机（其 `RenderTarget` 与指针的
    //    `NormalizedRenderTarget` 同为 `Window`），按钮才会响应点击。`RenderLayers::none()`
    //    确保它不重复渲染世界；`clear_color: None` 确保它不擦掉外层相机画好的 canvas。
    commands.spawn((
        Camera2d,
        Camera {
            order: UI_CAMERA_ORDER,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection::default_2d()),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, CAMERA_Z),
        RenderLayers::none(),
        IsDefaultUiCamera,
        UiCamera,
    ));
}

/// 计算整数放大倍数 `k = max(MIN_CANVAS_SCALE, floor(min(w / VIEWPORT_W, h / VIEWPORT_H)))`
/// 并写到外层相机的 `scale = 1 / k`；同时把 UI 相机的视口限定到**居中游戏区域**。
///
/// `ScalingMode::WindowSize` + `scale = s` 时视图覆盖的世界范围 = 窗口逻辑尺寸 × `s`；
/// 要让视口的世界范围（= `VIEWPORT_W` 世界单位）占满 `k × VIEWPORT_W` 物理像素，就需要 `s = 1 / k`（`07` §7.2.2 推导）。
///
/// UI 相机如果仍以整个窗口为视口：全屏 1920×1080、`k = 3` 时 UI 逻辑尺寸是 640×360，
/// 而菜单按 320×320 写 ⇒ 只占左上角。把 UI 视口设为居中的 `k*VIEWPORT` 正方形后，
/// 逻辑尺寸恒为 320×320，菜单与游戏区域严格重合（黑边留在两侧）。
fn fit_canvas(
    mut resized: MessageReader<WindowResized>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut outer: Single<&mut Projection, With<OuterCamera>>,
    mut ui_camera: Single<&mut Camera, (With<UiCamera>, Without<OuterCamera>)>,
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

    // UI 视口 = 游戏画布在窗口里的整数像素矩形（居中）。全屏/改变窗口尺寸都必须跟着走。
    let canvas_px = VIEWPORT_W * k;
    let viewport = Viewport {
        physical_position: UVec2::new(
            (size.x.round().max(0.0) as u32).saturating_sub(canvas_px) / 2,
            (size.y.round().max(0.0) as u32).saturating_sub(canvas_px) / 2,
        ),
        physical_size: UVec2::splat(canvas_px),
        depth: 0.0..1.0,
    };
    let unchanged = ui_camera.viewport.as_ref().is_some_and(|current| {
        current.physical_position == viewport.physical_position
            && current.physical_size == viewport.physical_size
            && current.depth == viewport.depth
    });
    if !unchanged {
        ui_camera.viewport = Some(viewport);
    }

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

/// 让 UI 的逻辑坐标继续是 `VIEWPORT_W × VIEWPORT_H`。
///
/// UI 相机的视口是居中的游戏区域，物理尺寸 = `k × VIEWPORT`。
///
/// `UiScale = k` 时 `logical_size = 视口物理尺寸 / k = 320×320`，且 `Val::Px` / `FontSize::Px`
/// 都乘 `k` ⇒ 布局与旧的 320 内层方案一致，但字形按 `k` 倍原生分辨率光栅化（不再放大低分辨率文本）。
fn sync_ui_scale(canvas_scale: Res<CanvasScale>, mut ui_scale: ResMut<UiScale>) {
    let target = canvas_scale.0 as f32;
    if ui_scale.0 != target {
        ui_scale.0 = target;
    }
}
