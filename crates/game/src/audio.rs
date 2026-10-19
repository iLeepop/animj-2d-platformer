//! 音效：事件 → cue → `bevy_audio`。所有者文档：`docs/04-assets.md` §6.9。
//!
//! 首批音效一律是**程序化占位 WAV**（`animj-dev gen-placeholder-assets` 生成）；缺文件只会让
//! `bevy_asset` 打一条 warn，不 panic。播放使用一次性实体 + `PlaybackSettings::DESPAWN`。

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::prelude::*;

use crate::player::PlayerStepOutcome;
use crate::player::death::PlayerDied;
use crate::room::{CollectiblePicked, DecorationGranted, RoomCleared};
use crate::state::{GameSet, GameState};

/// 首批 cue（`04` §6.9.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCue {
    Jump,
    Land,
    Bump,
    Pickup,
    Clear,
    Decoration,
    Death,
}

impl AudioCue {
    /// 资产相对路径（`04` §6.9：`assets/audio/<cue>.wav`）。
    pub const fn file(self) -> &'static str {
        match self {
            AudioCue::Jump => "audio/jump.wav",
            AudioCue::Land => "audio/land.wav",
            AudioCue::Bump => "audio/bump.wav",
            AudioCue::Pickup => "audio/pickup.wav",
            AudioCue::Clear => "audio/clear.wav",
            AudioCue::Decoration => "audio/decoration.wav",
            AudioCue::Death => "audio/death.wav",
        }
    }
}

/// 由同一步的物理结果选**一个**音效（纯函数）：起跳 > 撞头 > 落地。
pub fn cue_for_outcome(jumped: bool, landed: bool, bumped: bool) -> Option<AudioCue> {
    if jumped {
        Some(AudioCue::Jump)
    } else if bumped {
        Some(AudioCue::Bump)
    } else if landed {
        Some(AudioCue::Land)
    } else {
        None
    }
}

pub struct AudioFxPlugin;

impl Plugin for AudioFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (play_player_fx, play_room_fx)
                .in_set(GameSet::Fx)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn spawn_cue(commands: &mut Commands, assets: &AssetServer, cue: AudioCue) {
    let handle = assets.load::<AudioSource>(cue.file().to_string());
    commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
    info!("音效：{}", cue.file());
}

fn play_player_fx(
    mut commands: Commands,
    assets: Res<AssetServer>,
    outcomes: Query<&PlayerStepOutcome, Changed<PlayerStepOutcome>>,
) {
    for outcome in &outcomes {
        if let Some(cue) =
            cue_for_outcome(outcome.0.jumped, outcome.0.landed, outcome.0.head_bumped)
        {
            spawn_cue(&mut commands, &assets, cue);
        }
    }
}

fn play_room_fx(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut picked: MessageReader<CollectiblePicked>,
    mut cleared: MessageReader<RoomCleared>,
    mut granted: MessageReader<DecorationGranted>,
    mut died: MessageReader<PlayerDied>,
) {
    let picked_any = picked.read().count() > 0;
    let cleared_any = cleared.read().count() > 0;
    let granted_any = granted.read().count() > 0;
    let died_any = died.read().count() > 0;
    for (triggered, cue) in [
        (picked_any, AudioCue::Pickup),
        (cleared_any, AudioCue::Clear),
        (granted_any, AudioCue::Decoration),
        (died_any, AudioCue::Death),
    ] {
        if triggered {
            spawn_cue(&mut commands, &assets, cue);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_paths_follow_the_asset_convention() {
        for cue in [
            AudioCue::Jump,
            AudioCue::Land,
            AudioCue::Bump,
            AudioCue::Pickup,
            AudioCue::Clear,
            AudioCue::Decoration,
            AudioCue::Death,
        ] {
            let path = cue.file();
            assert!(path.starts_with("audio/"), "{path}");
            assert!(path.ends_with(".wav"), "{path}");
            let stem = path.trim_start_matches("audio/").trim_end_matches(".wav");
            assert!(
                stem.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "cue 必须是 snake_case：{stem}"
            );
        }
    }

    #[test]
    fn outcome_mapping_prioritizes_jump_then_bump_then_land() {
        assert_eq!(cue_for_outcome(true, true, true), Some(AudioCue::Jump));
        assert_eq!(cue_for_outcome(false, true, true), Some(AudioCue::Bump));
        assert_eq!(cue_for_outcome(false, true, false), Some(AudioCue::Land));
        assert_eq!(cue_for_outcome(false, false, false), None);
    }
}
