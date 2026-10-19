//! 条件求值（**纯函数**）。所有者文档：`docs/01-game-design.md` §1.4。
//!
//! 条件之间是逻辑与（AND）；空列表不算通关（「至少一个目标」是设计约束，代码只保证空列表永不满足）。
//! 求值只针对**当前房间**（`01` §1.4）：离屏房间不判定，避免误触发与无谓开销。

use level_format::Goal;

/// 一次求值所需的房间运行时数据。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoomRuntime {
    /// 本固定步玩家是否处于该房间的 `exit` 区域内（`06` §6.5.5）。
    pub at_exit: bool,
    /// 该房间已收集的果实数。
    pub collected_fruit: u32,
    /// 该房间的果实总数（用于 `CollectAll`）。
    pub total_fruit: u32,
}

/// 单个条件是否满足。
pub fn condition_met(goal: &Goal, runtime: &RoomRuntime) -> bool {
    match goal {
        Goal::ReachExit => runtime.at_exit,
        Goal::Collect { count } => runtime.collected_fruit >= *count,
        // 房间没有果实时不成立（校验也要求 `CollectAll` 的房间至少 1 个果实）
        Goal::CollectAll => {
            runtime.total_fruit > 0 && runtime.collected_fruit >= runtime.total_fruit
        }
    }
}

/// 全部条件是否满足（AND）。空列表返回 `false`（永不自动通关；是否补齐目标由关卡设计决定）。
pub fn goals_satisfied(goals: &[Goal], runtime: &RoomRuntime) -> bool {
    !goals.is_empty() && goals.iter().all(|goal| condition_met(goal, runtime))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(at_exit: bool, collected: u32, total: u32) -> RoomRuntime {
        RoomRuntime {
            at_exit,
            collected_fruit: collected,
            total_fruit: total,
        }
    }

    #[test]
    fn empty_goals_never_complete() {
        // 设计上要求每房至少一个目标；代码不强制，因此空列表必须保持"永不通关"而不是真空真
        assert!(!goals_satisfied(&[], &runtime(true, 5, 5)));
    }

    #[test]
    fn reach_exit_follows_the_zone() {
        assert!(condition_met(&Goal::ReachExit, &runtime(true, 0, 0)));
        assert!(!condition_met(&Goal::ReachExit, &runtime(false, 0, 0)));
    }

    #[test]
    fn collect_is_at_least_n() {
        let goal = Goal::Collect { count: 2 };
        assert!(!condition_met(&goal, &runtime(true, 1, 5)));
        assert!(condition_met(&goal, &runtime(true, 2, 5)));
        assert!(condition_met(&goal, &runtime(true, 5, 5)));
    }

    #[test]
    fn collect_all_needs_every_fruit_and_at_least_one() {
        assert!(condition_met(&Goal::CollectAll, &runtime(true, 3, 3)));
        assert!(!condition_met(&Goal::CollectAll, &runtime(true, 2, 3)));
        // 0 个果实的房间不成立
        assert!(!condition_met(&Goal::CollectAll, &runtime(true, 0, 0)));
    }

    #[test]
    fn goals_are_and_combined() {
        let goals = [Goal::ReachExit, Goal::Collect { count: 2 }];
        assert!(
            !goals_satisfied(&goals, &runtime(false, 2, 3)),
            "不在出口不算"
        );
        assert!(
            !goals_satisfied(&goals, &runtime(true, 1, 3)),
            "果实不够不算"
        );
        assert!(goals_satisfied(&goals, &runtime(true, 2, 3)));
    }

    #[test]
    fn empty_goal_list_never_clears() {
        assert!(!goals_satisfied(&[], &runtime(true, 9, 9)));
    }
}
