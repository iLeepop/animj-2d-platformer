//! 角色的生成与占位精灵。所有者文档：`docs/04-assets.md` §3.2/§5、`docs/06-physics-and-movement.md` §6.5.1。
//!
//! M1 只渲染**静态帧**（`index = 0`，即 idle 第一帧）：帧动画归 M4（`render/sprite_anim.rs`）。

use bevy::prelude::*;
use bevy::sprite::Anchor;
use level_format::TILE_PX;

use super::{PlayerFacing, PlayerMotion, PlayerStepOutcome};
use crate::config::render::{PLAYER_SHEET_COLUMNS, PLAYER_SHEET_IMAGE, PLAYER_SHEET_ROWS, z};
use crate::physics::Body;
use crate::render::pixel_camera::PIXEL_PERFECT_LAYERS;
use crate::render::sprite_anim::{AnimRow, AnimationFrames, AnimationTimer, CurrentAnimation};
use crate::render::snap::{PixelSnap, snap_px};

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
        let Some(image) = crate::render::tiles::load_tileset_image(&relative, &mut images) else {
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

/// 在 `center`（世界像素，AABB 中心）生成角色实体。
pub fn spawn_player(
    commands: &mut Commands,
    sheet: &PlayerSheet,
    anim: &crate::anim_data::PlayerAnimSet,
    center: Vec2,
) -> Entity {
    let idle = anim.state(AnimRow::Idle);
    commands
        .spawn((
            PlayerMotion(Body::at_center(center)),
            PlayerStepOutcome::default(),
            // 默认朝右（`03` §3.4），由 `player::update_player_facing` 随输入更新。
            PlayerFacing::default(),
            Sprite::from_atlas_image(
                sheet.image.clone(),
                TextureAtlas {
                    layout: sheet.layout.clone(),
                    index: idle.frames.first().copied().unwrap_or(0),
                },
            ),
            // 帧动画：初始 idle（`04` §5.2）
            AnimationFrames::new(idle.frames.clone(), idle.looping),
            AnimationTimer::for_state(idle.frame_secs),
            CurrentAnimation(AnimRow::Idle),
            Anchor::BOTTOM_CENTER,
            // 位置语义 = AABB 底边中点（脚底中点），与 `06` §6.8.2 的投影一致；
            // 每帧由 `render::snap::sync_player_transform` 覆盖。
            Transform::from_xyz(snap_px(center.x), snap_px(center.y), z::PLAYER),
            PIXEL_PERFECT_LAYERS,
            PixelSnap,
        ))
        .id()
}
