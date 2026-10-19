//! 渲染层。所有者文档：`docs/07-rendering.md`。

pub mod debug;
pub mod hud;
pub mod parallax;
pub mod pause;
pub mod pixel_camera;
pub mod snap;
pub mod sprite_anim;
pub mod tiles;

use bevy::prelude::*;

pub struct PixelRenderPlugin;

impl Plugin for PixelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((pixel_camera::PixelCameraPlugin, snap::SnapPlugin));
    }
}
