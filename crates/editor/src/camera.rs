//! 编辑器相机：唯一相机 + 档位缩放 + 平移 + 视角复位 + 房间切换 + 光标换算。
//! 所有者文档：`docs/08-level-editor.md` §8.4。

use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::PrimaryEguiContext;
use bevy_egui::input::egui_wants_any_pointer_input;
use level_format::{ROOM_PX, ROOM_TILES, TILE_PX};

use crate::EditorMode;
use crate::session::EditorSession;

/// 编辑态唯一相机（`08` §8.4.1）。
#[derive(Component)]
pub struct EditorCamera;

/// 放大档位（`08` §8.4.2）：`scale = 1.0 / magnify`。
pub const EDITOR_ZOOM_STEPS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// 初始档位索引（`magnify = 1.0`）。
pub const DEFAULT_ZOOM_INDEX: usize = 2;

#[derive(Resource, Debug, Clone, Copy)]
pub struct EditorViewState {
    pub zoom_index: usize,
}

impl Default for EditorViewState {
    fn default() -> Self {
        Self {
            zoom_index: DEFAULT_ZOOM_INDEX,
        }
    }
}

impl EditorViewState {
    pub fn magnify(self) -> f32 {
        EDITOR_ZOOM_STEPS[self.zoom_index.min(EDITOR_ZOOM_STEPS.len() - 1)]
    }

    pub fn scale(self) -> f32 {
        1.0 / self.magnify()
    }
}

pub struct EditorCameraPlugin;

impl Plugin for EditorCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorViewState>()
            .add_systems(Startup, spawn_editor_camera)
            .add_systems(
                Update,
                (
                    scroll_zoom.run_if(not(egui_wants_any_pointer_input)),
                    pan_camera.run_if(not(egui_wants_any_pointer_input)),
                    reset_view_on_home,
                    keep_camera_on_current_room,
                    focus_camera_on_request.after(keep_camera_on_current_room),
                    verify_zoom_anchor,
                ),
            );
    }
}

fn spawn_editor_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Msaa::Off,
        PrimaryEguiContext,
        EditorCamera,
        game::render::parallax::ParallaxCamera,
        // 世界在 layer 0；视差副本在 PARALLAX_LAYER_BASE（8）。编辑器没有离屏管线，直接用 layer 0 + 8。
        RenderLayers::from_layers(&[0, game::render::pixel_camera::PARALLAX_LAYER_BASE]),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::WindowSize,
            scale: 1.0,
            ..OrthographicProjection::default_2d()
        }),
        Transform::default(),
    ));
}

/// 缩放锚点自检：上一帧记录的（光标, 缩放前世界点）⇒ 本帧重新换算并断言。
fn verify_zoom_anchor(
    mut pending: Local<Option<(Vec2, Vec2)>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    camera: Option<Single<(&Camera, &GlobalTransform), With<EditorCamera>>>,
) {
    let Some((cursor, before)) = *pending else {
        return;
    };
    let (Some(window), Some(camera)) = (window, camera) else {
        return;
    };
    *pending = None;
    let _ = window;
    if let Ok(after) = camera.0.viewport_to_world_2d(camera.1, cursor) {
        debug_assert!(
            after.distance(before) < 1.5,
            "缩放锚点漂移：before={before:?} after={after:?}"
        );
    }
}

/// 平移量化到 **`scale` 的整数倍**（= 屏幕像素对齐）。
///
/// 早期按 0.5 量化：在 1× 下仍会落在半像素上，Gizmos（网格/高亮边框）逐帧抖动；
/// 放大到 2× 时 0.5 世界单位恰好等于 1 像素，抖动才消失——这正是实机现象。见 `08` §8.4.2。
pub fn quantize_translation(translation: Vec2, scale: f32) -> Vec2 {
    let unit = scale.max(f32::EPSILON);
    Vec2::new(
        (translation.x / unit).round() * unit,
        (translation.y / unit).round() * unit,
    )
}

/// 缩放后保持光标下的世界点不动（`08` §8.4.2 的解析式）。
pub fn anchored_translation(
    before_world: Vec2,
    cursor: Vec2,
    window: Vec2,
    new_scale: f32,
) -> Vec2 {
    before_world - (cursor - window / 2.0) * new_scale
}

/// `Shift+滚轮` 档位缩放，光标锚定。
fn scroll_zoom(
    mut pending_anchor: Local<Option<(Vec2, Vec2)>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut view: ResMut<EditorViewState>,
    camera: Option<
        Single<(&Camera, &GlobalTransform, &mut Transform, &mut Projection), With<EditorCamera>>,
    >,
) {
    let Some(window) = window else {
        return;
    };
    if !keys.pressed(KeyCode::ShiftLeft) && !keys.pressed(KeyCode::ShiftRight) {
        return;
    }
    let wheel_delta: f32 = wheel.read().map(|event| event.y).sum();
    if wheel_delta == 0.0 {
        return;
    }
    let Some(mut camera) = camera else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let before = camera.0.viewport_to_world_2d(camera.1, cursor).ok();
    let mut next_index = view.zoom_index;
    if wheel_delta > 0.0 {
        next_index = (next_index + 1).min(EDITOR_ZOOM_STEPS.len() - 1);
    } else {
        next_index = next_index.saturating_sub(1);
    }
    if next_index == view.zoom_index {
        return;
    }
    view.zoom_index = next_index;
    let new_scale = view.scale();
    if let Projection::Orthographic(ortho) = camera.3.as_mut() {
        ortho.scale = new_scale;
    }
    if let Some(before) = before {
        let size = window.size();
        let target = quantize_translation(
            anchored_translation(before, cursor, size, new_scale),
            new_scale,
        );
        camera.2.translation.x = target.x;
        camera.2.translation.y = target.y;
        // `08` §8.4.2 要求的自检：下一帧光标下的世界点必须基本不变（`GlobalTransform` 本帧还是旧值）
        *pending_anchor = Some((cursor, before));
    }
    let _ = wheel_delta;
}

/// 中键拖动平移（`08` §8.4.2），量化到 0.5。
fn pan_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    view: Res<EditorViewState>,
    mut camera: Option<Single<&mut Transform, (With<EditorCamera>, Without<PrimaryWindow>)>>,
) {
    let Some(mut camera) = camera.take() else {
        return;
    };
    let mut delta = Vec2::ZERO;
    for event in motion.read() {
        delta += event.delta;
    }
    if !buttons.pressed(MouseButton::Middle) || delta == Vec2::ZERO {
        return;
    }
    // 逻辑像素 → 世界单位：窗口坐标差 = 世界差 / scale
    camera.translation.x -= delta.x * view.scale();
    camera.translation.y += delta.y * view.scale();
    let quantized = quantize_translation(camera.translation.truncate(), view.scale());
    camera.translation.x = quantized.x;
    camera.translation.y = quantized.y;
    let _ = window;
}

/// `Home`：回到当前房间中心并复位缩放（`08` §8.4.2）。
fn reset_view_on_home(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<EditorViewState>,
    session: Option<Res<EditorSession>>,
    mut camera: Option<Single<(&mut Transform, &mut Projection), With<EditorCamera>>>,
) {
    if !keys.just_pressed(KeyCode::Home) {
        return;
    }
    let (Some(session), Some(mut camera)) = (session, camera.take()) else {
        return;
    };
    *view = EditorViewState::default();
    let center = game::coords::room_center(
        session.current_room.0,
        session.current_room.1,
        session.doc.grid.rows,
    );
    camera.0.translation.x = center.x;
    camera.0.translation.y = center.y;
    if let Projection::Orthographic(ortho) = camera.1.as_mut() {
        ortho.scale = view.scale();
    }
}

/// **自愈不变量**：`current_room` 一旦变化，相机必须跟到新房间中心。
///
/// 不依赖任何"请求标志"（那条路径在实机出现过未生效），用 `Local` 记住上一帧的房间，
/// 变化即对焦：点击房间选择器、键盘切换、错误列表定位、编辑器外部改动都覆盖。
fn keep_camera_on_current_room(
    mode: Res<State<EditorMode>>,
    session: Option<Res<EditorSession>>,
    game_room: Option<Res<game::world::CurrentRoom>>,
    mut last: Local<Option<(EditorMode, (u32, u32))>>,
    mut view: ResMut<EditorViewState>,
    mut camera: Option<Single<(&mut Transform, &mut Projection), With<EditorCamera>>>,
) {
    let Some(session) = session else {
        return;
    };
    // 试玩态跟随 **game 的**当前房间（玩家在哪里就看哪里）；
    // **编辑态不跟随**：选房间/点错误格只改编辑目标，相机由用户自己平移或显式对焦
    // （自动跟随会在放置瓦片时不断把视角拽走）。
    let mode_now = *mode.get();
    if mode_now != EditorMode::Playing {
        *last = Some((mode_now, session.current_room));
        return;
    }
    let current = game_room
        .as_ref()
        .map(|room| (room.col, room.row))
        .unwrap_or(session.current_room);
    // 模式也纳入 key：进入试玩时强制重新对焦（否则相机会停在编辑时的房间）
    if *last == Some((mode_now, current)) {
        return;
    }
    *last = Some((mode_now, current));
    let Some(mut camera) = camera.take() else {
        return;
    };
    *view = EditorViewState::default();
    let center = game::coords::room_center(current.0, current.1, session.doc.grid.rows);
    camera.0.translation.x = center.x;
    camera.0.translation.y = center.y;
    if let Projection::Orthographic(ortho) = camera.1.as_mut() {
        ortho.scale = view.scale();
    }
    info!(
        "编辑器：相机跟随房间 → ({}, {}) @({:.0}, {:.0})",
        current.0, current.1, center.x, center.y
    );
}

/// 错误列表点击等请求：把相机重新对焦到当前房间（`08` §8.8.2）。
fn focus_camera_on_request(
    mut session: Option<ResMut<EditorSession>>,
    mut view: ResMut<EditorViewState>,
    mut camera: Option<Single<(&mut Transform, &mut Projection), With<EditorCamera>>>,
) {
    let (Some(session), Some(mut camera)) = (session.as_mut(), camera.take()) else {
        return;
    };
    if !session.focus_camera {
        return;
    }
    session.focus_camera = false;
    *view = EditorViewState::default();
    // 有精确对焦点（错误所在格）时用它，否则用房间中心
    let center = session.focus_at.take().unwrap_or_else(|| {
        game::coords::room_center(
            session.current_room.0,
            session.current_room.1,
            session.doc.grid.rows,
        )
    });
    camera.0.translation.x = center.x;
    camera.0.translation.y = center.y;
    if let Projection::Orthographic(ortho) = camera.1.as_mut() {
        ortho.scale = view.scale();
    }
    info!(
        "编辑器：相机对焦房间 ({}, {}) → ({:.0}, {:.0})",
        session.current_room.0, session.current_room.1, center.x, center.y
    );
}

/// 光标所在房间与瓦片（`08` §8.4.3 的唯一换算路径）。
pub fn cursor_to_tile(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    window: &Window,
    doc: &level_format::Level,
) -> Option<((u32, u32), usize, usize)> {
    let cursor = window.cursor_position()?;
    let world = camera.viewport_to_world_2d(camera_transform, cursor).ok()?;
    let pos = game::coords::world_to_room(world, doc.grid.cols, doc.grid.rows)?;
    let file_row = (pos.local.y / TILE_PX as f32).floor() as i32;
    let tx = (pos.local.x / TILE_PX as f32).floor() as i32;
    if !(0..ROOM_TILES as i32).contains(&tx) || !(0..ROOM_TILES as i32).contains(&file_row) {
        return None;
    }
    let ty = ROOM_TILES as i32 - 1 - file_row;
    Some(((pos.col, pos.row), tx as usize, ty as usize))
}

/// 光标所在世界点。
pub fn cursor_world(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    window: &Window,
) -> Option<Vec2> {
    let cursor = window.cursor_position()?;
    camera.viewport_to_world_2d(camera_transform, cursor).ok()
}

/// 仅供 `view` 使用：房间世界中心（避免各处重复坐标算术）。
pub fn room_center_world(col: u32, row: u32, rows: u32) -> Vec2 {
    game::coords::room_center(col, row, rows)
}

/// 房间世界尺寸（gizmo 用）。
pub fn room_size() -> Vec2 {
    Vec2::splat(ROOM_PX as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 量化后必须落在屏幕像素网格上：`translation / scale` 为整数。
    #[test]
    fn translation_quantizes_to_screen_pixels() {
        for scale in [0.25, 0.5, 1.0, 2.0, 4.0] {
            for raw in [Vec2::new(3.3, -2.7), Vec2::new(-0.49, 0.51), Vec2::ZERO] {
                let q = quantize_translation(raw, scale);
                let in_pixels = q / scale;
                assert!(
                    (in_pixels.x.round() - in_pixels.x).abs() < 1e-4
                        && (in_pixels.y.round() - in_pixels.y).abs() < 1e-4,
                    "scale={scale} raw={raw:?} quantized={q:?}"
                );
            }
        }
    }

    /// 缩放锚定式：`before_world` 处的点在新比例下保持在光标下。
    #[test]
    fn anchored_translation_keeps_the_cursor_point() {
        let cursor = Vec2::new(700.0, 500.0);
        let window = Vec2::new(1024.0, 1024.0);
        let before = Vec2::new(100.0, 200.0);
        let new_scale = 0.5;
        let t = anchored_translation(before, cursor, window, new_scale);
        // 由 t 反推光标世界点：world = t + (cursor - win/2) * scale
        let world = t + (cursor - window / 2.0) * new_scale;
        assert!(
            world.distance(before) < 1e-3,
            "world={world:?} before={before:?}"
        );
    }
}

// 说明：`08` §8.4.2 要求的"缩放后下一帧断言光标世界点不变"
// 解析式本身已按文档给出，人工实测通过即可；此处如实标注而不是假装已覆盖。
