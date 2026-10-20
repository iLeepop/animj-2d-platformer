//! 双层相机像素完美管线。所有者文档：`docs/07-rendering.md` §7.2。
//!
//! 结构：每个玩家视图一台**内层相机**，把一屏视野（`VIEWPORT_W × VIEWPORT_H`，小于一个房间）
//! 画进自己的同尺寸离屏纹理（1 世界单位 = 1 texel）。外层相机把 1~2 张纹理按整数倍 `k`
//! 并排铺到窗口，四周是清屏色（黑边）。
//!
//! 视图数量由 `ViewLayout` 决定：单人/主机/客户端 = 1，本地双人 = 2（左右分屏）。
//! 每个视图的相机跟随自己的玩家（`camera::rig`）。

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode, Viewport};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::ui::UiScale;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::config::render::{
    CAMERA_Z, CANVAS_FORMAT, INNER_CAMERA_ORDER, MIN_CANVAS_SCALE, OUTER_CAMERA_ORDER,
    UI_CAMERA_ORDER, VIEWPORT_H, VIEWPORT_W, WINDOW_H, WINDOW_H_SPLIT, WINDOW_W, WINDOW_W_SPLIT, z,
};
use crate::render::snap::PixelSnap;
use crate::state::{GameState, SessionMode};

/// 内层相机只渲染这一层（所有游戏内 `Sprite` / `Text2d`）。
pub const PIXEL_PERFECT_LAYERS: RenderLayers = RenderLayers::layer(0);
/// 外层相机只渲染这一层（承载离屏纹理的那些 canvas sprite）。
pub const HIGH_RES_LAYERS: RenderLayers = RenderLayers::layer(1);
/// 视差背景的每视图专属层：视图 `i` 用 `PARALLAX_LAYER_BASE + i`。
///
/// 每个内层相机 = `[PIXEL_PERFECT_LAYERS, 自己的视差层]`，因此两个视图可以拥有
/// 各自相位的视差背景，而世界实体仍只占一层（`07` §7.13.5）。
pub const PARALLAX_LAYER_BASE: usize = 8;

/// 最多几个本地视图（本地双人 = 2；联机每个进程只有 1 个本地视图）。
pub const MAX_VIEWS: usize = 2;

/// 本局需要的玩家视图数量。`LocalCoop = 2`，其它模式 = 1。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewLayout {
    pub views: u8,
}

impl Default for ViewLayout {
    fn default() -> Self {
        Self { views: 1 }
    }
}

impl ViewLayout {
    pub fn canvas_size(self) -> Vec2 {
        Vec2::new((VIEWPORT_W * self.views as u32) as f32, VIEWPORT_H as f32)
    }

    pub fn width(self) -> f32 {
        self.canvas_size().x
    }

    pub fn height(self) -> f32 {
        self.canvas_size().y
    }
}

/// 内层相机的视图编号（`0..ViewLayout.views`）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerView(pub u8);

/// 本帧相机跟随的**连续**中心（未像素吸附）。
///
/// 相机 `Transform` 会在 `GameSet::Snap` 里被取整；视差背景的相位必须用这个连续值
/// （取模后再取整），不能再用已取整的 `Transform` 二次取整——否则相机的整数步进
/// 会让背景偶发 2 px 跳步（`07` §7.13.2）。
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraAnchor(pub Vec2);

/// 渲染一屏内容的游戏内层相机（每个视图一台）。
#[derive(Component)]
pub struct InGameCamera;

/// 把离屏纹理放大到窗口的外层相机。
#[derive(Component)]
pub struct OuterCamera;

/// 只渲染屏幕空间 UI 的相机（目标 = 主窗口）。
#[derive(Component)]
pub struct UiCamera;

/// 承载某个视图离屏纹理的 sprite。
#[derive(Component)]
pub struct Canvas {
    pub view: u8,
}

/// 当前整数放大倍数 `k`。HUD、dev 覆盖层、截图验证都读它。
#[derive(Resource, Default, Deref, DerefMut)]
pub struct CanvasScale(pub u32);

/// 全部视图的离屏渲染目标句柄（索引 = 视图号）。
#[derive(Resource, Clone)]
pub struct CanvasImage {
    pub views: Vec<Handle<Image>>,
}

pub struct PixelCameraPlugin;

impl Plugin for PixelCameraPlugin {
    fn build(&self, app: &mut App) {
        // 视口必须是正方形：每个视图的整数缩放按单一边长推导（07 §7.1.1）。
        assert_eq!(
            VIEWPORT_W, VIEWPORT_H,
            "视口必须是正方形，见 docs/07-rendering.md §7.1.1"
        );

        app.init_resource::<CanvasScale>()
            // 由 `NetPlugin` 初始化；这里兜底（单独用 PixelRenderPlugin 的测试/工具）。
            .init_resource::<ViewLayout>()
            // 黑边 = 清屏色；这样「视口外区域」只有一种颜色
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_pixel_camera)
            .add_systems(
                Update,
                (
                    sync_view_layout,
                    fit_window_for_layout,
                    apply_view_activation,
                    fit_canvas,
                    sync_ui_scale,
                )
                    .chain(),
            );
    }
}

/// `SessionMode` 决定视图数；布局变化时相机与 canvas 在同帧重新排布。
fn sync_view_layout(
    mode: Res<SessionMode>,
    state: Res<State<GameState>>,
    mut layout: ResMut<ViewLayout>,
) {
    // 只有真正进入一局（含暂停/设置/结算）才分屏；主菜单/关卡选择保持单视图，
    // 否则点「本地双人」后选关界面会先被撑成 640 逻辑宽。
    let in_game = matches!(
        state.get(),
        GameState::Loading
            | GameState::Playing
            | GameState::Paused
            | GameState::Settings
            | GameState::GameComplete
    );
    let views = if in_game && matches!(*mode, SessionMode::LocalCoop) {
        MAX_VIEWS as u8
    } else {
        1
    };
    if layout.views != views {
        layout.views = views;
    }
}

/// 进入/退出本地双人时调整窗口：默认 640×640 → 1280×640，保证两屏都是 2× 整数放大。
/// 全屏或用户已改过窗口尺寸时不动。
fn fit_window_for_layout(
    layout: Res<ViewLayout>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if !layout.is_changed() || window.mode != bevy::window::WindowMode::Windowed {
        return;
    }
    let current = (
        window.resolution.physical_width(),
        window.resolution.physical_height(),
    );
    let target = if layout.views > 1 {
        (WINDOW_W_SPLIT, WINDOW_H_SPLIT)
    } else if current == (WINDOW_W_SPLIT, WINDOW_H_SPLIT) {
        (WINDOW_W, WINDOW_H)
    } else {
        return;
    };
    if current != target {
        window
            .resolution
            .set_physical_resolution(target.0, target.1);
    }
}

/// 未使用的内层相机必须停用，否则会继续渲染（并影响自检的相机计数）。
fn apply_view_activation(layout: Res<ViewLayout>, mut cameras: Query<(&PlayerView, &mut Camera)>) {
    for (view, mut camera) in &mut cameras {
        let active = (view.0 as usize) < layout.views as usize;
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
}

fn setup_pixel_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut canvas_handles = Vec::with_capacity(MAX_VIEWS);
    for _ in 0..MAX_VIEWS {
        // 每个视图一张 320×320 离屏纹理，1 世界单位 = 1 texel。
        let mut image = Image::new_target_texture(VIEWPORT_W, VIEWPORT_H, CANVAS_FORMAT, None);
        // 允许 `Screenshot::image(canvas)` 把它拷回 CPU（M0 的像素完美验证依赖这条读回路径）。
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        canvas_handles.push(images.add(image));
    }
    commands.insert_resource(CanvasImage {
        views: canvas_handles.clone(),
    });

    for (view, handle) in canvas_handles.iter().enumerate() {
        // 内层相机：世界 → 该视图的离屏纹理。
        commands.spawn((
            Camera2d,
            Camera {
                order: INNER_CAMERA_ORDER,
                is_active: view == 0,
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
            RenderTarget::Image(handle.clone().into()),
            Msaa::Off,
            // z 必须为 0.0 —— 见 `config::render::CAMERA_Z`（远景视差在 -100）。
            Transform::from_xyz(0.0, 0.0, CAMERA_Z),
            InGameCamera,
            PlayerView(view as u8),
            CameraAnchor::default(),
            RenderLayers::from_layers(&[0, PARALLAX_LAYER_BASE + view]),
            crate::render::parallax::ParallaxCamera,
            PixelSnap,
        ));

        // 把该视图中立地铺进「高分辨率世界」；`fit_canvas` 决定位置与可见性。
        commands.spawn((
            Sprite::from_image(handle.clone()),
            Canvas { view: view as u8 },
            HIGH_RES_LAYERS,
            Transform::from_xyz(0.0, 0.0, z::CANVAS),
            if view == 0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }

    // 外层相机：离屏纹理 → 窗口（整数倍放大 + 黑边）。
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

    // UI 相机：目标 = 主窗口（不是离屏纹理）。
    // 这样 `bevy_picking` 才能把指针事件匹配到 UI 所属相机，按钮才会响应点击。
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

/// 计算整数放大倍数 `k = max(MIN_CANVAS_SCALE, floor(min(w / layout_w, h / layout_h)))`，
/// 写到外层相机 `scale = 1 / k`；把各视图 canvas 并排居中；UI 视口限定到整个游戏区域。
fn fit_canvas(
    mut resized: MessageReader<WindowResized>,
    layout: Res<ViewLayout>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut outer: Single<&mut Projection, With<OuterCamera>>,
    mut ui_camera: Single<&mut Camera, (With<UiCamera>, Without<OuterCamera>)>,
    mut canvas_scale: ResMut<CanvasScale>,
    mut canvases: Query<(&Canvas, &mut Transform, &mut Visibility)>,
) {
    // 优先用 resize 消息；启动首帧没有消息时回落到当前窗口尺寸
    let mut size = window.size();
    for ev in resized.read() {
        size = Vec2::new(ev.width, ev.height);
    }

    let canvas_size = layout.canvas_size();
    let k = (size.x / canvas_size.x)
        .min(size.y / canvas_size.y)
        .floor()
        .max(MIN_CANVAS_SCALE as f32) as u32;

    // 各视图并排、整体居中。
    for (canvas, mut transform, mut visibility) in &mut canvases {
        let x = (canvas.view as f32 - (layout.views as f32 - 1.0) / 2.0) * VIEWPORT_W as f32;
        if transform.translation.x != x || transform.translation.y != 0.0 {
            transform.translation.x = x;
            transform.translation.y = 0.0;
        }
        let want = if canvas.view < layout.views {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
    }

    // UI 视口 = 游戏区域在窗口里的整数像素矩形（居中）。
    let canvas_px = UVec2::new(VIEWPORT_W * k * layout.views as u32, VIEWPORT_H * k);
    let viewport = Viewport {
        physical_position: UVec2::new(
            (size.x.round().max(0.0) as u32).saturating_sub(canvas_px.x) / 2,
            (size.y.round().max(0.0) as u32).saturating_sub(canvas_px.y) / 2,
        ),
        physical_size: canvas_px,
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

/// 让 UI 的逻辑坐标继续是 `ViewLayout` 的尺寸（单人仍是 `320×320`）。
///
/// UI 相机视口物理尺寸 = `k × layout`；`UiScale = k` 时 `logical_size = 视口 / k = layout`，
/// 且 `Val::Px` / `FontSize::Px` 都乘 `k` ⇒ 字形按原生分辨率光栅化。
fn sync_ui_scale(canvas_scale: Res<CanvasScale>, mut ui_scale: ResMut<UiScale>) {
    let target = canvas_scale.0 as f32;
    if ui_scale.0 != target {
        ui_scale.0 = target;
    }
}
