//! 资产根路径。**所有者文档：`docs/09-dependencies.md` §9.6**。
//!
//! 仓库根的 `assets/` 由两个 bin 共用；Bevy 的 `get_base_path()` 会优先用 `CARGO_MANIFEST_DIR`
//! 环境变量，而它在 `cargo run` / `cargo test` 下指向**当前包**的目录（`crates/game`），
//! 因此必须显式覆盖 `AssetPlugin.file_path`，否则会去 `crates/game/assets/` 找文件。

use bevy::prelude::*;
use std::path::Path;

/// 解析资产根路径。**路径算术只允许出现在这一处**（`09` §9.6.2 规则 A1）。
///
/// - 开发期：`CARGO_MANIFEST_DIR` = `crates/<crate>`，上跳两级到仓库根再拼 `assets/`。
/// - 发布期：该变量不存在，返回 `"assets"`，即「exe 同级的 `assets/`」。
pub fn asset_file_path() -> String {
    match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(manifest_dir) => Path::new(&manifest_dir)
            .join("..")
            .join("..")
            .join("assets")
            .to_string_lossy()
            .into_owned(),
        None => "assets".to_string(),
    }
}

/// 贴图文件的修改时间（外部改动检测，`docs/04-assets.md` §5.6）。
///
/// 返回 `None` 表示文件不存在或无法读取；调用方把它当作"尚未观察到"，
/// **不要**把它当作"没变化"（新增文件时也必须能触发一次重载）。
pub fn texture_stamp(path: &str) -> Option<std::time::SystemTime> {
    let relative = level_format::normalize_asset_path(path)?;
    let full = Path::new(&asset_file_path()).join(&relative);
    std::fs::metadata(full)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// 把资产根路径暴露成资源，供 HUD / 编辑器 / 诊断显示。
#[derive(Resource, Debug, Clone)]
pub struct AssetRoot(pub String);

/// 关卡文件后缀（`00` §0.5）。文件名 `<level_id>.level.ron`。
pub const LEVEL_FILE_SUFFIX: &str = ".level.ron";

/// 文件名 → 关卡 id。不是 `<id>.level.ron`（或 id 为空）⇒ `None`。
pub fn level_id_from_file_name(name: &str) -> Option<&str> {
    name.strip_suffix(LEVEL_FILE_SUFFIX)
        .filter(|id| !id.is_empty())
}

/// 扫描 `assets/levels/` 下的关卡 id，按字典序返回。
///
/// 目录不存在 / 不可读 ⇒ 空列表（菜单显示 `NO LEVELS FOUND`），不 panic。
/// 只认文件名后缀，不解析关卡内容：损坏的关卡仍会出现在列表里，加载时按正常路径报错。
pub fn level_ids() -> Vec<String> {
    let dir = std::path::Path::new(&asset_file_path()).join("levels");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| level_id_from_file_name(&name).map(str::to_owned))
        .collect();
    ids.sort();
    ids
}

pub struct AssetPathPlugin;

impl Plugin for AssetPathPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AssetRoot(asset_file_path()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_level_ron_files_map_to_ids() {
        assert_eq!(
            level_id_from_file_name("world_01.level.ron"),
            Some("world_01")
        );
        assert_eq!(level_id_from_file_name("a.level.ron"), Some("a"));
        assert_eq!(level_id_from_file_name("world_01.ron"), None);
        assert_eq!(level_id_from_file_name(".level.ron"), None);
        assert_eq!(level_id_from_file_name("world_01.level.ron.bak"), None);
        assert_eq!(level_id_from_file_name("world_01.level.RON"), None);
    }
}
