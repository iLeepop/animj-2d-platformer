//! 编辑态热重载消费（`08` §8.7.3 / §8.7.4）。
//!
//! 只做「检测 + 交给用户确认」：内容与 `saved_text`/`ignored_text` 相同则静默（自写自读抑制）；
//! 应用/忽略由 UI 决定，本模块不直接改 `doc`。

use bevy::asset::AssetEvent;
use bevy::prelude::*;

use crate::session::EditorSession;

pub fn watch_external_changes(
    mut events: MessageReader<AssetEvent<game::level_asset::LevelAsset>>,
    assets: Res<Assets<game::level_asset::LevelAsset>>,
    loaded: Option<Res<game::world::load::LoadedLevel>>,
    mut session: Option<ResMut<EditorSession>>,
) {
    let (Some(loaded), Some(session)) = (loaded, session.as_mut()) else {
        return;
    };
    let modified = events
        .read()
        .any(|event| matches!(event, AssetEvent::Modified { id } if *id == loaded.handle.id()));
    if !modified {
        return;
    }
    let Some(asset) = assets.get(&loaded.handle) else {
        return;
    };
    let text = level_format::write(&asset.level);
    // 自写自读抑制：与最后一次加载/写盘、或用户已忽略的文本一致 ⇒ 静默
    if session.saved_text.as_deref() == Some(text.as_str())
        || session.ignored_text.as_deref() == Some(text.as_str())
    {
        return;
    }
    info!("检测到外部关卡改动，等待用户确认（`08` §8.7.4）");
    session.pending_external = Some(asset.level.clone());
}
