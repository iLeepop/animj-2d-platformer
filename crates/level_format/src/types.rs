//! 关卡数据结构。见 `docs/03-level-format.md` §3.2
//!
//! 所有结构都加了 `deny_unknown_fields`：字段名是契约，拼错字段名必须解析失败，
//! 不能被静默忽略（那正是 `docs/03-level-format.md` §3.8 禁止的宽容解析）。

use serde::{Deserialize, Serialize};

use crate::legend::TileKind;

/// 本引擎支持的关卡文件版本号。见 `docs/03-level-format.md` §3.8
pub const VERSION: u32 = 2;
/// 瓦片边长（像素）。见 `docs/00-conventions.md` §0.3
pub const TILE_PX: i32 = 32;
/// 房间边长（瓦片）。见 `docs/00-conventions.md` §0.3
pub const ROOM_TILES: usize = 16;
/// 房间边长（像素）= `TILE_PX * ROOM_TILES`。见 `docs/00-conventions.md` §0.3
pub const ROOM_PX: i32 = TILE_PX * ROOM_TILES as i32;
/// 地块的**碰撞行为**（v2）：可扩展的只有外观与名称，物理语义恒为这 4 种。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Behavior {
    Solid,
    OneWay,
    Hazard,
    None,
}

impl Behavior {
    /// 内置图集里的默认贴图格（`04` §10）。
    pub fn default_cell(self) -> u32 {
        match self {
            Behavior::Solid => 0,
            Behavior::OneWay => 1,
            Behavior::Hazard => 2,
            Behavior::None => 3,
        }
    }
}

/// 调色板条目：字符 → 名称 + 行为 + 贴图格（32×32、行主序、`tileset_columns` 列）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaletteEntry {
    pub char: char,
    pub name: String,
    pub behavior: Behavior,
    pub cell: u32,
}

/// 贴图切分列数的默认值（`03` §3.9）。
pub fn default_tileset_columns() -> u32 {
    8
}

/// 背景视差层数上限（`03` §3.2 / `07` §7.13）。
pub const MAX_BACKGROUND_LAYERS: usize = 3;

/// 关卡背景：按列表顺序从远到近绘制（`03` §3.2、`07` §7.13）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Background {
    pub layers: Vec<BackgroundLayer>,
}

/// 一层视差背景。贴图按自身像素尺寸平铺整个视口。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackgroundLayer {
    /// 相对 `assets/` 的 PNG 路径（加载时规范化）。
    pub image: String,
    /// 水平滚动百分比：`0` = 屏幕固定，`100` = 与世界同速。范围 `0..=100`（V20）。
    pub scroll_x: u8,
    /// 垂直滚动百分比：`0` = 屏幕固定，`100` = 与世界同速。范围 `0..=100`（V20）。
    pub scroll_y: u8,
}


/// 一个完整关卡 = `grid.cols × grid.rows` 个房间。见 `docs/03-level-format.md` §3.2
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Level {
    /// 格式版本，必须 == `VERSION`；作为首个字段，缺省即解析失败。
    pub version: u32,
    /// snake_case 关卡 id，必须等于文件名主干，见 `name_matches_file`。
    pub name: String,
    /// 房间网格尺寸。
    pub grid: Grid,
    /// 起始房间。
    pub start: RoomCoord,
    /// v2：自定义贴图（相对 `assets/`）。`None` ⇒ 内置 `assets/sprites/tiles/tileset.png`。
    #[serde(default)]
    pub tileset: Option<String>,
    /// v2：贴图按 `32×32` 切分的列数。
    #[serde(default = "default_tileset_columns")]
    pub tileset_columns: u32,
    /// v2：字符 → 名称/行为/贴图格。空 ⇒ 内置 4 种（等价 v1）。
    #[serde(default)]
    pub palette: Vec<PaletteEntry>,
    /// 背景视差层（`07` §7.13）。缺省 / 空 ⇒ 无背景（只显示房间背景板）。
    #[serde(default)]
    pub background: Option<Background>,
    /// 行主序排列的房间，`len() == grid.cols * grid.rows`，`row 0` 是地图最上一行。
    pub rooms: Vec<Room>,
}

/// 由贴图宽度推导 `tileset_columns`（每格 `TILE_PX` 宽）。
pub fn columns_from_sheet_width(width: u32) -> u32 {
    (width / (TILE_PX.max(1) as u32)).max(1)
}

/// 规范化关卡里记录的资产路径（`03` §…：**相对 `assets/`**）。
///
/// 接受两种写法并统一成相对形式：`assets/sprites/x.png` 与 `sprites/x.png`
/// 都返回 `Some("sprites/x.png")`；反斜杠统一成 `/`；空串返回 `None`。
/// 实机 bug：手填 `assets/sprites/tiles/tileset1.png` 会被 Bevy 拼接成
/// `assets/assets/sprites/...` 导致 `Path not found`。
pub fn normalize_asset_path(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_start_matches("./");
    let stripped = path
        .strip_prefix("assets/")
        .unwrap_or(path);
    let stripped = stripped.trim_start_matches('/');
    if stripped.is_empty() { None } else { Some(stripped.to_string()) }
}

impl Level {
    /// 内置调色板（等价 v1 的固定 legend）。
    pub fn builtin_palette() -> Vec<PaletteEntry> {
        vec![
            PaletteEntry { char: '#', name: "solid".into(), behavior: Behavior::Solid, cell: 0 },
            PaletteEntry { char: '=', name: "oneway".into(), behavior: Behavior::OneWay, cell: 1 },
            PaletteEntry { char: '^', name: "spike".into(), behavior: Behavior::Hazard, cell: 2 },
            PaletteEntry { char: '*', name: "decor".into(), behavior: Behavior::None, cell: 3 },
        ]
    }

    /// 生效调色板（空 ⇒ 内置）。
    pub fn palette(&self) -> Vec<PaletteEntry> {
        if self.palette.is_empty() { Self::builtin_palette() } else { self.palette.clone() }
    }

    pub fn entry_of(&self, ch: char) -> Option<PaletteEntry> {
        self.palette().into_iter().find(|entry| entry.char == ch)
    }

    pub fn behavior_of(&self, ch: char) -> Option<Behavior> {
        self.entry_of(ch).map(|entry| entry.behavior)
    }

    pub fn cell_of(&self, ch: char) -> Option<u32> {
        self.entry_of(ch).map(|entry| entry.cell)
    }
}

/// 关卡的房间网格尺寸。见 `docs/03-level-format.md` §3.2
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    /// 网格列数；`col ∈ [0, cols)`。
    pub cols: u32,
    /// 网格行数；`row ∈ [0, rows)`，`row 0` 是地图最上一行。
    pub rows: u32,
}

/// 房间网格坐标。见 `docs/03-level-format.md` §3.2
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoomCoord {
    /// 列，0 起。
    pub col: u32,
    /// 行，0 起，自上而下。
    pub row: u32,
}

/// 一个房间 = 16×16 瓦片。见 `docs/03-level-format.md` §3.2
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Room {
    /// 可选房间名，便于编辑器与 diff 阅读；全关卡唯一（V16）。
    #[serde(default)]
    pub name: Option<String>,
    /// 恰好 16 行、每行恰好 16 个 legend 字符；第 0 行 = 房间最上一行。
    pub tiles: Vec<String>,
    /// 房间目标条件。允许为空（设计上要求至少一个，但**不在代码里强制**）；空列表运行时永不满足。
    pub goals: Vec<Goal>,
    /// 装饰品 id；`None` = 本房间不授予装饰品。
    #[serde(default)]
    pub reward: Option<String>,
    /// 重生点（房间局部坐标，AABB 中心）；`None` = 用 `docs/02-world-and-rooms.md` 的默认值。
    #[serde(default)]
    pub spawn: Option<(i32, i32)>,
    /// 房间内的实体列表；作为契约字段，缺省即解析失败（不是「可选」）。
    pub entities: Vec<EntitySpec>,
}

/// 关卡文件中的一个实体。见 `docs/03-level-format.md` §3.2 / §3.4
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntitySpec {
    /// 实体类型，必须在 `docs/03-level-format.md` §3.4 的 v1 注册表中。
    pub kind: String,
    /// 房间局部坐标（`(0, 0)` = 房间左上角，+Y 向下）上的 AABB 中心，单位像素。
    pub at: (i32, i32),
    /// AABB 尺寸，单位像素；`None` = 用注册表默认值，见 `size_or_default`。
    #[serde(default)]
    pub size: Option<(i32, i32)>,
    /// 可选稳定标识，供编辑器与后续逻辑引用。
    #[serde(default)]
    pub id: Option<String>,
}

/// 房间目标条件；条件之间是 AND。见 `docs/03-level-format.md` §3.5
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Goal {
    /// 玩家 AABB 与房间内的 exit 区域相交（该房间需恰好 1 个 exit，见 V8）。
    ReachExit,
    /// 该房间所有 fruit 已收集（该房间需至少 1 个 fruit，见 V9）。
    CollectAll,
    /// 该房间已收集 fruit >= `count`（`1 <= count <=` 房间内 fruit 数，见 V10）。
    Collect {
        /// 需要的果实数量。
        count: u32,
    },
}

/// `(col, row)` 在行主序 `rooms` 数组中的下标 = `row * cols + col`。见 `docs/00-conventions.md` §0.3
pub fn room_index(col: u32, row: u32, cols: u32) -> usize {
    row as usize * cols as usize + col as usize
}

impl Level {
    /// 按网格坐标取房间；坐标越界或 `rooms` 长度与网格不符时返回 `None`。
    pub fn room(&self, col: u32, row: u32) -> Option<&Room> {
        if col >= self.grid.cols || row >= self.grid.rows {
            return None;
        }
        self.rooms.get(room_index(col, row, self.grid.cols))
    }
}

impl Room {
    /// 按**房间局部瓦片坐标**取瓦片：`(0, 0)` 是房间左下角，+X 向右、+Y 向上；
    /// 越界返回 `Air`。文件里的 `tiles[0]` 是房间最上一行，即 `ty == ROOM_TILES - 1`。
    pub fn tile_at(&self, tx: usize, ty: usize) -> TileKind {
        if tx >= ROOM_TILES || ty >= ROOM_TILES {
            return TileKind::Air;
        }
        let Some(line) = self.tiles.get(ROOM_TILES - 1 - ty) else {
            return TileKind::Air;
        };
        let Some(c) = line.chars().nth(tx) else {
            return TileKind::Air;
        };
        // 非法字符由校验期 V6 拦截；此处兜底为 Air，避免运行时因坏数据 panic
        TileKind::from_char(c).unwrap_or(TileKind::Air)
    }
}

impl EntitySpec {
    /// 实体 AABB 尺寸；未显式给出时取注册表默认值 `(TILE_PX, TILE_PX)`。见 `docs/03-level-format.md` §3.4
    pub fn size_or_default(&self) -> (i32, i32) {
        self.size.unwrap_or((TILE_PX, TILE_PX))
    }
}

#[cfg(test)]
mod path_tests {
    use super::normalize_asset_path;

    #[test]
    fn normalize_asset_path_accepts_both_forms() {
        assert_eq!(
            normalize_asset_path("assets/sprites/tiles/tileset1.png").as_deref(),
            Some("sprites/tiles/tileset1.png")
        );
        assert_eq!(
            normalize_asset_path("sprites/tiles/tileset1.png").as_deref(),
            Some("sprites/tiles/tileset1.png")
        );
        assert_eq!(
            normalize_asset_path("assets\\sprites\\tiles\\x.png").as_deref(),
            Some("sprites/tiles/x.png")
        );
        assert_eq!(
            normalize_asset_path("  ./assets/a.png  ").as_deref(),
            Some("a.png")
        );
        assert_eq!(normalize_asset_path(""), None);
        assert_eq!(normalize_asset_path("assets/"), None);
    }

    #[test]
    fn columns_follow_sheet_width() {
        assert_eq!(super::columns_from_sheet_width(192), 6);
        assert_eq!(super::columns_from_sheet_width(256), 8);
        assert_eq!(super::columns_from_sheet_width(10), 1);
    }
}
