//! 编辑态投影（`doc` 的只读投影）与 Gizmos 叠加。所有者文档：`docs/08-level-editor.md`
//! §8.5.5、§8.6.3 R6。
//!
//! **投影可无损重建**：`doc` 是唯一真相，实体只是它的投影；任何改动后重建受影响的房间即可。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::{ROOM_PX, ROOM_TILES, TILE_PX};

use crate::camera::room_center_world;
use crate::session::EditorSession;
use crate::EditorMode;

/// 编辑态投影的房间根。
#[derive(Component)]
pub struct EditorProjectionRoom(pub u32);

/// 关卡图集缓存：`(路径, mtime, 列数, 最大 cell, 图像, 布局)`。
type AtlasCache = Option<(
    String,
    Option<std::time::SystemTime>,
    u32,
    u32,
    Handle<Image>,
    Handle<TextureAtlasLayout>,
)>;

/// 旧的投影已经建过哪些房间、基于哪个修订。
#[derive(Resource, Default)]
pub struct ProjectionState {
    revision: Option<u64>,
    room: Option<(u32, u32)>,
}

/// 叠加开关（`G` / `Shift+G`）。
#[derive(Resource, Debug, Clone, Copy)]
pub struct EditorViewFlags {
    pub grid: bool,
    pub borders: bool,
}

impl Default for EditorViewFlags {
    fn default() -> Self {
        Self {
            grid: true,
            borders: true,
        }
    }
}

pub const GRID_COLOR: Color = Color::srgb_u8(58, 58, 66);
pub const ROOM_BORDER_COLOR: Color = Color::srgb_u8(120, 120, 140);
pub const HOVER_COLOR: Color = Color::srgb_u8(255, 240, 130);
pub const MARQUEE_COLOR: Color = Color::srgb_u8(80, 220, 255);
pub const ERROR_COLOR: Color = Color::srgb_u8(255, 64, 64);

pub struct EditorViewPlugin;

impl Plugin for EditorViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorViewFlags>()
            .init_resource::<ProjectionState>()
            .add_systems(
                Update,
                (
                    hide_game_world,
                    watch_doc_tileset,
                    sync_background_preview,
                    ensure_projection.run_if(in_state(EditorMode::Editing)),
                )
                    .chain(),
            );
    }
}

/// 编辑态隐藏 `game` 的瓦片/实体投影（它们由 `doc` 在编辑态重新投影）。
fn hide_game_world(
    mut commands: Commands,
    mode: Res<State<EditorMode>>,
    roots: Query<Entity, With<game::render::tiles::RoomRoot>>,
    zones: Query<Entity, With<game::room::RoomZone>>,
) {
    if *mode.get() != EditorMode::Editing {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    for entity in &zones {
        commands.entity(entity).despawn();
    }
}

/// 把未落盘的 `doc.background` 交给背景层（编辑器画布实时预览，`08` §8.5.7 C）。
fn sync_background_preview(
    mut commands: Commands,
    session: Option<Res<EditorSession>>,
    mut last: Local<Option<Option<level_format::Background>>>,
) {
    let Some(session) = session else {
        return;
    };
    if last.as_ref() == Some(&session.doc.background) {
        return;
    }
    *last = Some(session.doc.background.clone());
    commands.insert_resource(game::render::parallax::BackgroundOverride(
        session.doc.background.clone(),
    ));
}

/// 贴图文件被外部改动（美术软件重画）⇒ 整体重建投影（`04` §5.6）。
///
/// 与关卡数据无关：不碰 `doc`/`dirty`/undo，只让下一次 `ensure_projection` 走全量分支。
fn watch_doc_tileset(
    mut commands: Commands,
    session: Option<Res<EditorSession>>,
    mut state: ResMut<ProjectionState>,
    roots: Query<Entity, With<EditorProjectionRoom>>,
    mut last: Local<Option<(Option<String>, Option<std::time::SystemTime>)>>,
) {
    let Some(session) = session else {
        return;
    };
    let path = session.doc.tileset.clone();
    let stamp = path.as_deref().and_then(game::assets::texture_stamp);
    let now = (path, stamp);
    match last.as_ref() {
        // 首次观察只记录基准：启动时投影本来就会全量生成
        None => *last = Some(now),
        Some(previous) if *previous == now => {}
        Some(_) => {
            *last = Some(now.clone());
            for entity in &roots {
                commands.entity(entity).despawn();
            }
            state.revision = None;
            info!("编辑器：贴图文件外部改动，重建全部房间投影");
        }
    }
}

/// 首次建全图；之后只重建**当前房间**（工具只写当前房间，`08` §8.5.3）。
fn ensure_projection(
    mut commands: Commands,
    session: Option<Res<EditorSession>>,
    tileset: Option<Res<game::render::tiles::Tileset>>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut atlas_cache: Local<AtlasCache>,
    roots: Query<(Entity, &EditorProjectionRoom)>,
    mut state: ResMut<ProjectionState>,
) {
    let (Some(session), Some(tileset)) = (session, tileset) else {
        return;
    };
    // 投影必须用**关卡自己的**贴图与列数（否则改路径后画出来还是内置布局）
    let columns = session.doc.tileset_columns.max(1);
    let key = session
        .doc
        .tileset
        .as_deref()
        .and_then(level_format::normalize_asset_path)
        .unwrap_or_default();
    let max_cell = session
        .doc
        .palette()
        .iter()
        .map(|entry| entry.cell)
        .max()
        .unwrap_or(0);
    let stamp = game::assets::texture_stamp(&key);
    let (image, layout) = if key.is_empty() {
        (tileset.image.clone(), tileset.layout.clone())
    } else {
        match atlas_cache.as_ref() {
            Some((cached, cached_stamp, cached_columns, cached_max_cell, image, layout))
                if cached == &key
                    && *cached_stamp == stamp
                    && *cached_columns == columns
                    && *cached_max_cell == max_cell =>
            {
                (image.clone(), layout.clone())
            }
            _ => {
                let rows = ((max_cell + 1).div_ceil(columns)).max(1);
                let layout = layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::splat(level_format::TILE_PX as u32),
                    columns,
                    rows,
                    None,
                    None,
                ));
                let image = game::render::tiles::load_image(&key, &mut images)
                    .unwrap_or_else(|| tileset.image.clone());
                *atlas_cache = Some((
                    key.clone(),
                    stamp,
                    columns,
                    max_cell,
                    image.clone(),
                    layout.clone(),
                ));
                (image, layout)
            }
        }
    };
    let revision_changed = state.revision != Some(session.doc_revision);

    if roots.is_empty() {
        for index in 0..session.doc.rooms.len() {
            spawn_room(&mut commands, &session.doc, &image, &layout, index as u32);
        }
        state.revision = Some(session.doc_revision);
        state.room = Some(session.current_room);
        return;
    }
    // 切房间**不再重建**：所有房间在首次加载时已建好，只有被编辑（修订号变化）的当前房间需要重建
    if !revision_changed {
        state.room = Some(session.current_room);
        return;
    }
    let index = session.room_index() as u32;
    for (entity, root) in &roots {
        if root.0 == index {
            commands.entity(entity).despawn();
        }
    }
    spawn_room(&mut commands, &session.doc, &image, &layout, index);
    state.revision = Some(session.doc_revision);
    state.room = Some(session.current_room);
}

/// 一个房间的投影：瓦片 sprite（图集）+ 实体色块（尺寸 = 判定尺寸）。
fn spawn_room(
    commands: &mut Commands,
    doc: &level_format::Level,
    image: &Handle<Image>,
    layout: &Handle<TextureAtlasLayout>,
    index: u32,
) {
    let Some(room) = doc.rooms.get(index as usize) else {
        return;
    };
    let cols = doc.grid.cols;
    let rows = doc.grid.rows;
    let (col, row) = (index % cols, index / cols);
    let center = room_center_world(col, row, rows);
    let half = ROOM_PX as f32 / 2.0;
    let root = commands
        .spawn((
            EditorProjectionRoom(index),
            Transform::from_xyz(center.x, center.y, 0.0),
            Visibility::Visible,
        ))
        .id();

    for ty in 0..ROOM_TILES {
        for tx in 0..ROOM_TILES {
            // v2：画什么由**关卡调色板**决定（字符 → cell），不再查内置 legend。
            // 文件第 0 行是**顶部**，而渲染行 0 是**底部**（`local.y = ty * TILE_PX` + 底锚点），
            // 不翻转会让放置位置上下镜像（实机 bug）。
            let file_row = ROOM_TILES - 1 - ty;
            let Some(ch) = room
                .tiles
                .get(file_row)
                .and_then(|line| line.chars().nth(tx))
            else {
                continue;
            };
            if ch == '.' {
                continue;
            }
            let Some(cell) = doc.cell_of(ch) else {
                continue;
            };
            let local = Vec2::new(
                tx as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0,
                ty as f32 * TILE_PX as f32,
            );
            let rel = local - Vec2::splat(half);
            commands.spawn((
                Sprite::from_atlas_image(
                    image.clone(),
                    TextureAtlas {
                        layout: layout.clone(),
                        index: cell as usize,
                    },
                ),
                Anchor::BOTTOM_CENTER,
                Transform::from_xyz(rel.x, rel.y, game::config::render::z::TILE),
                ChildOf(root),
            ));
        }
    }

    for spec in &room.entities {
        // 出生点也必须可见（否则"加了但看不见"）：32×32 青色标记
        let size = match spec.kind.as_str() {
            "fruit" => Vec2::splat(game::config::physics::PICKUP_SIZE),
            "player_spawn" => Vec2::splat(32.0),
            _ => spec
                .size
                .map(|(w, h)| Vec2::new(w as f32, h as f32))
                .unwrap_or(Vec2::splat(32.0)),
        };
        let local = Vec2::new(spec.at.0 as f32, spec.at.1 as f32);
        let world = game::coords::room_local_to_world(col, row, rows, local);
        let color = match spec.kind.as_str() {
            "exit" => Color::srgb(0.30, 0.85, 0.42),
            "fruit" => Color::srgb(0.92, 0.36, 0.55),
            "checkpoint" => game::checkpoint::ZONE_COLOR,
            "player_spawn" => Color::srgb(0.35, 0.85, 0.95),
            _ => Color::srgb(0.9, 0.9, 0.4),
        };
        commands.spawn((
            Sprite::from_color(color, size),
            Anchor::BOTTOM_CENTER,
            Transform::from_xyz(
                world.x - center.x,
                world.y - size.y / 2.0 - center.y,
                game::config::render::z::PICKUP,
            ),
            ChildOf(root),
        ));
    }
}




#[cfg(test)]
mod tests {
    use super::*;

    fn level(background: Option<level_format::Background>) -> level_format::Level {
        level_format::Level {
            version: level_format::VERSION,
            name: "t".into(),
            grid: level_format::Grid { cols: 1, rows: 1 },
            start: level_format::RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background,
            rooms: Vec::new(),
        }
    }

    /// 编辑器画布的预览必须实时跟随 `doc.background`（不保存、不试玩）。
    #[test]
    fn background_preview_follows_document() {
        let mut app = App::new();
        app.add_systems(Update, sync_background_preview)
            .insert_resource(EditorSession::new(level(None), None));
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .resource::<game::render::parallax::BackgroundOverride>()
                .0,
            None
        );

        let background = level_format::Background {
            layers: vec![level_format::BackgroundLayer {
                image: "sprites/backgrounds/sky.png".into(),
                scroll_x: 30,
                scroll_y: 0,
            }],
        };
        app.world_mut()
            .resource_mut::<EditorSession>()
            .doc
            .background = Some(background.clone());
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .resource::<game::render::parallax::BackgroundOverride>()
                .0
                .as_ref(),
            Some(&background)
        );
    }
}
