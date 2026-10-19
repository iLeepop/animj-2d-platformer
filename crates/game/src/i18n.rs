//! 界面文案与语言。所有者文档：`docs/05-architecture.md` §5.14。
//!
//! 语言默认**中文**；英文是第二选项。所有玩家可见的 UI 文本都从这里取，禁止在 UI 系统里
//! 硬编码某一种语言（HUD 的数值/关卡名/资源路径等数据除外）。
//!
//! 为什么文案是 `&'static str`：语言只有两种，编译期常量表最省事，也不会在运行期分配；
//! 格式化串由调用方 `format!` 组合。

use serde::{Deserialize, Serialize};

use crate::state::GameState;

/// 游戏语言。默认中文（产品要求）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Zh,
    En,
}

impl Language {
    pub fn strings(self) -> &'static Strings {
        match self {
            Language::Zh => &ZH,
            Language::En => &EN,
        }
    }

    /// 设置界面里显示的语言名（各语言用自己的写法）。
    pub fn self_name(self) -> &'static str {
        self.strings().self_name
    }

    pub fn toggle(self) -> Self {
        match self {
            Language::Zh => Language::En,
            Language::En => Language::Zh,
        }
    }

    /// HUD 顶部的状态名。只用于人读；日志仍用 `GameState` 的 `Debug`。
    pub fn state_name(self, state: GameState) -> &'static str {
        match (self, state) {
            (Language::Zh, GameState::Boot) => "启动",
            (Language::Zh, GameState::MainMenu) => "主菜单",
            (Language::Zh, GameState::LevelSelect) => "选择关卡",
            (Language::Zh, GameState::OnlineMenu) => "联机",
            (Language::Zh, GameState::Settings) => "设置",
            (Language::Zh, GameState::Loading) => "加载中",
            (Language::Zh, GameState::Playing) => "游戏中",
            (Language::Zh, GameState::Paused) => "已暂停",
            (Language::Zh, GameState::GameComplete) => "全部通关",
            (Language::En, GameState::Boot) => "Boot",
            (Language::En, GameState::MainMenu) => "MainMenu",
            (Language::En, GameState::LevelSelect) => "LevelSelect",
            (Language::En, GameState::OnlineMenu) => "OnlineMenu",
            (Language::En, GameState::Settings) => "Settings",
            (Language::En, GameState::Loading) => "Loading",
            (Language::En, GameState::Playing) => "Playing",
            (Language::En, GameState::Paused) => "Paused",
            (Language::En, GameState::GameComplete) => "GameComplete",
        }
    }
}

/// 一种语言的全部 UI 文案。字段按界面分组，新增文案时两种语言都必须补。
pub struct Strings {
    // 主菜单 / 关卡选择
    pub start_game: &'static str,
    pub load_save: &'static str,
    pub settings: &'static str,
    pub exit: &'static str,
    pub select_level: &'static str,
    pub no_levels: &'static str,
    pub saved: &'static str,
    pub back: &'static str,
    // 联机
    pub local_coop: &'static str,
    pub online: &'static str,
    pub online_title: &'static str,
    pub create_room: &'static str,
    pub searching: &'static str,
    pub no_rooms: &'static str,
    pub room_full: &'static str,
    pub net_rejected: &'static str,
    pub net_disconnected: &'static str,
    pub net_timeout: &'static str,
    pub net_level_mismatch: &'static str,
    // 设置
    pub volume: &'static str,
    pub fullscreen: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub language: &'static str,
    pub self_name: &'static str,
    // 暂停
    pub resume: &'static str,
    pub save: &'static str,
    pub main_menu: &'static str,
    pub quit_save: &'static str,
    // 读档提示
    pub no_save: &'static str,
    pub save_version_mismatch: &'static str,
    pub save_level_not_found: &'static str,
    pub save_load_failed: &'static str,
    // HUD / 结算
    pub room: &'static str,
    pub cleared_count: &'static str,
    pub decorations: &'static str,
    pub goals: &'static str,
    pub exit_goal: &'static str,
    pub fruit: &'static str,
    pub all_fruits: &'static str,
    pub cleared: &'static str,
    pub room_cleared: &'static str,
    pub decoration_gained: &'static str,
    pub all_rooms_cleared: &'static str,
    pub assets: &'static str,
}

pub static ZH: Strings = Strings {
    start_game: "开始游戏",
    load_save: "读取存档",
    settings: "设置",
    exit: "退出游戏",
    select_level: "选择关卡",
    no_levels: "未找到关卡",
    saved: "（有存档）",
    back: "返回",
    local_coop: "本地双人",
    online: "联机游戏",
    online_title: "局域网房间",
    create_room: "创建房间",
    searching: "正在搜索局域网房间…",
    no_rooms: "未发现房间",
    room_full: "已满",
    net_rejected: "主机拒绝了加入请求",
    net_disconnected: "与主机的连接已断开",
    net_timeout: "连接主机超时",
    net_level_mismatch: "主机关卡与本地不一致",
    volume: "音量",
    fullscreen: "全屏",
    on: "开",
    off: "关",
    language: "语言",
    self_name: "中文",
    resume: "继续",
    save: "保存",
    main_menu: "主菜单",
    quit_save: "退出并保存",
    no_save: "没有存档",
    save_version_mismatch: "存档版本不匹配",
    save_level_not_found: "存档关卡不存在",
    save_load_failed: "读档失败",
    room: "房间",
    cleared_count: "通关",
    decorations: "装饰",
    goals: "目标",
    exit_goal: "出口",
    fruit: "果实",
    all_fruits: "全部果实",
    cleared: "已通关",
    room_cleared: "房间通关",
    decoration_gained: "获得装饰品",
    all_rooms_cleared: "全部房间通关",
    assets: "资产",
};

pub static EN: Strings = Strings {
    start_game: "START GAME",
    load_save: "LOAD SAVE",
    settings: "SETTINGS",
    exit: "EXIT",
    select_level: "SELECT LEVEL",
    no_levels: "NO LEVELS FOUND",
    saved: "(SAVED)",
    back: "BACK",
    local_coop: "LOCAL CO-OP",
    online: "ONLINE",
    online_title: "LAN ROOMS",
    create_room: "CREATE ROOM",
    searching: "SEARCHING LAN ROOMS…",
    no_rooms: "NO ROOMS FOUND",
    room_full: "FULL",
    net_rejected: "HOST REJECTED THE JOIN REQUEST",
    net_disconnected: "DISCONNECTED FROM HOST",
    net_timeout: "HOST CONNECTION TIMED OUT",
    net_level_mismatch: "HOST LEVEL MISMATCH",
    volume: "VOLUME",
    fullscreen: "FULLSCREEN",
    on: "ON",
    off: "OFF",
    language: "LANGUAGE",
    self_name: "ENGLISH",
    resume: "RESUME",
    save: "SAVE",
    main_menu: "MAIN MENU",
    quit_save: "QUIT & SAVE",
    no_save: "NO SAVE DATA",
    save_version_mismatch: "SAVE VERSION MISMATCH",
    save_level_not_found: "SAVE LEVEL NOT FOUND",
    save_load_failed: "SAVE LOAD FAILED",
    room: "room",
    cleared_count: "clear",
    decorations: "decor",
    goals: "goals",
    exit_goal: "EXIT",
    fruit: "FRUIT",
    all_fruits: "ALL FRUITS",
    cleared: "CLEARED",
    room_cleared: "ROOM CLEARED",
    decoration_gained: "DECORATION",
    all_rooms_cleared: "ALL ROOMS CLEARED",
    assets: "assets",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_chinese() {
        assert_eq!(Language::default(), Language::Zh);
        assert_eq!(Language::default().strings().start_game, "开始游戏");
    }

    #[test]
    fn toggle_cycles_between_the_two_languages() {
        assert_eq!(Language::Zh.toggle(), Language::En);
        assert_eq!(Language::En.toggle(), Language::Zh);
    }

    #[test]
    fn every_state_has_a_localized_name() {
        for state in [
            GameState::Boot,
            GameState::MainMenu,
            GameState::LevelSelect,
            GameState::OnlineMenu,
            GameState::Settings,
            GameState::Loading,
            GameState::Playing,
            GameState::Paused,
            GameState::GameComplete,
        ] {
            assert!(!Language::Zh.state_name(state).is_empty());
            assert!(!Language::En.state_name(state).is_empty());
        }
    }
}
