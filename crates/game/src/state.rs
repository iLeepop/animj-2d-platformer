//! 状态机与调度集合。见 `docs/05-architecture.md` §5.4、§5.5。

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::config::FIXED_HZ;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameState {
    /// 启动：按 [`StartupFlow`] 决定进主菜单还是直接加载关卡。
    #[default]
    Boot,
    /// 主菜单（开始游戏 / 读取存档 / 设置 / 退出）。仅发布 bin 注册菜单插件。
    MainMenu,
    /// 关卡选择。列出 `assets/levels/*.level.ron`。
    LevelSelect,
    /// 局域网联机：搜索房间 / 创建房间（`05` §5.15）。
    OnlineMenu,
    /// 全局设置（音量 / 全屏）。可从主菜单或暂停菜单进入。
    Settings,
    /// 加载 `.level.ron`、校验、生成世界。失败则**停留在本状态**并显示错误。
    Loading,
    /// 正常游玩。
    Playing,
    /// 暂停菜单 / 装饰品装备 / 编辑器编辑态。
    Paused,
    /// 全部房间通关的结算。
    GameComplete,
}

/// 启动流程。默认 [`StartupFlow::Menu`]；编辑器在 bin 里改成 [`StartupFlow::Direct`]（`05` §5.5）。
///
/// 这不是"game 知道编辑器"：它只是一个部署级开关——带菜单的发布进程与直入关卡的宿主进程
/// 走不同的入口。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupFlow {
    /// 进主菜单，由用户选择关卡/读档。
    #[default]
    Menu,
    /// 直接加载默认关卡（编辑器、冒烟验证）。
    Direct,
}

/// 本局会话模式（`05` §5.15）。菜单在进入 `Loading` 前写入；网络插件据此决定
/// 谁跑物理、谁接收快照。编辑器与单人游戏恒为 [`SessionMode::Solo`]。
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub enum SessionMode {
    /// 单人离线。
    #[default]
    Solo,
    /// 同屏双人（共享键盘，`06` §6.1.6）。
    LocalCoop,
    /// 主机：权威模拟全部玩家，并向客户端广播快照。
    Host,
    /// 客户端：只发送输入、应用主机快照。
    Client { server: std::net::SocketAddr },
}

impl SessionMode {
    /// 本进程需要生成的本地玩家数。客户端一个都不生成：玩家实体由主机快照创建。
    pub fn local_players(&self) -> usize {
        match self {
            SessionMode::Solo | SessionMode::Host => 1,
            SessionMode::LocalCoop => 2,
            SessionMode::Client { .. } => 0,
        }
    }

    pub fn is_client(&self) -> bool {
        matches!(self, SessionMode::Client { .. })
    }

    pub fn is_host(&self) -> bool {
        matches!(self, SessionMode::Host)
    }
}

/// 从设置界面返回的目标状态（主菜单或暂停菜单）。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsReturn(pub GameState);

impl Default for SettingsReturn {
    fn default() -> Self {
        Self(GameState::MainMenu)
    }
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
    /// `Update`：相机跟随玩家。见 `02` §2.2。
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
            .init_resource::<StartupFlow>()
            .init_resource::<SettingsReturn>()
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
