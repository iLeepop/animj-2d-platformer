//! 就地试玩（`08` §8.6）：把 `doc` 注入 `game` 的世界，退出时恢复编辑投影。
//!
//! **可逆性（R1–R5）**：本模块只持有 `Res<EditorSession>`，试玩期间绝不写 `doc`；
//! 进出试玩都不写盘。

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::session::EditorSession;
use crate::tools::PlaytestRequest;
use crate::view::ProjectionState;
use crate::EditorMode;

/// 退出试玩请求（`F5`/`F6` 之外的编程入口：UI 按钮、自动化冒烟）。
#[derive(Resource, Default)]
pub struct PlaytestExitRequest(pub bool);

pub struct EditorPlaytestPlugin;

impl Plugin for EditorPlaytestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaytestExitRequest>()
            .add_systems(
                Update,
                (
                    enter_play.run_if(in_state(EditorMode::Editing)),
                    exit_play
                        .run_if(in_state(EditorMode::Playing))
                        .run_if(not(egui_wants_any_keyboard_input)),
                ),
            )
            // 在 AABB→Transform 投影之后读 Transform，诊断才反映本帧真实绘制位置。
            .add_systems(
                PostUpdate,
                report_play_state
                    .after(game::state::GameSet::Snap)
                    .run_if(in_state(EditorMode::Playing)),
            );
    }
}

/// `F5` 或面板按钮 ⇒ 校验通过才进入试玩。
fn enter_play(
    keys: Res<ButtonInput<KeyCode>>,
    mut request: ResMut<PlaytestRequest>,
    mut session: Option<ResMut<EditorSession>>,
    loaded: Option<Res<game::world::load::LoadedLevel>>,
    sheet: Option<Res<game::player::spawn::PlayerSheet>>,
    anim: Option<Res<game::anim_data::ActiveAnim>>,
    mut current: Option<ResMut<game::world::CurrentRoom>>,
    mut next_state: ResMut<NextState<game::state::GameState>>,
    mut next_mode: ResMut<NextState<EditorMode>>,
    mut commands: Commands,
    players: Query<Entity, With<game::player::PlayerMotion>>,
    projection_entities: Query<Entity, With<crate::view::EditorProjectionRoom>>,
    mut projection: ResMut<ProjectionState>,
) {
    let requested = request.0 || keys.just_pressed(KeyCode::F5);
    if !requested {
        return;
    }
    request.0 = false;
    let (Some(session), Some(loaded), Some(sheet), Some(anim)) =
        (session.as_mut(), loaded, sheet, anim)
    else {
        return;
    };
    session.validate_now(0.0);
    if !session.playable() {
        session.status = format!(
            "试玩被阻断：{} 个 error（`08` §8.6.1 第 2 步）",
            session.error_count()
        );
        return;
    }

    // 把 doc 注入 game 的资源（不写盘）：瓦片/实体/进度都会随 `LoadedLevel` 变化重建
    commands.insert_resource(game::world::load::LoadedLevel {
        level: session.doc.clone(),
        handle: loaded.handle.clone(),
    });
    commands.insert_resource(game::tiles::LevelTiles::from_level(&session.doc));
    commands.insert_resource(game::world::progress::LevelProgress::from_level(
        &session.doc,
    ));
    if let Some(current) = current.as_mut() {
        **current = game::world::CurrentRoom {
            col: session.doc.start.col,
            row: session.doc.start.row,
        };
    }
    // 重放玩家：位置 = 起始房间重生点
    for entity in &players {
        commands.entity(entity).despawn();
    }
    let center = game::world::room_spawn_center(
        &session.doc,
        session.doc.start.col,
        session.doc.start.row,
    );
    game::player::spawn::spawn_player(&mut commands, &sheet, &anim.set, center);

    // 编辑投影让位给 game 的世界：**显式销毁**（否则叠加层仍画在试玩画面上）
    for entity in &projection_entities {
        commands.entity(entity).despawn();
    }
    *projection = ProjectionState::default();
    next_state.set(game::state::GameState::Playing);
    next_mode.set(EditorMode::Playing);
    session.status = "试玩中（编辑缓冲已冻结）".to_string();
}

/// 进入试玩 10 帧后报告：玩家实体 / 变换 / 相机 / 窗口（用于定位"看不见玩家"）。
fn report_play_state(
    mut frames: Local<u32>,
    mut reported: Local<bool>,
    players: Query<(
        &game::player::PlayerMotion,
        &Transform,
        &Sprite,
        Option<&Visibility>,
        Option<&RenderLayers>,
    )>,
    camera: Option<Single<&Transform, With<crate::camera::EditorCamera>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    current: Option<Res<game::world::CurrentRoom>>,
) {
    *frames += 1;
    if *reported || *frames < 10 {
        return;
    }
    *reported = true;
    let cam = camera
        .as_ref()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO);
    let win = window.as_ref().map(|w| w.size()).unwrap_or(Vec2::ZERO);
    info!(
        "试玩诊断：玩家实体 {} 个；相机 ({:.0},{:.0})；窗口 {:.0}x{:.0}；game room {:?}",
        players.iter().count(),
        cam.x,
        cam.y,
        win.x,
        win.y,
        current.as_ref().map(|room| (room.col, room.row))
    );
    for (motion, transform, sprite, visibility, layers) in &players {
        info!(
            "试玩诊断：玩家 aabb center ({:.1},{:.1})；transform ({:.0},{:.0}) z={:.0}；投影误差 {:.1}px",
            motion.0.aabb.center().x,
            motion.0.aabb.center().y,
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            (motion.0.aabb.center().x - transform.translation.x).abs(),
        );
        info!(
            "试玩诊断：sprite atlas={:?} image={:?} visibility={:?} layers={:?}",
            sprite.texture_atlas.as_ref().map(|atlas| atlas.index),
            sprite.image.id(),
            visibility,
            layers
                .map(|layers| format!("{layers:?}"))
                .unwrap_or_else(|| "<none>".into())
        );
    }
}

/// `F5` / `F6` 退出试玩（`08` §8.6.2）。
fn exit_play(
    keys: Res<ButtonInput<KeyCode>>,
    mut request: ResMut<PlaytestExitRequest>,
    mut next_state: ResMut<NextState<game::state::GameState>>,
    mut next_mode: ResMut<NextState<EditorMode>>,
    _session: Option<Res<EditorSession>>,
) {
    let requested =
        request.0 || keys.just_pressed(KeyCode::F5) || keys.just_pressed(KeyCode::F6);
    if !requested {
        return;
    }
    request.0 = false;
    info!("退出试玩：回到编辑态（编辑缓冲未改动）");
    next_state.set(game::state::GameState::Paused);
    next_mode.set(EditorMode::Editing);
}
