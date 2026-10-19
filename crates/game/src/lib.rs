//! animj 玩法 crate。架构与模块划分见 `docs/05-architecture.md`。
//!
//! 本 crate **不含** `DefaultPlugins` / 窗口 / 资产源 / 任何编辑器类型：
//! 两个 bin（`animj`、`animj-editor`）各自负责外壳，再调用 [`add_game_plugins`] 复用同一套玩法插件
//! （见 `docs/08-level-editor.md` §8.2）。

pub mod anim_data;
pub mod assets;
pub mod audio;
pub mod camera;
pub mod checkpoint;
pub mod config;
pub mod coords;
pub mod demo;
pub mod fonts;
pub mod i18n;
pub mod level_asset;
pub mod physics;
pub mod player;
pub mod render;
pub mod room;
pub mod save;
pub mod settings;
pub mod state;
pub mod tiles;
pub mod world;

use bevy::app::PluginGroupBuilder;
use bevy::asset::AssetPlugin;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowMode, WindowResolution};

use crate::config::render::{WINDOW_H, WINDOW_TITLE, WINDOW_W};

/// 两个 bin 共用的 `DefaultPlugins` 外壳：最近邻采样 + 像素风窗口 + 仓库根 `assets/`。
///
/// 单独抽出来是为了让 `animj` 与 `animj-editor` 的差异**只出现在**「额外加了什么插件」，
/// 而不是各自抄一份窗口/资产配置（两份配置必然漂移）。
pub fn default_plugins() -> PluginGroupBuilder {
    DefaultPlugins
        .set(ImagePlugin::default_nearest())
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: WINDOW_TITLE.into(),
                // 锁死 1 逻辑像素 = 1 物理像素：否则 OS 的 DPI 缩放会破坏整数倍放大（见 07 §7.4.2）
                resolution: WindowResolution::new(WINDOW_W, WINDOW_H)
                    .with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                mode: WindowMode::Windowed,
                resizable: true,
                ..default()
            }),
            ..default()
        })
        .set(AssetPlugin {
            file_path: assets::asset_file_path(),
            ..default()
        })
}

/// 注册全部玩法插件（**两个 bin 共用**）。
///
/// 只包含「世界与玩法」：状态、关卡资产、关卡加载与生成、瓦片实体、相机跟随、调试。
/// **不含** `DefaultPlugins` / 窗口 / 资产源 / 编辑器类型，也**不含** [`add_render_plugins`] 的像素管线
/// —— 编辑器用单个 `EditorCamera` 直接渲染世界（`08` §8.1.4）。
pub fn add_game_plugins(app: &mut App) {
    app.add_plugins(fonts::UiFontPlugin);
    app.add_plugins((
        state::CorePlugin,
        assets::AssetPathPlugin,
        level_asset::LevelAssetPlugin,
        world::WorldPlugin,
        render::tiles::TileRenderPlugin,
        player::PlayerPlugin,
        player::spawn::PlayerSpawnPlugin,
        camera::CameraRigPlugin,
        room::RoomPlugin,
        save::SavePlugin,
        render::pause::PausePlugin,
        render::debug::DebugPlugin,
        render::sprite_anim::SpriteAnimPlugin,
        checkpoint::CheckpointPlugin,
        (audio::AudioFxPlugin, anim_data::AnimDataPlugin),
    ));
    // 背景视差：编辑器与游戏共用（`07` §7.13）。
    app.add_plugins(render::parallax::ParallaxPlugin);
}

/// 注册像素完美渲染管线与**发布外壳**（主菜单 / 关卡选择 / 设置）。
///
/// 编辑器不注册它：离屏像素完美管线（整数倍放大 + 黑边）的验收在 `animj` 上做（`08` §8.1.4）；
/// 菜单流程也只属于发布进程（编辑器用 `StartupFlow::Direct` 直达关卡，`05` §5.5）。
pub fn add_render_plugins(app: &mut App) {
    app.add_plugins((
        render::PixelRenderPlugin,
        render::hud::HudPlugin,
        settings::SettingsPlugin,
        render::menu::MenuPlugin,
    ));
}
