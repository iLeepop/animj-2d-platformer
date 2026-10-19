//! UI 字体加载。所有者文档：`docs/04-assets.md` §9。
//!
//! 中文 UI 必须有含 CJK 字形的字体；Bevy 的默认字体只有 ASCII，缺字形时整行后续文本都会被丢掉
//! （`10` §10.3.1 E7）。仓库当前没有提交字体资产，因此按以下优先级查找：
//!
//! 1. `ANIMJ_UI_FONT` 环境变量指向的字体文件（自动化/自定义字体）；
//! 2. `assets/fonts/` 下按文件名字典序的第一个 `.ttf` / `.otf` / `.ttc`（将来入库的正式字体）；
//! 3. 常见系统 CJK 字体（开发期；Windows/macOS/Linux 各一组）。
//!
//! 正式发布应把字体放进 `assets/fonts/`，避免依赖用户机器（`04` §9）。

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::text::{Font, FontSize};

use crate::assets::asset_file_path;

/// 游戏 UI 使用的字体句柄。`None` ⇒ 回退到 Bevy 默认字体（仅 ASCII）。
#[derive(Resource, Debug, Clone, Default)]
pub struct UiFont(pub Option<Handle<Font>>);

impl UiFont {
    /// 构造一个使用本字体的 `TextFont`。`None` 时用默认字体，仅保证布局不 panic。
    pub fn text_font(&self, size: f32) -> TextFont {
        let mut font = TextFont {
            font_size: FontSize::Px(size),
            ..default()
        };
        if let Some(handle) = &self.0 {
            font.font = handle.clone().into();
        }
        font
    }
}

pub struct UiFontPlugin;

impl Plugin for UiFontPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiFont>()
            .add_systems(PreStartup, load_ui_font);
    }
}

fn load_ui_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let Some((path, bytes)) = find_font_file() else {
        warn!(
            "未找到可用 UI 字体：中文文本将无法显示。把 .ttf 放进 assets/fonts/ 或设置 ANIMJ_UI_FONT（04 §9）"
        );
        return;
    };
    let handle = fonts.add(Font::from_bytes(bytes));
    info!("UI 字体已加载：{}", path.display());
    commands.insert_resource(UiFont(Some(handle)));
}

/// 按优先级返回第一个可读的字体文件及其字节。
fn find_font_file() -> Option<(PathBuf, Vec<u8>)> {
    for path in candidate_fonts() {
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((path, bytes));
        }
    }
    None
}

fn candidate_fonts() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("ANIMJ_UI_FONT") {
        paths.push(PathBuf::from(path));
    }
    paths.extend(asset_fonts());
    paths.extend(system_font_candidates());
    paths
}

/// `assets/fonts/` 下的字体，按路径排序（多个字体时结果确定）。
fn asset_fonts() -> Vec<PathBuf> {
    let dir = Path::new(&asset_file_path()).join("fonts");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_font_file(path))
        .collect();
    paths.sort();
    paths
}

fn is_font_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("ttf" | "otf" | "ttc")
    )
}

#[cfg(target_os = "windows")]
fn system_font_candidates() -> Vec<PathBuf> {
    [
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simkai.ttf",
        "C:/Windows/Fonts/simfang.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "macos")]
fn system_font_candidates() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn system_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_font_extensions_are_accepted() {
        assert!(is_font_file(Path::new("a.ttf")));
        assert!(is_font_file(Path::new("a.OTF")));
        assert!(is_font_file(Path::new("a.ttc")));
        assert!(!is_font_file(Path::new("a.png")));
        assert!(!is_font_file(Path::new("a")));
    }
}
