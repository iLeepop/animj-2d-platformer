//! UI 视觉与排版常量。**所有者文档：`docs/07-rendering.md` §7.10.5（菜单/组件）与
//! §7.10.4（HUD）**。
//!
//! 调色板只允许取自这里；UI 代码不得再写调色板之外的 `Color::srgb*` 值。

use bevy::prelude::Color;

// ---- 调色板（07 §7.10.5.1） ----

/// 屏幕底、深色面板。
pub const UI_INK: Color = Color::srgb_u8(0x1a, 0x1c, 0x2c);
/// 边框/分隔线；对 `UI_INK` 只有 1.54:1，仅装饰。
pub const UI_PLUM: Color = Color::srgb_u8(0x5d, 0x27, 0x5d);
/// 危险/退出；对 `UI_INK` 2.96:1，仅大字/图标。
pub const UI_CRIMSON: Color = Color::srgb_u8(0xb1, 0x3e, 0x53);
/// 警告、按下态。
pub const UI_ORANGE: Color = Color::srgb_u8(0xef, 0x7d, 0x57);
/// 主按钮、焦点环。
pub const UI_GOLD: Color = Color::srgb_u8(0xff, 0xcd, 0x75);
/// 成功/已存档。
pub const UI_GREEN: Color = Color::srgb_u8(0x38, 0xb7, 0x64);
/// 正文。
pub const UI_PAPER: Color = Color::srgb_u8(0xf4, 0xf4, 0xf4);

/// 次要文本（PAPER 60% alpha）。
pub const UI_MUTED: Color = Color::srgba_u8(0xf4, 0xf4, 0xf4, 0x99);
/// 次按钮/设置行底。
pub const UI_SURFACE: Color = Color::srgb_u8(0x24, 0x27, 0x3d);
/// 次按钮 hover。
pub const UI_SURFACE_HI: Color = Color::srgb_u8(0x2f, 0x33, 0x4d);
/// 暂停/设置/结算面板底（INK 95% alpha）。
pub const UI_PANEL_BG: Color = Color::srgba_u8(0x1a, 0x1c, 0x2c, 0xf2);
/// 暂停/结算遮罩（INK 72% alpha）。
pub const UI_SCRIM: Color = Color::srgba_u8(0x1a, 0x1c, 0x2c, 0xb8);
/// HUD 药丸底（INK 65% alpha）。
pub const UI_HUD_BG: Color = Color::srgba_u8(0x1a, 0x1c, 0x2c, 0xa6);
/// 2px 硬阴影。
pub const UI_SHADOW: Color = Color::srgba_u8(0x00, 0x00, 0x00, 0x73);

// ---- 字体（07 §7.10.5.2） ----

/// 菜单类 UI 的正文字号（逻辑像素）。见 `07` §7.10.5.2。
pub const MENU_FONT_PX: f32 = 12.0;
/// 菜单/暂停/设置的标题字号。见 `07` §7.10.5.2。
pub const MENU_TITLE_FONT_PX: f32 = 24.0;

// ---- 间距（07 §7.10.5.2） ----

/// 4px 基准间距。
pub const UI_SPACE_XS: f32 = 4.0;
/// 8px 基准间距。
pub const UI_SPACE_SM: f32 = 8.0;
/// 12px 基准间距。
pub const UI_SPACE_MD: f32 = 12.0;
/// 16px 基准间距。
pub const UI_SPACE_LG: f32 = 16.0;

/// 屏幕/面板内边距。见 `07` §7.10.5.2。
pub const MENU_PADDING_PX: f32 = 12.0;
/// 菜单行/按钮间距。见 `07` §7.10.5.2。
pub const MENU_GAP_PX: f32 = 8.0;

// ---- 组件（07 §7.10.5.2） ----

/// 主菜单/关卡选择按钮宽。
pub const UI_BUTTON_WIDTH: f32 = 200.0;
/// 菜单按钮高。
pub const UI_BUTTON_HEIGHT: f32 = 24.0;
/// 设置控件、装饰品 chip 高。
pub const UI_BUTTON_HEIGHT_SM: f32 = 20.0;
/// 按钮/面板边框宽。
pub const UI_BORDER_PX: f32 = 2.0;
/// 硬阴影偏移（blur 必须为 0）。
pub const UI_SHADOW_PX: f32 = 2.0;
/// 焦点环宽度（offset = 1）。
pub const UI_FOCUS_OUTLINE_PX: f32 = 1.0;

/// 面板内边距。
pub const UI_PANEL_PADDING: f32 = 12.0;
/// 暂停面板最大高度。
pub const UI_PANEL_MAX_HEIGHT: f32 = 288.0;
/// 暂停面板宽。
pub const UI_PAUSE_PANEL_WIDTH: f32 = 260.0;
/// 暂停按钮宽（260 − 2×2 边框 − 2×12 内边距）。
pub const UI_PAUSE_BUTTON_WIDTH: f32 = 232.0;
/// 设置面板宽。
pub const UI_SETTINGS_PANEL_WIDTH: f32 = 240.0;
/// 设置行宽（240 − 2×2 − 2×12）。
pub const UI_SETTINGS_CONTENT_WIDTH: f32 = 212.0;
/// 设置页 toggle 宽（全屏/语言；固定宽度避免文本切换时抖动）。
pub const UI_SETTINGS_TOGGLE_WIDTH: f32 = 80.0;
/// 设置页音量数值宽。
pub const UI_SETTINGS_VALUE_WIDTH: f32 = 32.0;
/// 关卡卡片宽（两列 + 8 间距 = 264）。
pub const UI_LEVEL_CARD_WIDTH: f32 = 128.0;
/// 关卡卡片高。
pub const UI_LEVEL_CARD_HEIGHT: f32 = 56.0;
/// 联机房间行宽。
pub const UI_ONLINE_ROOM_WIDTH: f32 = 264.0;

// ---- HUD（07 §7.10.4） ----

/// HUD 字号（逻辑像素，坐标系是 `VIEWPORT_W` 宽的视口）。
pub const HUD_FONT_PX: f32 = 12.0;
/// HUD 距视口左/上边距（逻辑像素）。
pub const HUD_MARGIN_PX: f32 = 8.0;
/// HUD 药丸最大宽度；超出换行。
pub const UI_HUD_PILL_MAX_WIDTH: f32 = 240.0;
/// 提示条距视口底部。
pub const UI_TOAST_BOTTOM: f32 = 24.0;
/// 结算面板宽。
pub const UI_COMPLETION_WIDTH: f32 = 240.0;
/// 结算面板高。
pub const UI_COMPLETION_HEIGHT: f32 = 120.0;
