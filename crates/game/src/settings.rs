//! 全局设置：音量与全屏。所有者文档：`docs/05-architecture.md` §5.13。
//!
//! 设置与存档**分开存放**（`settings.ron` / `save.ron`）：存档是单关卡进度，设置是进程级偏好，
//! 两者版本与失效规则不同（`05` §5.11 / §5.13）。路径仍由 `ANIMJ_SAVE_DIR` 统一覆盖，
//! 便于自动化验证把两者一起隔离到临时目录。
//!
//! 只有发布 bin（`animj`）注册 [`SettingsPlugin`]：编辑器窗口由 egui 宿主控制，
//! 不应被玩家的全屏偏好改写。

use std::path::PathBuf;

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use serde::{Deserialize, Serialize};

use crate::save;

/// 设置文件版本（`05` §5.13）。版本不符 ⇒ 忽略整份设置。
pub const SETTINGS_VERSION: u32 = 1;

/// 音量档位数：`0..=10` 档，每档 10%（`05` §5.13）。
const VOLUME_STEPS: i32 = 10;

/// 全局设置。`volume` 归一化到 `0.0..=1.0`，`fullscreen` 使用无边框全屏。
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSettings {
    pub version: u32,
    pub volume: f32,
    pub fullscreen: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            volume: 1.0,
            fullscreen: false,
        }
    }
}

impl GameSettings {
    /// 归一化到合法区间与档位（读档与运行期修改共用）。
    pub fn clamp(&mut self) {
        self.volume =
            (self.volume.clamp(0.0, 1.0) * VOLUME_STEPS as f32).round() / VOLUME_STEPS as f32;
    }

    /// 调整 `steps` 档（负数为减小），结果对齐档位并钳制在 `0..=1`。
    pub fn adjust_volume(&mut self, steps: i32) {
        let current = (self.volume * VOLUME_STEPS as f32).round() as i32;
        let next = (current + steps).clamp(0, VOLUME_STEPS);
        self.volume = next as f32 / VOLUME_STEPS as f32;
    }

    /// 音量百分比（UI 显示用，整数）。
    pub fn volume_percent(&self) -> i32 {
        (self.volume * 100.0).round() as i32
    }
}

/// 设置路径的**唯一**决定处：与存档同目录（`05` §5.11 / §5.13）。
pub fn settings_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("ANIMJ_SAVE_DIR") {
        return PathBuf::from(dir).join("settings.ron");
    }
    save::save_path().with_file_name("settings.ron")
}

/// 读取设置。文件不存在 ⇒ 默认值（不是错误）；版本不符或解析失败 ⇒ 默认值并 warn。
pub fn load() -> GameSettings {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return GameSettings::default();
    };
    match ron::from_str::<GameSettings>(&text) {
        Ok(mut settings) if settings.version == SETTINGS_VERSION => {
            settings.clamp();
            settings
        }
        Ok(settings) => {
            warn!(
                "设置版本 {} 与当前 {} 不符，忽略设置（05 §5.13）",
                settings.version, SETTINGS_VERSION
            );
            GameSettings::default()
        }
        Err(error) => {
            warn!("设置解析失败（{}）：{error}", path.display());
            GameSettings::default()
        }
    }
}

/// 原子写设置（复用存档的 `.tmp` + rename 规则，`05` §5.11）。
pub fn persist(settings: &GameSettings) {
    let path = settings_path();
    let text = ron::ser::to_string_pretty(
        settings,
        ron::ser::PrettyConfig::new()
            .indentor("    ")
            .new_line("\n"),
    )
    .unwrap_or_default();
    match save::write_atomic(&path, &text) {
        Ok(()) => info!("设置已写：{}", path.display()),
        Err(error) => warn!("设置写入失败（{}）：{error}", path.display()),
    }
}

/// 把设置投射到运行期：全局音量 + 主窗口显示模式。
///
/// `GlobalVolume` 只影响**之后创建**的音源（`10` §10.2.16），本项目的音效都是一次性
/// `PlaybackSettings::DESPAWN`，因此等价于即时生效。
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load())
            .init_resource::<GlobalVolume>()
            .add_systems(
                Update,
                apply_settings.run_if(resource_changed::<GameSettings>),
            )
            // 首次 `apply_settings` 只是把读到的值写进引擎，不应该反过来立刻写盘。
            .add_systems(Update, persist_changed_settings);
    }
}

fn apply_settings(
    settings: Res<GameSettings>,
    mut global: ResMut<GlobalVolume>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    global.volume = Volume::Linear(settings.volume);
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
        info!(
            "全屏：{}（音量 {}%）",
            if settings.fullscreen { "开" } else { "关" },
            settings.volume_percent()
        );
    } else {
        info!(
            "设置：音量 {}%，全屏 {}",
            settings.volume_percent(),
            if settings.fullscreen { "开" } else { "关" }
        );
    }
}

fn persist_changed_settings(settings: Res<GameSettings>, mut initialized: Local<bool>) {
    if !settings.is_changed() {
        return;
    }
    if !*initialized {
        *initialized = true;
        return;
    }
    persist(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_steps_snap_and_clamp_to_bounds() {
        let mut settings = GameSettings {
            volume: 0.55,
            ..default()
        };
        settings.adjust_volume(1);
        assert!((settings.volume - 0.7).abs() < 1e-6);
        settings.adjust_volume(-100);
        assert_eq!(settings.volume, 0.0);
        settings.adjust_volume(100);
        assert_eq!(settings.volume, 1.0);
        assert_eq!(settings.volume_percent(), 100);
    }

    #[test]
    fn loaded_values_are_normalized() {
        let mut settings = GameSettings {
            volume: 0.876,
            ..default()
        };
        settings.clamp();
        assert!((settings.volume - 0.9).abs() < 1e-6);
    }

    #[test]
    fn settings_roundtrip_keeps_values() {
        let settings = GameSettings {
            version: SETTINGS_VERSION,
            volume: 0.4,
            fullscreen: true,
        };
        let text = ron::ser::to_string(&settings).expect("serialize");
        let restored: GameSettings = ron::from_str(&text).expect("deserialize");
        assert_eq!(restored, settings);
    }
}
