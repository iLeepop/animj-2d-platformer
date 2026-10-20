//! 角色的生成与占位精灵。所有者文档：`docs/04-assets.md` §3.2/§5、`docs/06-physics-and-movement.md` §6.5.1。
//!
//! M1 只渲染**静态帧**（`index = 0`，即 idle 第一帧）：帧动画归 M4（`render/sprite_anim.rs`）。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::TILE_PX;

use super::{PlayerFacing, PlayerId, PlayerMotion, PlayerStepOutcome};
use crate::config::net::{PLAYER_SPAWN_GAP_PX, SPAWN_PLACEMENT_TRIES};
use crate::config::physics::PLAYER_SIZE;
use crate::config::render::{
    PLAYER_SHEET_COLUMNS, PLAYER_SHEET_IMAGE, PLAYER_SHEET_ROWS, player_tint, z,
};
use crate::physics::{Aabb, Body, TileKind, overlaps_tile_kind};
use crate::render::pixel_camera::PIXEL_PERFECT_LAYERS;
use crate::render::snap::{PixelSnap, snap_px};
use crate::render::sprite_anim::{AnimRow, AnimationFrames, AnimationTimer, CurrentAnimation};
use crate::tiles::LevelTiles;

/// 角色图集句柄（图集布局只建一次）。
#[derive(Resource, Clone)]
pub struct PlayerSheet {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    /// 每个动画状态自己的贴图与布局（`player.anim.ron` 的 `sheet`/`columns`）。
    pub states: Vec<(String, Handle<Image>, Handle<TextureAtlasLayout>)>,
}

pub struct PlayerSpawnPlugin;

impl Plugin for PlayerSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_player_sheet)
            .add_systems(Update, rebuild_player_sheets);
    }
}

fn setup_player_sheet(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(TILE_PX as u32),
        PLAYER_SHEET_COLUMNS,
        PLAYER_SHEET_ROWS,
        None,
        None,
    ));
    commands.insert_resource(PlayerSheet {
        image: assets.load(PLAYER_SHEET_IMAGE),
        layout,
        states: Vec::new(),
    });
}

/// 动画资产就绪后，按每个状态的 `sheet`/`columns` 同步加载贴图并建布局。
fn rebuild_player_sheets(
    active: Option<Res<crate::anim_data::ActiveAnim>>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    // 修订号 + 每个状态贴图的 mtime：外部重画 PNG 后也必须重建（`04` §5.6）
    mut done: Local<Option<(u64, Vec<Option<std::time::SystemTime>>)>>,
) {
    let Some(active) = active else {
        return;
    };
    let stamps: Vec<_> = active
        .set
        .states
        .iter()
        .map(|state| crate::assets::texture_stamp(&state.sheet))
        .collect();
    let key = (active.revision, stamps);
    if done.as_ref() == Some(&key) {
        return;
    }
    if let Some((revision, previous)) = done.as_ref()
        && *revision == active.revision
        && previous != &key.1
    {
        info!("检测到角色贴图外部改动，重建图集");
    }
    let mut states = Vec::new();
    for state in &active.set.states {
        let Some(relative) = level_format::normalize_asset_path(&state.sheet) else {
            continue;
        };
        let Some(image) = crate::render::tiles::load_image(&relative, &mut images) else {
            error!("动画状态 `{}` 的贴图 `{}` 加载失败", state.name, relative);
            continue;
        };
        let columns = state.columns.max(1);
        let max_frame = state.frames.iter().copied().max().unwrap_or(0) as u32;
        let rows = ((max_frame + 1).div_ceil(columns)).max(1);
        let layout = layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(TILE_PX as u32),
            columns,
            rows,
            None,
            None,
        ));
        states.push((state.name.clone(), image, layout));
    }
    if states.is_empty() {
        error!("动画状态贴图全部加载失败，继续使用内置图集");
        return;
    }
    *done = Some(key);
    let (_, image, layout) = states[0].clone();
    commands.insert_resource(PlayerSheet {
        image,
        layout,
        states,
    });
}

/// 多玩家出生位置：第一个在 `base`，其余按 ±`PLAYER_SPAWN_GAP_PX` 寻找不与实心瓦片重叠的位置。
///
/// 不依赖房间数据（出生点可能被墙挤住）；找不到空位时退化为按 id 偏移（碰撞求解会处理）。
pub fn player_spawn_centers(base: Vec2, count: usize, tiles: &LevelTiles) -> Vec<Vec2> {
    let mut centers = Vec::with_capacity(count);
    for index in 0..count {
        if index == 0 {
            centers.push(base);
            continue;
        }
        let free = |x: f32| {
            let aabb = Aabb::from_center_size(Vec2::new(x, base.y), PLAYER_SIZE);
            !overlaps_tile_kind(tiles, aabb, 0.0, TileKind::Solid)
        };
        let mut chosen = base.x + index as f32 * PLAYER_SPAWN_GAP_PX;
        for step in 1..=SPAWN_PLACEMENT_TRIES {
            let offset = step as f32 * PLAYER_SPAWN_GAP_PX;
            if free(base.x + offset) {
                chosen = base.x + offset;
                break;
            }
            if free(base.x - offset) {
                chosen = base.x - offset;
                break;
            }
        }
        centers.push(Vec2::new(chosen, base.y));
    }
    centers
}

/// 联机加入点：优先**精确出生点**；被实心瓦片或已有玩家占用时，向四周找最近空位。
///
/// 主机已离开出生点时，远程玩家就出现在出生点本身；主机还站在那儿时才偏移。
pub fn nearest_free_spawn(base: Vec2, tiles: &LevelTiles, occupied: &[Aabb]) -> Vec2 {
    let free = |center: Vec2| {
        let aabb = Aabb::from_center_size(center, PLAYER_SIZE);
        let blocked_by_tile = overlaps_tile_kind(tiles, aabb, 0.0, TileKind::Solid);
        let blocked_by_player = occupied.iter().any(|other| {
            aabb.min.x < other.max.x
                && other.min.x < aabb.max.x
                && aabb.min.y < other.max.y
                && other.min.y < aabb.max.y
        });
        !blocked_by_tile && !blocked_by_player
    };
    if free(base) {
        return base;
    }
    for step in 1..=SPAWN_PLACEMENT_TRIES {
        let offset = step as f32 * PLAYER_SPAWN_GAP_PX;
        for candidate in [
            base + Vec2::new(offset, 0.0),
            base - Vec2::new(offset, 0.0),
            base + Vec2::new(0.0, offset),
            base - Vec2::new(0.0, offset),
        ] {
            if free(candidate) {
                return candidate;
            }
        }
    }
    base
}

/// 在 `center`（世界像素，AABB 中心）生成角色实体。
pub fn spawn_player(
    commands: &mut Commands,
    sheet: &PlayerSheet,
    anim: &crate::anim_data::PlayerAnimSet,
    id: PlayerId,
    center: Vec2,
) -> Entity {
    let idle = anim.state(AnimRow::Idle);
    let mut sprite = Sprite::from_atlas_image(
        sheet.image.clone(),
        TextureAtlas {
            layout: sheet.layout.clone(),
            index: idle.frames.first().copied().unwrap_or(0),
        },
    );
    sprite.color = player_tint(id.0);
    commands
        .spawn((
            id,
            PlayerMotion(Body::at_center(center)),
            PlayerStepOutcome::default(),
            // 默认朝右（`03` §3.4），由 `player::update_player_facing` 随输入更新。
            PlayerFacing::default(),
            sprite,
            // 帧动画：初始 idle（`04` §5.2）
            AnimationFrames::new(idle.frames.clone(), idle.looping),
            AnimationTimer::for_state(idle.frame_secs),
            CurrentAnimation(AnimRow::Idle),
            Anchor::BOTTOM_CENTER,
            // 位置语义 = AABB 底边中点（脚底中点），与 `06` §6.8.2 的投影一致；
            // 每帧由 `render::snap::sync_player_transform` 覆盖。
            Transform::from_xyz(snap_px(center.x), snap_px(center.y), z::player_z(id.0)),
            PIXEL_PERFECT_LAYERS,
            PixelSnap,
        ))
        .id()
}
