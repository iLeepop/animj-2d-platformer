//! 计时、终点与结算。所有者文档：`docs/01-game-design.md` §1.5–§1.7、
//! `docs/06-physics-and-movement.md` §6.5.5。
//!
//! 规则：
//! - 计时从进入 `Playing` 开始，暂停/加载不累计；每固定步 +1/60 秒。
//! - 任一玩家 AABB 与 `goal` 区域相交 ⇒ **该玩家**锁定自己的通关时间并继续（多人各自记时）。
//! - 所有在场玩家都到达终点 ⇒ 进入 `GameComplete` 结算。
//! - 分数 = `SCORE_BASE - 通关秒数 × SCORE_PER_SECOND`，最低 0（`01` §1.7）。

use bevy::prelude::*;

use crate::config::net::MAX_PLAYERS;
use crate::config::physics::FIXED_DT;
use crate::player::{PlayerId, PlayerMotion};
use crate::room::{RoomZone, RoomZoneKind, aabb_overlaps};
use crate::state::{GameSet, GameState};

/// 结算基础分（`01` §1.7）。
pub const SCORE_BASE: f32 = 10_000.0;
/// 每多花 1 秒扣掉的分数（`01` §1.7）。
pub const SCORE_PER_SECOND: f32 = 10.0;

/// 由通关秒数计算分数（纯函数）：四舍五入到整数，最低 0 分。
pub fn score_for_time(seconds: f32) -> u32 {
    (SCORE_BASE - seconds.max(0.0) * SCORE_PER_SECOND)
        .clamp(0.0, SCORE_BASE)
        .round() as u32
}

/// 本局计时与完赛状态。客户端由主机快照覆盖。
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct RunState {
    /// 本局已进行的秒数（固定步累计，暂停不计）。
    pub elapsed: f32,
    /// 每名玩家的通关时间（下标 = `PlayerId`）；`None` = 尚未到达终点。
    pub finished: [Option<f32>; MAX_PLAYERS],
    /// 已进入结算（防止重复触发）。
    pub complete: bool,
}

impl RunState {
    pub fn finish(&mut self, player: PlayerId, time: f32) -> bool {
        let index = player.index().min(MAX_PLAYERS - 1);
        if self.finished[index].is_some() {
            return false;
        }
        self.finished[index] = Some(time);
        true
    }

    pub fn time_of(&self, player: PlayerId) -> Option<f32> {
        self.finished.get(player.index()).copied().flatten()
    }

    pub fn score_of(&self, player: PlayerId) -> Option<u32> {
        self.time_of(player).map(score_for_time)
    }

    pub fn finished_count(&self) -> usize {
        self.finished.iter().filter(|time| time.is_some()).count()
    }
}

/// 某名玩家到达终点（HUD 提示 / 音效用）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayerFinished {
    pub player: PlayerId,
    pub time: f32,
}

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunState>()
            .add_message::<PlayerFinished>()
            .add_systems(
                FixedUpdate,
                (advance_run_clock, detect_goal_touch)
                    .chain()
                    .after(crate::player::apply_player_physics)
                    .in_set(GameSet::Physics)
                    .run_if(crate::net::not_client),
            )
            .add_systems(
                Update,
                finish_run_when_all_arrived
                    .in_set(GameSet::Room)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// 计时只在 `Playing` 的固定步推进；暂停（`Paused`）与加载不累计。
fn advance_run_clock(mut run: ResMut<RunState>) {
    run.elapsed += FIXED_DT;
}

/// 玩家 AABB 与终点区域相交 ⇒ 记录该玩家的通关时间（每人只记一次）。
fn detect_goal_touch(
    mut run: ResMut<RunState>,
    players: Query<(&PlayerId, &PlayerMotion)>,
    goals: Query<&RoomZone>,
    mut finished: MessageWriter<PlayerFinished>,
) {
    for (id, motion) in &players {
        if run.time_of(*id).is_some() {
            continue;
        }
        if !goals.iter().any(|zone| {
            zone.kind == RoomZoneKind::Goal && aabb_overlaps(&motion.0.aabb, &zone.aabb)
        }) {
            continue;
        }
        let time = run.elapsed;
        if run.finish(*id, time) {
            finished.write(PlayerFinished { player: *id, time });
            info!("玩家 {} 到达终点：{:.2} 秒", id.0, time);
        }
    }
}

/// 所有在场玩家都到达终点 ⇒ 结算。单人时等价于「碰到终点即获胜」。
fn finish_run_when_all_arrived(
    players: Query<&PlayerId, With<PlayerMotion>>,
    mut run: ResMut<RunState>,
    mut next: ResMut<NextState<GameState>>,
) {
    if run.complete {
        return;
    }
    let mut total = 0usize;
    let mut done = 0usize;
    for id in &players {
        total += 1;
        if run.time_of(*id).is_some() {
            done += 1;
        }
    }
    if total == 0 || done < total {
        return;
    }
    run.complete = true;
    info!(
        "全部玩家到达终点（{} 人，用时 {:.2} 秒），进入结算",
        total, run.elapsed
    );
    next.set(GameState::GameComplete);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_decreases_with_time_and_floors_at_zero() {
        assert_eq!(score_for_time(0.0), 10_000);
        assert_eq!(score_for_time(42.35), 9_577); // 10000 - 423.5 = 9576.5 ⇒ 四舍五入
        assert_eq!(score_for_time(1_000.0), 0);
        assert_eq!(score_for_time(9_999.0), 0);
        assert_eq!(score_for_time(-5.0), 10_000, "负时间按 0 处理");
    }

    #[test]
    fn each_player_locks_their_first_finish_time() {
        let mut run = RunState {
            elapsed: 12.5,
            ..RunState::default()
        };
        assert!(run.finish(PlayerId(0), 12.5));
        assert!(!run.finish(PlayerId(0), 99.0), "同一玩家只记第一次");
        run.elapsed = 20.0;
        assert!(run.finish(PlayerId(1), 20.0));
        assert_eq!(run.time_of(PlayerId(0)), Some(12.5));
        assert_eq!(run.time_of(PlayerId(1)), Some(20.0));
        assert_eq!(run.finished_count(), 2);
    }

    /// 单人：只有一名玩家到达 ⇒ 可以结算。多人：少一人就不行。
    #[test]
    fn completion_requires_every_player() {
        let mut run = RunState::default();
        run.finish(PlayerId(0), 10.0);
        assert_eq!(run.finished_count(), 1);
        run.finish(PlayerId(1), 11.0);
        assert_eq!(run.finished_count(), 2);
    }

    /// 触碰终点 → 锁定该玩家时间 → 全员到达 → 进入 `GameComplete`。
    #[test]
    fn touching_the_goal_finishes_the_run() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::state::app::StatesPlugin;

        use crate::physics::{Aabb, Body};
        use crate::room::{RoomZone, RoomZoneKind};

        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .add_message::<PlayerFinished>()
            .init_resource::<RunState>()
            .init_state::<GameState>();
        let center = Vec2::new(100.0, 100.0);
        app.world_mut()
            .spawn((PlayerId(0), PlayerMotion(Body::at_center(center))));
        app.world_mut().spawn(RoomZone {
            room_index: 0,
            kind: RoomZoneKind::Goal,
            aabb: Aabb::from_center_size(center, Vec2::splat(32.0)),
            at: (100, 100),
        });
        app.world_mut().resource_mut::<RunState>().elapsed = 12.34;

        app.world_mut()
            .run_system_once(detect_goal_touch)
            .expect("终点检测");
        assert_eq!(
            app.world().resource::<RunState>().time_of(PlayerId(0)),
            Some(12.34)
        );

        app.world_mut()
            .run_system_once(finish_run_when_all_arrived)
            .expect("结算检查");
        assert!(app.world().resource::<RunState>().complete);
        match app.world().resource::<NextState<GameState>>() {
            NextState::Pending(GameState::GameComplete) => {}
            other => panic!("全员到达应进入结算，实际 {other:?}"),
        }
    }
}
