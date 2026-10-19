//! 角色动画数据（外部资产，工具可编辑）。所有者文档：`docs/04-assets.md` §5.5。
//!
//! 图集只描述"像素在哪"，**播放规则**（每状态用哪些帧、每帧多久、是否循环）外置到
//! `assets/anim/player.anim.ron`；缺文件/解析失败一律回退到 [`PlayerAnimSet::builtin`]
//! （与改造前的常量完全一致），因此"没有这个文件"也是合法状态。

use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetLoader, LoadContext};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::render::sprite_anim::AnimRow;

/// 动画数据的相对路径（`assets/` 之下）。
pub const ANIM_PATH: &str = "anim/player.anim.ron";
/// 当前数据版本（与 `04` §5.5 一致）。
pub const ANIM_VERSION: u32 = 2;
/// 单帧时长的合法区间（秒，`04` §5.5）。
pub const MIN_FRAME_SECS: f32 = 0.02;
pub const MAX_FRAME_SECS: f32 = 2.0;

/// 一个状态的动画定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateAnim {
    /// `idle` / `run` / `jump` / `fall`（与 `06` 的选行规则一一对应）。
    pub name: String,
    /// 该状态自己的贴图（相对 assets/），每格 32×32。
    pub sheet: String,
    /// 图集列数（行数按最大帧号自动推导）。
    pub columns: u32,
    /// 图集索引序列（行主序）；允许重复与重排。
    pub frames: Vec<usize>,
    /// 每帧时长（秒）。
    pub frame_secs: f32,
    /// 循环，还是停在最后一帧。
    pub looping: bool,
}

/// 整份动画数据。
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerAnimSet {
    pub version: u32,
    pub states: Vec<StateAnim>,
}

impl PlayerAnimSet {
    /// 内置默认值：与改造前的常量逐项一致（`04` §5.2）。
    pub fn builtin() -> Self {
        let state = |name: &str, frames: Vec<usize>, frame_secs: f32, looping: bool| StateAnim {
            name: name.to_string(),
            sheet: "sprites/player/cat_sheet.png".to_string(),
            columns: 6,
            frames,
            frame_secs,
            looping,
        };
        Self {
            version: ANIM_VERSION,
            states: vec![
                state("idle", vec![0, 1, 2, 3], 0.20, true),
                state("run", vec![6, 7, 8, 9, 10, 11], 0.08, true),
                state("jump", vec![12, 13], 0.10, false),
                state("fall", vec![18, 19], 0.12, false),
            ],
        }
    }

    /// 某一行（物理状态）对应的动画；数据缺失时回退到内置值。
    pub fn state(&self, row: AnimRow) -> StateAnim {
        self.states
            .iter()
            .find(|state| state.name == row.name())
            .cloned()
            .unwrap_or_else(|| {
                Self::builtin()
                    .states
                    .into_iter()
                    .find(|state| state.name == row.name())
                    .expect("内置数据必含四个状态")
            })
    }

    /// 校验：版本、状态齐全、帧非空且越界检查、时长区间。
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.version != ANIM_VERSION {
            issues.push(format!("版本 {} ≠ {ANIM_VERSION}", self.version));
        }
        for row in [AnimRow::Idle, AnimRow::Run, AnimRow::Jump, AnimRow::Fall] {
            match self.states.iter().find(|state| state.name == row.name()) {
                None => issues.push(format!("缺少状态 `{}`", row.name())),
                Some(state) => {
                    if state.sheet.trim().is_empty() {
                        issues.push(format!("状态 `{}` 的贴图路径为空", state.name));
                    }
                    if state.columns == 0 {
                        issues.push(format!("状态 `{}` 的列数不能为 0", state.name));
                    }
                    if state.frames.is_empty() {
                        issues.push(format!("状态 `{}` 的帧序列为空", state.name));
                    }
                    if !(MIN_FRAME_SECS..=MAX_FRAME_SECS).contains(&state.frame_secs) {
                        issues.push(format!(
                            "状态 `{}` 的帧时长 {} 不在 {MIN_FRAME_SECS}..={MAX_FRAME_SECS}",
                            state.name, state.frame_secs
                        ));
                    }
                }
            }
        }
        issues
    }
}

/// `AnimRow` 与数据里的状态名。
impl AnimRow {
    pub fn name(self) -> &'static str {
        match self {
            AnimRow::Idle => "idle",
            AnimRow::Run => "run",
            AnimRow::Jump => "jump",
            AnimRow::Fall => "fall",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "idle" => Some(AnimRow::Idle),
            "run" => Some(AnimRow::Run),
            "jump" => Some(AnimRow::Jump),
            "fall" => Some(AnimRow::Fall),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct AnimLoadError(String);

impl core::fmt::Display for AnimLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for AnimLoadError {}

#[derive(Default, TypePath)]
pub struct AnimLoader;

impl AssetLoader for AnimLoader {
    type Asset = PlayerAnimSet;
    type Settings = ();
    type Error = AnimLoadError;

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
            .map_err(|error| AnimLoadError(format!("读取失败：{error}")))?;
        let text = String::from_utf8(bytes)
            .map_err(|error| AnimLoadError(format!("不是 UTF-8：{error}")))?;
        let set: PlayerAnimSet =
            ron::from_str(&text).map_err(|error| AnimLoadError(format!("解析失败：{error}")))?;
        let issues = set.validate();
        if !issues.is_empty() {
            return Err(AnimLoadError(format!(
                "动画数据校验失败：{}",
                issues.join("；")
            )));
        }
        Ok(set)
    }

    fn extensions(&self) -> &[&str] {
        &["anim.ron"]
    }
}

/// 当前生效的动画数据（缺文件时为内置默认）。
#[derive(Resource, Debug, Clone)]
pub struct ActiveAnim {
    pub set: PlayerAnimSet,
    pub source: Option<Handle<PlayerAnimSet>>,
    /// 每次数据变更 +1；下游（贴图/图集布局）据此重建。
    pub revision: u64,
}

impl Default for ActiveAnim {
    fn default() -> Self {
        Self {
            set: PlayerAnimSet::builtin(),
            source: None,
            revision: 0,
        }
    }
}

pub struct AnimDataPlugin;

impl Plugin for AnimDataPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<PlayerAnimSet>()
            .init_asset_loader::<AnimLoader>()
            .init_resource::<ActiveAnim>()
            .add_systems(Startup, load_anim)
            .add_systems(Update, (adopt_loaded_anim, watch_anim_file));
    }
}

/// 动画文件热重载：编辑器保存后**无需重启**即可生效（编辑器与游戏共用本系统）。
fn watch_anim_file(mut active: ResMut<ActiveAnim>, mut last: Local<Option<std::time::SystemTime>>) {
    let path = std::path::Path::new(&crate::assets::asset_file_path()).join(ANIM_PATH);
    let Ok(modified) = std::fs::metadata(&path).and_then(|meta| meta.modified()) else {
        return;
    };
    if last.as_ref() == Some(&modified) {
        return;
    }
    *last = Some(modified);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    match ron::from_str::<PlayerAnimSet>(&text) {
        Ok(set) if set == active.set => {}
        Ok(set) => {
            info!("动画数据热重载：{} 个状态", set.states.len());
            active.set = set;
            active.revision = active.revision.wrapping_add(1);
        }
        Err(error) => error!("动画数据热重载失败（保留旧数据）：{error}"),
    }
}

fn load_anim(mut commands: Commands, assets: Res<AssetServer>) {
    let handle = assets.load::<PlayerAnimSet>(ANIM_PATH);
    commands.insert_resource(PendingAnim(handle));
}

#[derive(Resource)]
struct PendingAnim(Handle<PlayerAnimSet>);

/// 资产加载/热重载 ⇒ 更新 [`ActiveAnim`]；失败则保留内置默认（只 warn）。
fn adopt_loaded_anim(
    pending: Option<Res<PendingAnim>>,
    sets: Res<Assets<PlayerAnimSet>>,
    mut active: ResMut<ActiveAnim>,
    mut events: MessageReader<AssetEvent<PlayerAnimSet>>,
) {
    let Some(pending) = pending else {
        return;
    };
    let changed = events
        .read()
        .any(|event| matches!(event, AssetEvent::Added { id } | AssetEvent::Modified { id } if *id == pending.0.id()));
    if !changed && active.source.is_some() {
        return;
    }
    let Some(set) = sets.get(&pending.0) else {
        return;
    };
    if active.source.as_ref() == Some(&pending.0) && active.set == *set {
        return;
    }
    info!("动画数据已加载：{} 个状态", set.states.len());
    active.set = set.clone();
    active.source = Some(pending.0.clone());
    active.revision = active.revision.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_matches_the_documented_table() {
        let set = PlayerAnimSet::builtin();
        assert!(set.validate().is_empty());
        let idle = set.state(AnimRow::Idle);
        assert_eq!(idle.frames, vec![0, 1, 2, 3]);
        assert_eq!(idle.frame_secs, 0.20);
        assert!(idle.looping);
        let run = set.state(AnimRow::Run);
        assert_eq!(run.frames.len(), 6);
        let jump = set.state(AnimRow::Jump);
        assert!(!jump.looping);
        assert_eq!(set.state(AnimRow::Fall).frames, vec![18, 19]);
    }

    #[test]
    fn validation_rejects_empty_frames_missing_sheet_and_bad_timing() {
        let mut set = PlayerAnimSet::builtin();
        set.states[0].frames.clear();
        set.states[1].sheet.clear();
        set.states[2].frame_secs = 5.0;
        let issues = set.validate();
        assert_eq!(issues.len(), 3, "{issues:?}");

        let mut set = PlayerAnimSet::builtin();
        set.states.pop(); // 缺 fall
        assert!(set.validate().iter().any(|issue| issue.contains("fall")));

        let mut set = PlayerAnimSet::builtin();
        set.version = 9;
        assert!(set.validate().iter().any(|issue| issue.contains("版本")));
    }

    #[test]
    fn missing_state_falls_back_to_builtin() {
        let mut set = PlayerAnimSet::builtin();
        set.states.clear();
        assert_eq!(set.state(AnimRow::Run).frames, vec![6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn state_names_round_trip() {
        for row in [AnimRow::Idle, AnimRow::Run, AnimRow::Jump, AnimRow::Fall] {
            assert_eq!(AnimRow::from_name(row.name()), Some(row));
        }
        assert_eq!(AnimRow::from_name("bogus"), None);
    }
}
