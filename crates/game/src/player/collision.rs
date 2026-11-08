//! 全局玩家碰撞配置。所有者文档：`docs/06-physics-and-movement.md` §6.5.1。
//!
//! 项目级固定值（不是每关一份）：保存于 `assets/config/player.ron`，游戏启动读取，
//! 编辑器「玩家」标签页修改并写回。所有玩家（单机 / 本地双人 / 联机）共用同一套形状。
//!
//! 坐标约定：`size` 是 AABB 宽高；`offset` 是 **AABB 底边中点相对精灵底边中点** 的偏移
//! （+x 右、+y 上，单位像素）。默认 `(0, 0)` 与旧行为完全一致（AABB 底边中点 = 精灵脚底中点）。

use std::path::{Path, PathBuf};

use bevy::math::Vec2;
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::config::physics::{PLAYER_OFFSET_MAX, PLAYER_SIZE, PLAYER_SIZE_MAX, PLAYER_SIZE_MIN};
use crate::physics::Aabb;

/// 配置文件版本（`06` §6.5.1）。版本不符 ⇒ 忽略整份配置并回退默认值。
pub const PLAYER_COLLISION_VERSION: u32 = 1;
/// 相对 `assets/` 的配置文件路径。
pub const PLAYER_COLLISION_FILE: &str = "config/player.ron";

/// 全局玩家碰撞形状（项目级固定值）。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerCollision {
    /// 配置版本；必须等于 [`PLAYER_COLLISION_VERSION`]。
    pub version: u32,
    /// AABB 宽高（px）。
    pub size: (f32, f32),
    /// AABB 底边中点相对精灵底边中点的偏移（px，+x 右、+y 上）。
    pub offset: (f32, f32),
}

impl Default for PlayerCollision {
    fn default() -> Self {
        Self {
            version: PLAYER_COLLISION_VERSION,
            size: (PLAYER_SIZE.x, PLAYER_SIZE.y),
            offset: (0.0, 0.0),
        }
    }
}

impl PlayerCollision {
    /// AABB 尺寸。
    pub fn size_vec(&self) -> Vec2 {
        Vec2::new(self.size.0, self.size.1)
    }

    /// AABB 底边中点相对精灵脚底的偏移。
    pub fn offset_vec(&self) -> Vec2 {
        Vec2::new(self.offset.0, self.offset.1)
    }

    /// 精灵底边中点的世界坐标：`AABB 底边中点 − offset`。
    pub fn sprite_bottom_center(&self, aabb: &Aabb) -> Vec2 {
        Vec2::new(aabb.center().x - self.offset.0, aabb.min.y - self.offset.1)
    }

    /// 钳制到合法范围（读档、保存与 UI 共用）。
    pub fn clamp(&mut self) {
        self.version = PLAYER_COLLISION_VERSION;
        self.size.0 = self.size.0.clamp(PLAYER_SIZE_MIN, PLAYER_SIZE_MAX);
        self.size.1 = self.size.1.clamp(PLAYER_SIZE_MIN, PLAYER_SIZE_MAX);
        self.offset.0 = self.offset.0.clamp(-PLAYER_OFFSET_MAX, PLAYER_OFFSET_MAX);
        self.offset.1 = self.offset.1.clamp(-PLAYER_OFFSET_MAX, PLAYER_OFFSET_MAX);
    }

    /// 配置文件路径：`<assets>/config/player.ron`。
    pub fn config_path() -> PathBuf {
        Path::new(&crate::assets::asset_file_path()).join(PLAYER_COLLISION_FILE)
    }

    /// 从磁盘读取；文件不存在 / 解析失败 / 版本不符一律回退默认值（不 panic）。
    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    /// 从指定路径读取（测试用）。
    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match ron::from_str::<Self>(&text) {
            Ok(mut config) if config.version == PLAYER_COLLISION_VERSION => {
                config.clamp();
                config
            }
            Ok(config) => {
                bevy::log::warn!(
                    "玩家碰撞配置版本 {} 与当前 {} 不符，回退默认值（06 §6.5.1）",
                    config.version,
                    PLAYER_COLLISION_VERSION
                );
                Self::default()
            }
            Err(error) => {
                bevy::log::warn!("玩家碰撞配置解析失败（{}）：{error}", path.display());
                Self::default()
            }
        }
    }

    /// 原子写入 `assets/config/player.ron`。
    pub fn persist(&self) -> std::io::Result<()> {
        self.persist_to(&Self::config_path())
    }

    /// 写入指定路径（测试用）。
    pub fn persist_to(&self, path: &Path) -> std::io::Result<()> {
        let mut config = *self;
        config.clamp();
        let text = ron::ser::to_string_pretty(
            &config,
            ron::ser::PrettyConfig::new()
                .indentor("    ")
                .new_line("\n"),
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        crate::save::write_atomic(path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "animj-player-collision-{}-{tag}.ron",
            std::process::id()
        ))
    }

    #[test]
    fn default_matches_legacy_player_size() {
        let config = PlayerCollision::default();
        assert_eq!(config.size_vec(), PLAYER_SIZE);
        assert_eq!(config.offset_vec(), Vec2::ZERO);
        assert_eq!(config.version, PLAYER_COLLISION_VERSION);
    }

    #[test]
    fn sprite_bottom_follows_offset() {
        let config = PlayerCollision {
            size: (20.0, 28.0),
            offset: (3.0, 4.0),
            ..Default::default()
        };
        let aabb = Aabb::from_center_size(Vec2::new(100.0, 50.0), config.size_vec());
        assert_eq!(config.sprite_bottom_center(&aabb), Vec2::new(97.0, 32.0));
    }

    #[test]
    fn clamp_keeps_values_in_range() {
        let mut config = PlayerCollision {
            size: (0.0, 1000.0),
            offset: (-1000.0, 1000.0),
            ..Default::default()
        };
        config.clamp();
        assert_eq!(config.size, (PLAYER_SIZE_MIN, PLAYER_SIZE_MAX));
        assert_eq!(config.offset, (-PLAYER_OFFSET_MAX, PLAYER_OFFSET_MAX));
    }

    #[test]
    fn persist_and_load_round_trip() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let config = PlayerCollision {
            size: (16.0, 24.0),
            offset: (2.0, 3.0),
            ..Default::default()
        };
        config.persist_to(&path).expect("写盘应成功");
        assert_eq!(PlayerCollision::load_from(&path), config);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_or_bad_file_falls_back_to_default() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            PlayerCollision::load_from(&path),
            PlayerCollision::default()
        );

        let bad = temp_path("bad");
        std::fs::write(&bad, "(version: 99, size: (1.0, 1.0), offset: (0.0, 0.0))")
            .expect("写测试文件");
        assert_eq!(PlayerCollision::load_from(&bad), PlayerCollision::default());
        let _ = std::fs::remove_file(&bad);
    }

    #[test]
    fn load_clamps_out_of_range_values() {
        let path = temp_path("clamp");
        std::fs::write(
            &path,
            format!(
                "(version: {PLAYER_COLLISION_VERSION}, size: (0.0, 999.0), offset: (-999.0, 999.0))"
            ),
        )
        .expect("写测试文件");
        let config = PlayerCollision::load_from(&path);
        assert_eq!(config.size, (PLAYER_SIZE_MIN, PLAYER_SIZE_MAX));
        assert_eq!(config.offset, (-PLAYER_OFFSET_MAX, PLAYER_OFFSET_MAX));
        let _ = std::fs::remove_file(&path);
    }
}
