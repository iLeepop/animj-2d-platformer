//! 编辑工具与世界输入。所有者文档：`docs/08-level-editor.md` §8.4.4、§8.5。
//!
//! 输入竞争：无拖拽时受 `egui_wants_any_pointer_input` 门控；**拖拽一旦开始就归编辑器所有**
//! （中途划过面板不丢操作），直到鼠标释放或 `Esc` 取消。

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use level_format::{ROOM_TILES, TILE_PX};

use crate::EditorMode;
use crate::camera::{EditorCamera, cursor_to_tile, cursor_world};
use crate::session::{EditorSession, Tool};
use crate::view::EditorViewFlags;

/// 一次拖拽（一个 undo 步骤）。
#[derive(Debug, Clone)]
struct DragOp {
    tool: Tool,
    room: (u32, u32),
    last: (usize, usize),
    /// 按下时已经 `begin_edit`，取消时要回滚。
    pushed_snapshot: bool,
    /// 工具 6：被拖动的实体下标。
    entity: Option<usize>,
    /// 瓦片工具的按下格（`Shift` 直线/正方形约束的锚点）。
    start: (usize, usize),
}

#[derive(Resource, Default)]
pub struct EditorDrag {
    active: Option<DragOp>,
}

/// UI 的「试玩」按钮请求（由 `playtest` 消费）。
#[derive(Resource, Default)]
pub struct PlaytestRequest(pub bool);

pub struct EditorToolsPlugin;

impl Plugin for EditorToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorDrag>()
            .init_resource::<PlaytestRequest>()
            // ★ 关键：世界输入必须排在 egui **构建完本帧 UI 之后**，并用**同一帧**的
            //   `ctx.egui_wants_pointer_input()` 判定。放在 `Update` 里时读的是上一帧的 UI 布局，
            //   「鼠标移到按钮上并立刻点击」会漏到画布（实机：按钮要点两次才生效）。
            .add_systems(
                EguiPrimaryContextPass,
                (world_input, shortcuts)
                    .chain()
                    .in_set(crate::EditorUiSet::WorldInput)
                    .run_if(in_state(EditorMode::Editing)),
            );
    }
}

fn brush_of(session: &EditorSession, tool: Tool) -> char {
    match tool {
        Tool::Eraser => '.',
        _ => session.brush_char,
    }
}

/// 快捷键（`08` §8.9 的首批条目）。
fn shortcuts(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut session: Option<ResMut<EditorSession>>,
    mut flags: ResMut<EditorViewFlags>,
    mut drag: ResMut<EditorDrag>,
) {
    let Some(session) = session.as_mut() else {
        return;
    };
    // 焦点在 egui 文本输入里时，快捷键全部让路
    if contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_keyboard_input())
        .unwrap_or(false)
    {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ];
    for (index, key) in digits.iter().enumerate() {
        if keys.just_pressed(*key) {
            session.tool = Tool::ALL[index];
        }
    }

    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        session.undo();
    }
    if ctrl && keys.just_pressed(KeyCode::KeyY) {
        session.redo();
    }
    if ctrl && keys.just_pressed(KeyCode::KeyS) {
        session.status = match session.save() {
            Ok(()) => "已保存".to_string(),
            Err(error) => format!("保存失败：{error}"),
        };
    }
    if keys.just_pressed(KeyCode::KeyG) {
        if shift {
            flags.borders = !flags.borders;
        } else {
            flags.grid = !flags.grid;
        }
    }
    // 切房间：`Tab`/`Shift+Tab` 之外，**必须**提供不被 egui 焦点遍历吞掉的键（`[` / `]`）。
    // 实机：只按 Tab 时切房间不生效 ⇒ 相机不动 ⇒ 叠加层"永远只在房间 1"。
    let cycle = if keys.just_pressed(KeyCode::BracketLeft) {
        -1
    } else if keys.just_pressed(KeyCode::BracketRight) {
        1
    } else if keys.just_pressed(KeyCode::Tab) {
        if shift { -1 } else { 1 }
    } else {
        0
    };
    if cycle != 0 {
        let cols = session.doc.grid.cols;
        let rows = session.doc.grid.rows;
        let total = cols * rows;
        if total > 0 {
            let current = session.current_room.1 * cols + session.current_room.0;
            let next = ((current as i64 + cycle as i64).rem_euclid(total as i64)) as u32;
            session.current_room = (next % cols, next / cols);
            // 强制对焦（camera 的 `focus_camera_on_request` 会处理）
            session.focus_camera = true;
        }
    }

    if keys.just_pressed(KeyCode::Escape)
        && let Some(op) = drag.active.take()
        && op.pushed_snapshot
    {
        session.cancel_last_edit();
        session.status = "已取消本次拖拽".to_string();
    }
}

/// 世界区鼠标输入（笔刷 / 矩形 / 吸管 / 放实体）。
fn world_input(
    mut contexts: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    wants: Res<EguiWantsInput>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    camera: Option<Single<(&Camera, &GlobalTransform), With<EditorCamera>>>,
    mut session: Option<ResMut<EditorSession>>,
    mut drag: ResMut<EditorDrag>,
) {
    let (Some(window), Some(camera), Some(mut session)) = (window, camera, session.as_mut()) else {
        return;
    };
    let hovered = cursor_to_tile(camera.0, camera.1, &window, &session.doc);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let pressed = buttons.just_pressed(MouseButton::Left);
    let released = buttons.just_released(MouseButton::Left);

    // ---- 进行中的拖拽：所有权在编辑器，忽略 egui ----
    if let Some(op) = drag.active.clone() {
        // 工具 6：实时更新实体位置（吸附到瓦片中心，`Ctrl` 关闭）
        if op.tool == Tool::MoveEntity
            && let (Some(index), Some(world)) =
                (op.entity, cursor_world(camera.0, camera.1, &window))
        {
            if let Some(pos) = cursor_to_tile(camera.0, camera.1, &window, &session.doc) {
                if pos.0 == op.room {
                    session.set_entity_at_world(index, world, !ctrl);
                } else {
                    // 跨房间拖动 ⇒ 取消并回滚（`08` §8.5.3）
                    session.cancel_last_edit();
                    drag.active = None;
                    session.status = "跨房间拖动已取消".to_string();
                    return;
                }
            }
        }
        if released {
            if op.tool == Tool::RectFill {
                // 矩形填充在释放时一次成型（一个 undo 步骤）
                if let Some((room, tx, ty)) = hovered
                    && room == op.room
                {
                    session.begin_edit();
                    let (mut tx, mut ty) = (tx, ty);
                    if shift {
                        // `Shift`：约束为正方形（边长取两轴较小者）
                        let (sx, sy) = op.start;
                        let size = (tx as isize - sx as isize)
                            .abs()
                            .min((ty as isize - sy as isize).abs());
                        tx = (sx as isize + size * if tx >= sx { 1 } else { -1 }) as usize;
                        ty = (sy as isize + size * if ty >= sy { 1 } else { -1 }) as usize;
                    }
                    let (x0, x1) = (op.last.0.min(tx), op.last.0.max(tx));
                    let (y0, y1) = (op.last.1.min(ty), op.last.1.max(ty));
                    let kind = session.brush_char;
                    for y in y0..=y1 {
                        for x in x0..=x1 {
                            session.set_tile_char(x, y, kind);
                        }
                    }
                }
            }
            drag.active = None;
            return;
        }
        if let Some((room, tx, ty)) = hovered
            && room == op.room
            && (tx, ty) != op.last
        {
            let mut op = op;
            match op.tool {
                Tool::TileBrush | Tool::Eraser => {
                    let kind = brush_of(&session, op.tool);
                    // `Shift`：只沿与起点偏差更大的轴延伸（`08` §8.5.3）
                    let target = if shift {
                        let (sx, sy) = op.start;
                        if (tx as isize - sx as isize).abs() >= (ty as isize - sy as isize).abs() {
                            (tx, sy)
                        } else {
                            (sx, ty)
                        }
                    } else {
                        (tx, ty)
                    };
                    for (x, y) in line_cells(op.last, target) {
                        session.set_tile_char(x, y, kind);
                    }
                }
                Tool::RectFill => {}
                _ => {}
            }
            op.last = (tx, ty);
            drag.active = Some(op);
        }
        return;
    }

    // ---- 新拖拽：受 egui 门控（同一帧的 UI 结果 + 资源双保险）----
    let ctx_wants_pointer = contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(true);
    if ctx_wants_pointer || wants.wants_any_pointer_input() || !pressed {
        return;
    }
    let Some((room, tx, ty)) = hovered else {
        return;
    };
    // 在别的房间**起手**：先切换当前房间再编辑（`08` §8.5.3 的"不跨房写入"只约束拖拽中途）。
    // 否则玩家必须在编辑前先按 Tab，表现为"只能编辑第一个房间"。
    if room != session.current_room {
        session.current_room = room;
    }
    let tool = if alt { Tool::Picker } else { session.tool };
    let cursor_world_pos = cursor_world(camera.0, camera.1, &window);
    match tool {
        Tool::MoveEntity => {
            let Some(world) = cursor_world_pos else {
                return;
            };
            let Some(index) = entity_at(&session, world) else {
                return;
            };
            session.begin_edit();
            session.selected_entity = Some(index);
            drag.active = Some(DragOp {
                tool,
                room,
                last: (tx, ty),
                pushed_snapshot: true,
                entity: Some(index),
                start: (tx, ty),
            });
            return;
        }
        Tool::Picker => {
            if let Some(ch) = session.tile_char(tx, ty) {
                session.brush_char = ch;
            }
            return;
        }
        Tool::PlaceEntity => {
            place_entity(&mut session, camera.0, camera.1, &window, ctrl);
            return;
        }
        Tool::TileBrush | Tool::Eraser => {
            session.begin_edit();
            let kind = brush_of(&session, tool);
            session.set_tile_char(tx, ty, kind);
            drag.active = Some(DragOp {
                tool,
                room,
                last: (tx, ty),
                pushed_snapshot: true,
                entity: None,
                start: (tx, ty),
            });
        }
        Tool::RectFill => {
            drag.active = Some(DragOp {
                tool,
                room,
                last: (tx, ty),
                pushed_snapshot: false,
                entity: None,
                start: (tx, ty),
            });
        }
    }
}

/// 光标下的实体（后放的在上层 ⇒ 取最后一个命中）。
pub fn entity_at(session: &EditorSession, world: Vec2) -> Option<usize> {
    let mut hit = None;
    for (index, _room, center, size) in session.entity_boxes() {
        let half = size * 0.5;
        if (world.x - center.x).abs() <= half.x && (world.y - center.y).abs() <= half.y {
            hit = Some(index);
        }
    }
    hit
}

/// 单击放置实体（吸附瓦片中心；`Ctrl` 关闭吸附）。
fn place_entity(
    session: &mut EditorSession,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    window: &Window,
    no_snap: bool,
) {
    let rows = session.doc.grid.rows;
    let local = if no_snap {
        // 关闭吸附：对齐到整世界像素（`08` §8.5.4）
        let Some(world) = cursor_world(camera, camera_transform, window) else {
            return;
        };
        let world = Vec2::new(world.x.round(), world.y.round());
        let Some(pos) = game::coords::world_to_room(world, session.doc.grid.cols, rows) else {
            return;
        };
        pos.local
    } else {
        let Some(((_, _), tx, ty)) = cursor_to_tile(camera, camera_transform, window, &session.doc)
        else {
            return;
        };
        let file_row = ROOM_TILES - 1 - ty;
        Vec2::new(
            tx as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0,
            file_row as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0,
        )
    };
    session.begin_edit();
    let spec = level_format::EntitySpec {
        kind: session.entity_kind.clone(),
        at: (local.x.round() as i32, local.y.round() as i32),
        size: None,
        id: None,
    };
    session.room_mut().entities.push(spec);
    session.selected_entity = Some(session.room().entities.len() - 1);
    session.status = format!("已放置 {}", session.entity_kind);
}

/// 两格之间按 `TILE_PX / 4` 采样的全部格子（含端点、去重）。
fn line_cells(from: (usize, usize), to: (usize, usize)) -> Vec<(usize, usize)> {
    let (fx, fy) = (from.0 as f32, from.1 as f32);
    let (tx, ty) = (to.0 as f32, to.1 as f32);
    let steps = ((tx - fx).abs().max((ty - fy).abs()) * 4.0).ceil() as i32;
    let mut cells = Vec::new();
    for step in 0..=steps.max(1) {
        let t = step as f32 / steps.max(1) as f32;
        let x = (fx + (tx - fx) * t).round() as usize;
        let y = (fy + (ty - fy) * t).round() as usize;
        if cells.last() != Some(&(x, y)) {
            cells.push((x, y));
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_cells_cover_the_segment() {
        let cells = line_cells((0, 0), (4, 0));
        assert_eq!(cells.first(), Some(&(0, 0)));
        assert_eq!(cells.last(), Some(&(4, 0)));
        assert!(cells.len() >= 5, "快速拖动不能漏格：{cells:?}");
        assert_eq!(cells, {
            let mut expected = vec![(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)];
            expected.dedup();
            expected
        });
    }

    #[test]
    fn line_cells_handle_diagonals_without_duplicates() {
        let cells = line_cells((0, 0), (3, 3));
        assert!(cells.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(cells.first(), Some(&(0, 0)));
        assert_eq!(cells.last(), Some(&(3, 3)));
    }
}
