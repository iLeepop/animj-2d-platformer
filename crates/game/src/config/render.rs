//! 渲染常量。**所有者文档：`docs/07-rendering.md` §7.1**（Z 数值归 `docs/00-conventions.md` §0.4）。

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use level_format::TILE_PX;

/// 内层视野覆盖的瓦片数（10×10 = 320×320 像素）。见 `07` §7.1.1。
///
/// 必须小于 `ROOM_TILES`（一个房间 16×16 瓦片）：玩家的视野小于房间，看不到太多地图。
pub const VIEWPORT_TILES: u32 = 10;

/// 内层离屏渲染目标宽（像素）= 10 瓦片。见 `07` §7.1.1。
pub const VIEWPORT_W: u32 = VIEWPORT_TILES * TILE_PX as u32;
/// 内层离屏渲染目标高。同上。
pub const VIEWPORT_H: u32 = VIEWPORT_TILES * TILE_PX as u32;

/// 窗口初始宽（物理像素）= 2 倍视口 ⇒ 启动即 `k = 2`、四周无黑边。见 `07` §7.1.1。
pub const WINDOW_W: u32 = 2 * VIEWPORT_W;
/// 窗口初始高。同上。
pub const WINDOW_H: u32 = 2 * VIEWPORT_H;

/// 本地双人进入游戏时的窗口宽：两个视图并排、每个视图 2× 视口。见 `07` §7.1.1。
pub const WINDOW_W_SPLIT: u32 = 2 * WINDOW_W;
/// 本地双人窗口高（与单视图相同）。同上。
pub const WINDOW_H_SPLIT: u32 = WINDOW_H;

/// 整数放大倍数 `k` 的下限。见 `07` §7.1.1。
pub const MIN_CANVAS_SCALE: u32 = 1;

/// 窗口标题。见 `07` §7.1.1。
pub const WINDOW_TITLE: &str = "animj";

/// 内层相机 `order`（先渲染，填满离屏纹理）。见 `07` §7.1.2。
pub const INNER_CAMERA_ORDER: isize = -1;
/// 外层相机 `order`（默认值，不得更改）。见 `07` §7.1.2。
pub const OUTER_CAMERA_ORDER: isize = 0;
/// UI 相机 `order`：必须在 canvas 之后渲染（UI 覆盖在画面上，且目标都是窗口）。见 `07` §7.1.2。
pub const UI_CAMERA_ORDER: isize = OUTER_CAMERA_ORDER + 1;

/// 离屏渲染目标格式。必须是 sRGB 变体，否则整条管线颜色被系统性压暗（`07` §7.1.5）。
pub const CANVAS_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

/// 瓦片图集的列数（行数算法见 `07` §7.8.2 / `04` §10）。
pub const TILESET_COLUMNS: u32 = 8;

/// 瓦片图集资产路径（相对 `assets/`）。路径与命名规格见 `04` §2/§10。
pub const TILESET_IMAGE: &str = "sprites/tiles/tileset.png";

/// 角色图集资产路径。规格见 `04` §2。
pub const PLAYER_SHEET_IMAGE: &str = "sprites/player/cat_sheet.png";
/// 角色图集的列数（`04` §3.2/§5.2：6 列 × 4 行）。
pub const PLAYER_SHEET_COLUMNS: u32 = 6;
/// 角色图集的行数。
pub const PLAYER_SHEET_ROWS: u32 = 4;

/// 相机自身的 `Z`。
///
/// **必须为 0.0**：`OrthographicProjection::default_2d()` 的深度范围是 `near = -1000 / far = 1000`，
/// 而 `00` §0.4 的 Z 表一直排到 `-100`（远景视差）。相机放在 `999.9` 会把 `-100` 推到近平面之外而整体不可见。
pub const CAMERA_Z: f32 = 0.0;

/// Z 层常量名。**数值一律来自 `docs/00-conventions.md` §0.4**，本文件不另行决定。
/// HUD 提示条（通关/获奖）的显示时长（秒）。见 `07` §7.10.4。
pub const HUD_TOAST_SECS: f32 = 2.5;

/// 装饰品槽位数（`04` §6.1）：`layer_index ∈ 0..DECORATION_LAYERS`。
pub const DECORATION_LAYERS: usize = 7;

pub mod z {
    /// 远景视差。见 `00` §0.4。
    pub const PARALLAX: f32 = -100.0;
    /// 房间背景板。见 `00` §0.4。
    pub const ROOM_BG: f32 = -50.0;
    /// 瓦片贴花（草、砖纹）。见 `00` §0.4。
    pub const TILE_DECAL: f32 = -10.0;
    /// 实体瓦片。见 `00` §0.4。
    pub const TILE: f32 = 0.0;
    /// 可拾取物。见 `00` §0.4。
    pub const PICKUP: f32 = 20.0;
    /// 存档点标记。见 `00` §0.4。
    pub const CHECKPOINT: f32 = 21.0;
    /// 敌人。见 `00` §0.4。
    pub const ENEMY: f32 = 30.0;
    /// 角色本体基准。见 `00` §0.4。
    pub const PLAYER: f32 = 40.0;

    /// 第 `player` 个角色的 Z：`PLAYER + 0.1 × player`。
    ///
    /// 玩家之间会互相碰撞（同屏重叠），必须用不同 Z 保证绘制顺序稳定（`00` §0.4）；
    /// 上限 `MAX_PLAYERS = 4` ⇒ 最大 40.3，仍低于装饰品的 41.0。
    pub fn player_z(player: u8) -> f32 {
        PLAYER + player as f32 * 0.1
    }
    /// 装饰品图层基准：实际 Z = `DECORATION_BASE + layer_index`（槽位见 `04` §6.1）。
    pub const DECORATION_BASE: f32 = 41.0;
    /// 粒子 / 特效。见 `00` §0.4。
    pub const PARTICLE: f32 = 60.0;
    /// 前景瓦片。见 `00` §0.4。
    pub const TILE_FOREGROUND: f32 = 70.0;
    /// 世界空间文字。见 `00` §0.4。
    pub const TEXT2D: f32 = 80.0;

    /// 外层视图里那张离屏纹理 sprite 的 Z。**本文定义**，不属于 `00` §0.4 的游戏 Z 表：
    /// 外层视图里只有它一个 sprite，因此不会与任何游戏 sprite 冲突（`07` §7.1.3）。
    pub const CANVAS: f32 = 0.0;
}

/// 多玩家颜色（`04` §3.2）：只用于 `Sprite::color` 着色，不新增贴图。
///
/// 顺序即玩家编号；主机/单人是白色（保持原视觉）。
pub fn player_tint(player: u8) -> Color {
    match player {
        0 => Color::WHITE,
        1 => Color::srgb(0.55, 0.90, 1.00),
        2 => Color::srgb(1.00, 0.85, 0.45),
        _ => Color::srgb(0.70, 1.00, 0.60),
    }
}

/// 房间背景板颜色（`04` §7.4 占位调色板的 `bg`）。
///
/// 用**非黑**色是刻意的：它让「房间内容」与「窗口黑边」在截图里可区分，从而能客观验证整数倍放大与黑边。
pub fn room_bg_color() -> Color {
    Color::srgb_u8(0x1a, 0x1c, 0x2c)
}
