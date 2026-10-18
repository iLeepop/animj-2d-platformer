//! 关卡资产的注册与加载。见 `docs/03-level-format.md` §3.1、`docs/05-architecture.md` §5.3。
//!
//! loader 注册在扩展名 `ron` 上：Bevy 按路径**最后一个** `.` 后的扩展名匹配 loader，
//! 因此 `*.level.ron` 的双后缀只是给人看的（`03` §3.1 的唯一性约束：项目内只有关卡片用 `.ron`）。

use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetLoader, LoadContext};
use bevy::prelude::*;
use level_format::{Level, LevelError};

/// 关卡的 Bevy 资产包装。
///
/// `level_format::Level` 不依赖 bevy，不能直接 `#[derive(Asset)]`，因此用 newtype 隔开。
#[derive(Asset, TypePath, Clone, Debug)]
pub struct LevelAsset {
    pub level: Level,
}

/// 加载失败的原因。`AssetLoader::Error` 只需 `Into<BevyError>`，`BevyError` 可吸收任何
/// `std::error::Error + Send + Sync + 'static`，因此这里用最小自定义类型，不引第三方错误库（`00` D15）。
#[derive(Debug)]
pub struct LevelLoadError {
    message: String,
}

impl LevelLoadError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for LevelLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LevelLoadError {}

impl From<LevelError> for LevelLoadError {
    fn from(value: LevelError) -> Self {
        Self::new(value.to_string())
    }
}

#[derive(Default, TypePath)]
pub struct LevelLoader;

impl AssetLoader for LevelLoader {
    type Asset = LevelAsset;
    type Settings = ();
    type Error = LevelLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|err| LevelLoadError::new(format!("读取关卡文件失败：{err}")))?;
        let text = String::from_utf8(bytes)
            .map_err(|err| LevelLoadError::new(format!("关卡文件不是合法 UTF-8：{err}")))?;
        let level = level_format::read(&text)?;
        Ok(LevelAsset { level })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

pub struct LevelAssetPlugin;

impl Plugin for LevelAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<LevelAsset>()
            .init_asset_loader::<LevelLoader>();
    }
}
