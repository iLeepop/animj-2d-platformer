//! 状态机与调度集合。见 `docs/05-architecture.md` §5.4、§5.5。

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::config::FIXED_HZ;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameState {
    /// 启动：注册资产、读存档。立即转 `Loading`。
    #[default]
    Boot,
    /// 加载 `.level.ron`、校验、生成世界。失败则**停留在本状态**并显示错误。
    Loading,
    /// 正常游玩。
    Playing,
    /// 暂停菜单 / 装饰品装备 / 编辑器编辑态。
    Paused,
    /// 全部房间通关的结算。
    GameComplete,
}

/// 玩法系统集合。**所有玩法系统统一以 `in_state(GameState::Playing)` 门控**（`05` §5.5），
/// 因此编辑器只需把 `GameState` 停在 `Paused` 就能零侵入地停摆玩法。
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// `PreUpdate`：采样输入为 `PlayerInput`。
    Input,
    /// `FixedUpdate`：全部位移与碰撞。
    Physics,
    /// `Update`：把 `FixedUpdate` 的结果同步到 `Transform`、发一次性消息。
    PostPhysics,
    /// `Update`：当前房间判定、条件求值、通关与奖励。
    Room,
    /// `Update`：相机落点。
    Camera,
    /// `Update`：动画、粒子、音效触发。
    Fx,
    /// `PostUpdate`：像素吸附，**必须早于** `TransformSystems::Propagate`（`07` §7.6.3）。
    Snap,
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ))
            .configure_sets(
                PreUpdate,
                GameSet::Input.run_if(in_state(GameState::Playing)),
            )
            .configure_sets(
                FixedUpdate,
                GameSet::Physics.run_if(in_state(GameState::Playing)),
            )
            .configure_sets(
                Update,
                (
                    GameSet::PostPhysics,
                    GameSet::Room,
                    GameSet::Camera,
                    GameSet::Fx,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            // 吸附**不**门控状态：相机与既定 sprite 在任何状态下都应落在整像素上。
            // 必须在 `Propagate` 之前 —— 之后写 `Transform` 会延迟一帧（`07` §7.6.3）。
            .configure_sets(
                PostUpdate,
                GameSet::Snap.before(TransformSystems::Propagate),
            );
    }
}
