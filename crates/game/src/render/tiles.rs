//! 瓦片渲染。所有者文档：`docs/07-rendering.md` §7.8。
//!
//! `bevy_ecs_tilemap` 在 Bevy 0.20 上不可用（`09` §9.2），因此自研：
//! 每个**非空**瓦片一个 `Sprite` 实体，按房间分组挂在 `RoomRoot` 下，房间粒度做可见性裁剪。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::{RENDERABLE_TILE_KINDS, ROOM_PX, ROOM_TILES, TILE_PX, TileKind, room_index};

use crate::config::render::{TILESET_COLUMNS, TILESET_IMAGE, room_bg_color, z};
use crate::coords::room_center;
use crate::render::pixel_camera::{InGameCamera, PIXEL_PERFECT_LAYERS};
use crate::world::CurrentRoom;
use crate::world::load::LoadedLevel;

/// 一个房间的实体树根。`Transform` = 房间中心（世界整像素）。
#[derive(Component, Debug, Clone, Copy)]
pub struct RoomRoot {
    pub col: u32,
    pub row: u32,
}

/// 瓦片图集的句柄（图集布局只建一次，全关复用）。
#[derive(Resource, Clone)]
pub struct Tileset {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

/// 房间粒度可见性裁剪开关。
///
/// 默认开启（发布 bin `animj`：一屏 = 一个房间，非当前房间必然在视口外）。
/// **编辑器把它设为 `false`**：编辑时需要看到整张地图（`08` §8.1.4 的单相机自由平移视图）。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomCulling(pub bool);

/// 关卡图集缓存：`(路径, mtime, 图像)`；mtime 变化即重新解码。
type TilesetCache = Option<(String, Option<std::time::SystemTime>, Handle<Image>)>;

/// 关卡贴图文件的外部改动请求（`docs/04-assets.md` §5.6）。
///
/// 仅由 [`watch_level_tileset`] 置位，由 [`spawn_level_tiles`] 消费并复位。
#[derive(Resource, Default)]
pub struct TilesetReloadRequest(pub bool);

impl Default for RoomCulling {
    fn default() -> Self {
        Self(true)
    }
}

pub struct TileRenderPlugin;

impl Plugin for TileRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomCulling>()
            .init_resource::<TilesetReloadRequest>()
            .add_systems(Startup, setup_tileset)
            // 关卡加载完成（`LoadedLevel` 出现）那一帧生成全部房间；
            // 贴图文件被外部改动时由 watcher 置位并重建（`04` §5.6）。
            .add_systems(
                Update,
                (
                    watch_level_tileset,
                    spawn_level_tiles
                        .run_if(tiles_need_rebuild)
                        .before(sync_room_visibility),
                )
                    .chain(),
            )
            .add_systems(Update, sync_room_visibility);
    }
}

fn setup_tileset(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // 行数算法见 04 §10 / 07 §7.8.2：ceil(可渲染条目数 / TILESET_COLUMNS)
    let rows = (RENDERABLE_TILE_KINDS.len() as u32)
        .div_ceil(TILESET_COLUMNS)
        .max(1);
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(TILE_PX as u32),
        TILESET_COLUMNS,
        rows,
        None,
        None,
    ));
    commands.insert_resource(Tileset {
        image: assets.load(TILESET_IMAGE),
        layout,
    });
}

/// 同步加载关卡自带 tileset：直接读盘 + 解码后插入 `Assets<Image>`。
///
/// **不用 `AssetServer::load`**：实测编辑器进程里**运行期**请求的新图像会永远停在
/// `Loading`（同一进程启动期请求的图像正常），导致"设置贴图路径没有效果"。
/// 同步读盘对编辑器（改一次路径加载一次）和游戏（启动一次）都足够快。
pub fn load_tileset_image(path: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    let relative = level_format::normalize_asset_path(path)?;
    let full = std::path::Path::new(&crate::assets::asset_file_path()).join(&relative);
    let bytes = match std::fs::read(&full) {
        Ok(bytes) => bytes,
        Err(error) => {
            error!("tileset `{}` 读取失败：{error}", full.display());
            return None;
        }
    };
    let extension = relative.rsplit('.').next().unwrap_or("png").to_string();
    match Image::from_buffer(
        &bytes,
        bevy::image::ImageType::Extension(Box::leak(extension.into_boxed_str())),
        bevy::image::CompressedImageFormats::NONE,
        true,
        bevy::image::ImageSampler::nearest(),
        bevy::asset::RenderAssetUsages::default(),
    ) {
        Ok(image) => Some(images.add(image)),
        Err(error) => {
            error!("tileset `{}` 解码失败：{error}", full.display());
            None
        }
    }
}

/// 轮询关卡贴图文件的 mtime：外部软件重画贴图后无需重启即可重建（`04` §5.6）。
///
/// 只比较「路径 + 修改时间」：路径变化由关卡热重载负责，这里也会跟随并重置基准。
fn watch_level_tileset(
    loaded: Option<Res<LoadedLevel>>,
    mut reload: ResMut<TilesetReloadRequest>,
    mut last: Local<Option<(Option<String>, Option<std::time::SystemTime>)>>,
) {
    let Some(loaded) = loaded else {
        return;
    };
    let path = loaded.level.tileset.clone();
    let stamp = path.as_deref().and_then(crate::assets::texture_stamp);
    let now = (path, stamp);
    match last.as_ref() {
        // 首次观察只记录基准，不触发重建：关卡加载本身已经会生成瓦片
        None => *last = Some(now),
        Some(previous) if *previous == now => {}
        Some(_) => {
            *last = Some(now.clone());
            if let Some(path) = &now.0 {
                info!("检测到贴图外部改动：{path}，重建瓦片");
            }
            reload.0 = true;
        }
    }
}

/// `LoadedLevel` 首次出现 / 热重载 / 贴图外部改动时重建瓦片。
fn tiles_need_rebuild(
    loaded: Option<Res<LoadedLevel>>,
    reload: Res<TilesetReloadRequest>,
) -> bool {
    reload.0
        || loaded
            .as_ref()
            .is_some_and(|loaded| loaded.is_changed())
}

/// `LoadedLevel` 出现的同一帧生成瓦片实体。
fn spawn_level_tiles(
    mut commands: Commands,
    loaded: Res<LoadedLevel>,
    tileset: Res<Tileset>,
    level_tiles: Option<Res<crate::tiles::LevelTiles>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
    mut sync_cache: Local<TilesetCache>,
    mut reload: ResMut<TilesetReloadRequest>,
    _assets: Res<AssetServer>,
    current: Res<CurrentRoom>,
    culling: Res<RoomCulling>,
    old_roots: Query<Entity, With<RoomRoot>>,
) {
    reload.0 = false;
    // 热重载会再次进入本系统：先销毁旧的房间根（其 `ChildOf` 子树一并销毁），避免残影
    for entity in &old_roots {
        commands.entity(entity).despawn();
    }
    let level = &loaded.level;
    let (cols, rows) = (level.grid.cols, level.grid.rows);

    // 关卡自带贴图（v2）：按 `tileset_columns` 建图集；无自定义时用内置
    let columns = level.tileset_columns.max(1);
    let max_cell = level
        .palette()
        .iter()
        .map(|entry| entry.cell)
        .max()
        .unwrap_or(0);
    let layout_rows = (max_cell as u32 + 1).div_ceil(columns).max(1);
    let (image, layout) = match &level.tileset {
        Some(path) => (
            {
                let key = level_format::normalize_asset_path(path).unwrap_or_default();
                // mtime 也是键的一部分：只按路径缓存会在外部重画贴图后继续用旧图
                let stamp = crate::assets::texture_stamp(&key);
                let reused = match sync_cache.as_ref() {
                    Some((cached, cached_stamp, handle))
                        if cached == &key && *cached_stamp == stamp =>
                    {
                        Some(handle.clone())
                    }
                    _ => None,
                };
                let handle = reused.or_else(|| {
                    let loaded = load_tileset_image(&key, &mut images);
                    if let Some(handle) = &loaded {
                        *sync_cache = Some((key.clone(), stamp, handle.clone()));
                    }
                    loaded
                });
                // 失败时退回内置图集，避免整个房间无图
                handle.unwrap_or_else(|| tileset.image.clone())
            },
            layouts.add(TextureAtlasLayout::from_grid(
                UVec2::splat(TILE_PX as u32),
                columns,
                layout_rows,
                None,
                None,
            )),
        ),
        None => (tileset.image.clone(), tileset.layout.clone()),
    };
    let half = ROOM_PX as f32 / 2.0;
    let mut tile_count: usize = 0;

    for row in 0..rows {
        for col in 0..cols {
            let Some(_room) = level.rooms.get(room_index(col, row, cols)) else {
                continue;
            };
            let center = room_center(col, row, rows);
            let is_current = col == current.col && row == current.row;
            let visible = if !culling.0 || is_current {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            let root = commands
                .spawn((
                    RoomRoot { col, row },
                    Transform::from_xyz(center.x, center.y, 0.0),
                    visible,
                    PIXEL_PERFECT_LAYERS,
                ))
                .id();

            // 房间背景板：整房间一张，用非黑色以便在截图里与窗口黑边区分
            commands.spawn((
                Sprite::from_color(room_bg_color(), Vec2::splat(ROOM_PX as f32)),
                Anchor::BOTTOM_CENTER,
                Transform::from_xyz(0.0, -half, z::ROOM_BG),
                ChildOf(root),
            ));

            let Some(room_tiles) = level_tiles
                .as_ref()
                .and_then(|tiles| tiles.room(col, row))
                .cloned()
            else {
                continue;
            };
            for ty in 0..ROOM_TILES {
                for tx in 0..ROOM_TILES {
                    // v2：贴图格来自关卡调色板（`03` §3.9）；行为只影响物理与层次
                    let Some(cell) = room_tiles.cell(tx, ty) else {
                        continue; // 空 / 无贴图不生成实体
                    };
                    let kind = room_tiles.get(tx, ty);
                    // 瓦片 AABB 底边中点（房间局部，原点左下）→ 相对房间中心 → 整像素
                    let local = Vec2::new(
                        tx as f32 * TILE_PX as f32 + TILE_PX as f32 / 2.0,
                        ty as f32 * TILE_PX as f32,
                    );
                    let rel = local - Vec2::splat(half);
                    let tile_z = if kind == TileKind::Decor {
                        z::TILE_DECAL
                    } else {
                        z::TILE
                    };
                    commands.spawn((
                        Sprite::from_atlas_image(
                            image.clone(),
                            TextureAtlas {
                                layout: layout.clone(),
                                index: cell as usize,
                            },
                        ),
                        Anchor::BOTTOM_CENTER,
                        Transform::from_xyz(rel.x, rel.y, tile_z),
                        ChildOf(root),
                    ));
                    tile_count += 1;
                }
            }
        }
    }

    info!(
        "关卡已生成：{}×{} = {} 个房间，非空瓦片实体 {tile_count} 个",
        cols,
        rows,
        cols * rows
    );
}

/// 房间可见性：**与相机视野矩形相交**的房间可见（`02` §2.2 的过渡中会同时看到两个房间）。
///
/// 视野 = 以相机为中心、边长 `ROOM_PX` 的正方形（内层 `ScalingMode::Fixed` 的契约，见 `07` §7.5）。
/// 用「房间中心 ± 半边长」与视野做**严格重叠**判断：静止时视野恰好等于当前房间，
/// 只会命中它自己（共边不算重叠），过渡时自然命中相邻房间。
fn sync_room_visibility(
    culling: Res<RoomCulling>,
    loaded: Option<Res<LoadedLevel>>,
    camera: Option<Single<&Transform, With<InGameCamera>>>,
    mut roots: Query<(&RoomRoot, &mut Visibility)>,
) {
    // 编辑器：`RoomCulling(false)` ⇒ 整张地图可见
    if !culling.0 {
        for (_, mut visibility) in &mut roots {
            if *visibility != Visibility::Visible {
                *visibility = Visibility::Visible;
            }
        }
        return;
    }

    let (Some(loaded), Some(camera)) = (loaded, camera) else {
        return;
    };
    let half = ROOM_PX as f32 / 2.0;
    let view_min = Vec2::new(camera.translation.x - half, camera.translation.y - half);
    let view_max = Vec2::new(camera.translation.x + half, camera.translation.y + half);
    let rows = loaded.level.grid.rows;

    for (root, mut visibility) in &mut roots {
        let center = room_center(root.col, root.row, rows);
        let visible = view_min.x < center.x + half
            && center.x - half < view_max.x
            && view_min.y < center.y + half
            && center.y - half < view_max.y;
        let want = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
    }
}
