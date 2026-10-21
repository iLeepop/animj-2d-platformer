//! 资产根路径。**所有者文档：`docs/09-dependencies.md` §9.6**。
//!
//! 仓库根的 `assets/` 由两个 bin 共用；Bevy 的 `get_base_path()` 会优先用 `CARGO_MANIFEST_DIR`
//! 环境变量，而它在 `cargo run` / `cargo test` 下指向**当前包**的目录（`crates/game`），
//! 因此必须显式覆盖 `AssetPlugin.file_path`，否则会去 `crates/game/assets/` 找文件。

use bevy::prelude::*;
use std::path::{Path, PathBuf};

/// 解析资产根路径（**绝对路径**）。**路径算术只允许出现在这一处**（`09` §9.6.2 规则 A1）。
///
/// 优先级（`09` §9.6.3）：
/// 1. **exe 同级存在 `assets/`** ⇒ 发布布局，直接用它。即使父环境残留
///    `CARGO_MANIFEST_DIR`，打包产物也只认自己旁边的资产。
/// 2. `CARGO_MANIFEST_DIR`（`cargo run` / `cargo test` 注入，指向 `crates/<crate>`）
///    ⇒ 上跳两级到仓库根。
/// 3. `BEVY_ASSET_ROOT` ⇒ `<root>/assets`。
/// 4. 可执行文件目录 ⇒ `<exe dir>/assets`。
///
/// 必须返回绝对路径：Bevy 的 `AssetServer` 与直接用 `std::fs` 的读取方（贴图直读、
/// 关卡列表、字体扫描）因此看到同一个根目录，且都不受进程当前工作目录影响。
pub fn asset_file_path() -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
    let bevy_asset_root = std::env::var_os("BEVY_ASSET_ROOT").map(PathBuf::from);
    resolve_asset_path(
        exe_dir.as_deref(),
        manifest_dir.as_deref(),
        bevy_asset_root.as_deref(),
    )
    .to_string_lossy()
    .into_owned()
}

/// [`asset_file_path`] 的纯决策部分（文件系统只用于判断 exe 同级是否有 `assets/`）。
fn resolve_asset_path(
    exe_dir: Option<&Path>,
    manifest_dir: Option<&Path>,
    bevy_asset_root: Option<&Path>,
) -> PathBuf {
    if let Some(dir) = exe_dir {
        let sibling = dir.join("assets");
        if sibling.is_dir() {
            return sibling;
        }
    }
    if let Some(manifest_dir) = manifest_dir {
        return manifest_dir.join("..").join("..").join("assets");
    }
    if let Some(root) = bevy_asset_root {
        return root.join("assets");
    }
    exe_dir
        .map(|dir| dir.join("assets"))
        .unwrap_or_else(|| PathBuf::from("assets"))
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

    fn temp_exe_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("animj-assets-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建临时 exe 目录");
        dir
    }

    /// 发布布局：exe 同级有 `assets/` 时优先用它，不受残留的 `CARGO_MANIFEST_DIR` 影响。
    #[test]
    fn exe_sibling_assets_win_over_manifest() {
        let exe = temp_exe_dir("sibling");
        std::fs::create_dir_all(exe.join("assets")).unwrap();
        let manifest = Path::new("/repo/crates/game");

        assert_eq!(
            resolve_asset_path(Some(&exe), Some(manifest), None),
            exe.join("assets")
        );
        std::fs::remove_dir_all(&exe).unwrap();
    }

    /// 开发布局：exe 同级没有 `assets/` 时用 `CARGO_MANIFEST_DIR/../../assets`。
    #[test]
    fn dev_layout_uses_manifest_root() {
        let exe = temp_exe_dir("dev");
        let manifest = Path::new("/repo/crates/game");

        assert_eq!(
            resolve_asset_path(Some(&exe), Some(manifest), None),
            manifest.join("..").join("..").join("assets")
        );
        std::fs::remove_dir_all(&exe).unwrap();
    }

    /// 裸跑 exe（无环境变量）：解析成 exe 同级的绝对路径，与 cwd 无关。
    #[test]
    fn release_layout_is_absolute_and_independent_of_cwd() {
        let exe = temp_exe_dir("release");

        let path = resolve_asset_path(Some(&exe), None, None);
        assert_eq!(path, exe.join("assets"));
        assert!(path.is_absolute());
        std::fs::remove_dir_all(&exe).unwrap();
    }

    /// 没有 manifest 时，`BEVY_ASSET_ROOT` 仍然生效（与 Bevy 的 base path 语义一致）。
    #[test]
    fn bevy_asset_root_is_honored_without_manifest() {
        let exe = temp_exe_dir("bevy-root");

        assert_eq!(
            resolve_asset_path(Some(&exe), None, Some(Path::new("/custom/root"))),
            PathBuf::from("/custom/root").join("assets")
        );
        std::fs::remove_dir_all(&exe).unwrap();
    }

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
