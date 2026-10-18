//! 角色帧动画（自写）。所有者文档：`docs/04-assets.md` §5、`docs/06-physics-and-movement.md`
//! （动画选择由 `Grounded` / 速度符号驱动）。
//!
//! `bevy_animation` **不支持** sprite 帧动画（`04` §5.1），因此用 `AnimationIndices + AnimationTimer +
//! TextureAtlas.index` 三段式；帧率与图集区间全部来自 `04` §5.2，本模块不新增常量值。

use bevy::prelude::*;

use crate::config::render::PLAYER_SHEET_COLUMNS;
use crate::player::{PlayerFacing, PlayerMotion};

/// 动画行（`04` §5.2：行序 = `idle / run / jump / fall`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimRow {
    #[default]
    Idle = 0,
    Run = 1,
    Jump = 2,
    Fall = 3,
}

impl AnimRow {
    pub const COUNT: usize = 4;

    pub fn first(self) -> usize {
        self as usize * PLAYER_SHEET_COLUMNS as usize
    }

    pub fn last(self) -> usize {
        self.first() + frame_count(self) - 1
    }

    /// 单帧时长（秒/帧，`04` §5.2）。
    pub fn frame_secs(self) -> f32 {
        match self {
            AnimRow::Idle => 0.20,
            AnimRow::Run => 0.08,
            AnimRow::Jump => 0.10,
            AnimRow::Fall => 0.12,
        }
    }

    /// 循环播放（`jump` / `fall` 停在最后一帧）。
    pub fn looping(self) -> bool {
        matches!(self, AnimRow::Idle | AnimRow::Run)
    }

    /// 内置默认的帧序列（`04` §5.2 的区间展开）。
    pub fn builtin_frames(self) -> Vec<usize> {
        (self.first()..=self.last()).collect()
    }
}

/// 每行的有效帧数（`04` §5.2）。
pub fn frame_count(row: AnimRow) -> usize {
    match row {
        AnimRow::Idle => 4,
        AnimRow::Run => 6,
        AnimRow::Jump => 2,
        AnimRow::Fall => 2,
    }
}

/// 当前动画的帧序列（图集索引）与游标。数据来自 `anim_data::PlayerAnimSet`（`04` §5.5）。
#[derive(Component, Debug, Clone, PartialEq)]
pub struct AnimationFrames {
    pub frames: Vec<usize>,
    pub cursor: usize,
    pub looping: bool,
}

impl AnimationFrames {
    pub fn new(frames: Vec<usize>, looping: bool) -> Self {
        let frames = if frames.is_empty() { vec![0] } else { frames };
        Self {
            frames,
            cursor: 0,
            looping,
        }
    }

    pub fn current(&self) -> usize {
        self.frames.get(self.cursor).copied().unwrap_or(0)
    }
}

#[derive(Component, Debug, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer);

impl AnimationTimer {
    pub fn for_row(row: AnimRow) -> Self {
        Self::for_state(row.frame_secs())
    }

    pub fn for_state(frame_secs: f32) -> Self {
        Self(Timer::from_seconds(frame_secs.max(0.01), TimerMode::Repeating))
    }
}

/// 当前动画行（驱动用；变化时才重置计时器与索引区间）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CurrentAnimation(pub AnimRow);

/// 推进游标：循环回到 0，单次停在最后一帧。
pub fn next_cursor(cursor: usize, len: usize, looping: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if cursor + 1 >= len {
        if looping { 0 } else { len - 1 }
    } else {
        cursor + 1
    }
}

/// 由物理状态选行（`06`：`grounded` 与速度符号）。
///
/// 水平速度小于 `RUN_THRESHOLD` 时即使在地面也播 `idle`（避免静止时原地跑步）。
pub const RUN_THRESHOLD: f32 = 1.0;

pub fn select_row(grounded: bool, vel: Vec2) -> AnimRow {
    if !grounded {
        if vel.y > 0.0 {
            AnimRow::Jump
        } else {
            AnimRow::Fall
        }
    } else if vel.x.abs() >= RUN_THRESHOLD {
        AnimRow::Run
    } else {
        AnimRow::Idle
    }
}

pub struct SpriteAnimPlugin;

impl Plugin for SpriteAnimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (drive_player_animation, animate_sprite).chain());
    }
}

/// 按物理状态切换动画行（只在变化时重置，避免每帧打断帧序）。
fn drive_player_animation(
    anim: Res<crate::anim_data::ActiveAnim>,
    sheets: Res<crate::player::spawn::PlayerSheet>,
    players: Query<
        (
            &PlayerMotion,
            &mut CurrentAnimation,
            &mut AnimationFrames,
            &mut AnimationTimer,
            &mut Sprite,
            &PlayerFacing,
        ),
        Or<(
            Changed<PlayerMotion>,
            Added<CurrentAnimation>,
            Changed<AnimationTimer>,
            Changed<PlayerFacing>,
        )>,
    >,
) {
    for (motion, mut current, mut frames, mut timer, mut sprite, facing) in players {
        let body = &motion.0;
        let row = select_row(body.grounded, body.vel);
        let state = anim.set.state(row);
        // 只在**状态变化**时重置帧序列；图集同步必须每帧做——
        // 否则编辑器热重载改了贴图/列数后，玩家要等下一次状态切换才换布局（实机 bug）。
        if current.0 != row {
            current.0 = row;
            *frames = AnimationFrames::new(state.frames, state.looping);
            *timer = AnimationTimer::for_state(state.frame_secs);
        }
        // 每状态独立贴图：切状态时同时换图集图与布局
        let target = sheets
            .states
            .iter()
            .find(|(name, _, _)| name == &state.name)
            .map(|(_, image, layout)| (image.clone(), layout.clone()))
            .unwrap_or((sheets.image.clone(), sheets.layout.clone()));
        if sprite.image != target.0 {
            sprite.image = target.0;
        }
        if sprite.texture_atlas.as_ref().map(|atlas| atlas.layout.clone()) != Some(target.1.clone())
            && let Some(atlas) = sprite.texture_atlas.as_mut()
        {
            atlas.layout = target.1;
        }
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = frames.current();
        }
        // 朝向：默认素材朝右（`03` §3.4），向左移动时水平翻转（`01` §1.3）。
        let flip_x = !facing.0;
        if sprite.flip_x != flip_x {
            sprite.flip_x = flip_x;
        }
    }
}

fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(&mut AnimationFrames, &mut AnimationTimer, &mut Sprite)>,
) {
    for (mut frames, mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if !timer.just_finished() {
            continue;
        }
        frames.cursor = next_cursor(frames.cursor, frames.frames.len(), frames.looping);
        let index = frames.current();
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim_data::PlayerAnimSet;

    #[test]
    fn builtin_rows_match_the_spec_ranges() {
        assert_eq!(AnimRow::Idle.builtin_frames(), vec![0, 1, 2, 3]);
        assert_eq!(AnimRow::Run.builtin_frames(), vec![6, 7, 8, 9, 10, 11]);
        assert_eq!(AnimRow::Jump.builtin_frames(), vec![12, 13]);
        assert_eq!(AnimRow::Fall.builtin_frames(), vec![18, 19]);
        assert_eq!(AnimRow::COUNT, 4);
    }

    #[test]
    fn frame_times_match_the_spec() {
        assert_eq!(AnimRow::Idle.frame_secs(), 0.20);
        assert_eq!(AnimRow::Run.frame_secs(), 0.08);
        assert_eq!(AnimRow::Jump.frame_secs(), 0.10);
        assert_eq!(AnimRow::Fall.frame_secs(), 0.12);
    }

    #[test]
    fn looping_advances_and_single_shot_stops() {
        assert_eq!(next_cursor(0, 4, true), 1);
        assert_eq!(next_cursor(3, 4, true), 0);
        assert_eq!(next_cursor(0, 2, false), 1);
        assert_eq!(next_cursor(1, 2, false), 1);
        assert_eq!(next_cursor(0, 0, true), 0);
    }

    #[test]
    fn row_selection_follows_physics() {
        assert_eq!(select_row(true, Vec2::ZERO), AnimRow::Idle);
        assert_eq!(select_row(true, Vec2::new(120.0, 0.0)), AnimRow::Run);
        assert_eq!(select_row(true, Vec2::new(-120.0, 0.0)), AnimRow::Run);
        assert_eq!(select_row(false, Vec2::new(0.0, 200.0)), AnimRow::Jump);
        assert_eq!(select_row(false, Vec2::new(0.0, -200.0)), AnimRow::Fall);
        assert_eq!(select_row(true, Vec2::new(0.5, 0.0)), AnimRow::Idle);
    }

    /// 数据驱动：自定义帧序列（重复/重排）必须被原样采用。
    #[test]
    fn custom_frames_are_used_verbatim() {
        let mut set = PlayerAnimSet::builtin();
        set.states[0].frames = vec![2, 1, 2, 0];
        let state = set.state(AnimRow::Idle);
        let frames = AnimationFrames::new(state.frames.clone(), state.looping);
        assert_eq!(frames.current(), 2);
        assert_eq!(frames.frames, vec![2, 1, 2, 0]);
    }

    #[test]
    fn empty_frames_degrade_to_a_single_cell() {
        let frames = AnimationFrames::new(Vec::new(), true);
        assert_eq!(frames.current(), 0);
        assert_eq!(next_cursor(0, frames.frames.len(), true), 0);
    }
}
