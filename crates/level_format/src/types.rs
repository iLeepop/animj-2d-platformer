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

/// 调色板条目：字符 → 名称 + 行为 + 贴图格 + 可选碰撞形状（32×32、行主序、`tileset_columns` 列）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaletteEntry {
    pub char: char,
    pub name: String,
    pub behavior: Behavior,
    pub cell: u32,
    /// 自定义碰撞范围（`03` §3.9）。`None` = 整格 32×32；`Some` = 瓦片内简单多边形。
    /// 仅对 `Solid` / `OneWay` / `Hazard` 生效；`None` 行为不使用。
    ///
    /// `skip_serializing_if`：默认值不写盘，旧关卡保存后不会多出无意义的 `collision: None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision: Option<CollisionShape>,
}

/// 碰撞多边形的顶点数上限（`03` §3.9 / V21）。
pub const MAX_COLLISION_VERTICES: usize = 32;

/// 瓦片碰撞多边形：顶点为**瓦片局部像素**，原点 = 瓦片左上角，+Y 向下，取值 `0..=TILE_PX`。
///
/// 物理不直接使用连续多边形：加载时按**像素中心**栅格化为轴对齐块（[`CollisionShape::raster_parts`]），
/// 因此斜边呈像素阶梯——与 `06` §6.4.7「不做连续斜坡」的既有决策一致。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CollisionShape {
    pub vertices: Vec<(i32, i32)>,
}

/// 碰撞多边形栅格化后的轴对齐块（瓦片局部像素，原点 = 左上角，+Y 向下）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionPart {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl CollisionShape {
    /// 整格矩形的顶点（与 `None` 等价，供编辑器从默认值起步）。
    pub fn full() -> Self {
        Self {
            vertices: vec![(0, 0), (TILE_PX, 0), (TILE_PX, TILE_PX), (0, TILE_PX)],
        }
    }

    /// 由左上角与宽高构造矩形顶点（编辑器预设用）。
    pub fn rectangle(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            vertices: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
        }
    }

    /// 按像素中心采样（奇偶规则）把多边形栅格化为轴对齐块，并把上下相邻的同一水平段合并。
    ///
    /// 返回的坐标与顶点同一朝向（原点左上、+Y 向下）。完全落在像素点之外的多边形返回空 `Vec`。
    pub fn raster_parts(&self) -> Vec<CollisionPart> {
        const N: usize = TILE_PX as usize;
        let mut mask = [[false; N]; N];
        for (py, row) in mask.iter_mut().enumerate() {
            for (px, inside) in row.iter_mut().enumerate() {
                *inside = point_in_polygon(&self.vertices, px as f64 + 0.5, py as f64 + 0.5);
            }
        }

        // 逐行取水平段（半开区间 [x0, x1)），再把上下完全相同的段合并成矩形。
        let mut parts = Vec::new();
        // (x0, x1, 起始行)
        let mut open: Vec<(i32, i32, i32)> = Vec::new();
        for (py, row) in mask.iter().enumerate() {
            let mut runs: Vec<(i32, i32)> = Vec::new();
            let mut px = 0;
            while px < N {
                if !row[px] {
                    px += 1;
                    continue;
                }
                let start = px;
                while px < N && row[px] {
                    px += 1;
                }
                runs.push((start as i32, px as i32));
            }

            for &(x0, x1, y_start) in &open {
                if !runs.contains(&(x0, x1)) {
                    parts.push(CollisionPart {
                        x: x0,
                        y: y_start,
                        w: x1 - x0,
                        h: py as i32 - y_start,
                    });
                }
            }
            let mut next = Vec::new();
            for &(x0, x1) in &runs {
                let y_start = open
                    .iter()
                    .find(|(a, b, _)| *a == x0 && *b == x1)
                    .map(|(_, _, y_start)| *y_start)
                    .unwrap_or(py as i32);
                next.push((x0, x1, y_start));
            }
            open = next;
        }
        for (x0, x1, y_start) in open {
            parts.push(CollisionPart {
                x: x0,
                y: y_start,
                w: x1 - x0,
                h: N as i32 - y_start,
            });
        }
        parts
    }

    /// 多边形结构是否可被物理使用（顶点数/范围/重复点/面积/自交）。返回全部问题。
    ///
    /// 规则编号与严重度由 [`crate::validate`] 的 V21 负责；此处只给人类可读的原因。
    pub fn geometry_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let n = self.vertices.len();
        if n < 3 {
            errors.push(format!("顶点数 {n}，至少需要 3 个"));
        }
        if n > MAX_COLLISION_VERTICES {
            errors.push(format!("顶点数 {n} 超过上限 {MAX_COLLISION_VERTICES}"));
        }
        for (index, (x, y)) in self.vertices.iter().enumerate() {
            if !(0..=TILE_PX).contains(x) || !(0..=TILE_PX).contains(y) {
                errors.push(format!(
                    "顶点 {index} ({x}, {y}) 超出瓦片范围 0..={TILE_PX}"
                ));
            }
        }
        for i in 0..n {
            for j in (i + 1)..n {
                if self.vertices[i] == self.vertices[j] {
                    errors.push(format!("顶点 {i} 与顶点 {j} 重合"));
                }
            }
        }
        if n >= 3 {
            let doubled_area: i64 = (0..n)
                .map(|i| {
                    let (x0, y0) = self.vertices[i];
                    let (x1, y1) = self.vertices[(i + 1) % n];
                    x0 as i64 * y1 as i64 - x1 as i64 * y0 as i64
                })
                .sum();
            if doubled_area == 0 {
                errors.push("多边形面积为 0".to_string());
            }
        }
        if n >= 4 {
            for i in 0..n {
                let a = self.vertices[i];
                let b = self.vertices[(i + 1) % n];
                for j in (i + 1)..n {
                    if j == i || j == (i + 1) % n || (j + 1) % n == i {
                        continue; // 相邻边共享端点
                    }
                    let c = self.vertices[j];
                    let d = self.vertices[(j + 1) % n];
                    if segments_intersect(a, b, c, d) {
                        errors.push(format!("边 {i} 与边 {j} 自交"));
                    }
                }
            }
        }
        errors
    }
}

/// 点是否在多边形内（奇偶规则；用于栅格化）。
fn point_in_polygon(vertices: &[(i32, i32)], x: f64, y: f64) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (vertices[i].0 as f64, vertices[i].1 as f64);
        let (xj, yj) = (vertices[j].0 as f64, vertices[j].1 as f64);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// 二维叉积的符号（`i64` 足够：坐标 ≤ 32）。
fn orient(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i64 {
    let ab = ((b.0 - a.0) as i64, (b.1 - a.1) as i64);
    let ac = ((c.0 - a.0) as i64, (c.1 - a.1) as i64);
    ab.0 * ac.1 - ab.1 * ac.0
}

/// `p` 是否落在共线的 `q`–`r` 线段上。
fn on_segment(q: (i32, i32), p: (i32, i32), r: (i32, i32)) -> bool {
    p.0 >= q.0.min(r.0) && p.0 <= q.0.max(r.0) && p.1 >= q.1.min(r.1) && p.1 <= q.1.max(r.1)
}

/// 两线段是否相交（含共线重叠与端点接触）。
fn segments_intersect(a: (i32, i32), b: (i32, i32), c: (i32, i32), d: (i32, i32)) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    if o1 == 0 && on_segment(a, c, b) {
        return true;
    }
    if o2 == 0 && on_segment(a, d, b) {
        return true;
    }
    if o3 == 0 && on_segment(c, a, d) {
        return true;
    }
    if o4 == 0 && on_segment(c, b, d) {
        return true;
    }
    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
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
    let stripped = path.strip_prefix("assets/").unwrap_or(path);
    let stripped = stripped.trim_start_matches('/');
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

impl Level {
    /// 内置调色板（等价 v1 的固定 legend）。
    pub fn builtin_palette() -> Vec<PaletteEntry> {
        vec![
            PaletteEntry {
                char: '#',
                name: "solid".into(),
                behavior: Behavior::Solid,
                cell: 0,
                collision: None,
            },
            PaletteEntry {
                char: '=',
                name: "oneway".into(),
                behavior: Behavior::OneWay,
                cell: 1,
                collision: None,
            },
            PaletteEntry {
                char: '^',
                name: "spike".into(),
                behavior: Behavior::Hazard,
                cell: 2,
                collision: None,
            },
            PaletteEntry {
                char: '*',
                name: "decor".into(),
                behavior: Behavior::None,
                cell: 3,
                collision: None,
            },
        ]
    }

    /// 生效调色板（空 ⇒ 内置）。
    pub fn palette(&self) -> Vec<PaletteEntry> {
        if self.palette.is_empty() {
            Self::builtin_palette()
        } else {
            self.palette.clone()
        }
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

#[cfg(test)]
mod collision_tests {
    use super::{CollisionPart, CollisionShape, TILE_PX};

    #[test]
    fn full_tile_rasterizes_to_one_part() {
        assert_eq!(
            CollisionShape::full().raster_parts(),
            vec![CollisionPart {
                x: 0,
                y: 0,
                w: TILE_PX,
                h: TILE_PX,
            }]
        );
        // 反向绕序（顺时针）必须得到同一结果：奇偶规则与绕序无关
        let mut reversed = CollisionShape::full();
        reversed.vertices.reverse();
        assert_eq!(
            reversed.raster_parts(),
            CollisionShape::full().raster_parts()
        );
    }

    #[test]
    fn rectangle_rasterizes_to_exact_bounds() {
        let parts = CollisionShape::rectangle(0, 16, 32, 16).raster_parts();
        assert_eq!(
            parts,
            vec![CollisionPart {
                x: 0,
                y: 16,
                w: 32,
                h: 16,
            }]
        );
        // 右半格
        let parts = CollisionShape::rectangle(16, 0, 16, 32).raster_parts();
        assert_eq!(
            parts,
            vec![CollisionPart {
                x: 16,
                y: 0,
                w: 16,
                h: 32,
            }]
        );
    }

    /// 直角三角形栅格化后面积 ≈ 半个瓦片（像素中心采样产生少量取整误差）。
    #[test]
    fn triangle_rasterizes_to_about_half_the_tile() {
        let shape = CollisionShape {
            vertices: vec![(0, 0), (TILE_PX, 0), (0, TILE_PX)],
        };
        let parts = shape.raster_parts();
        assert!(!parts.is_empty());
        let area: i32 = parts.iter().map(|part| part.w * part.h).sum();
        assert!(
            (400..=550).contains(&area),
            "三角碰撞块面积 {area} 应接近 512"
        );
        // 斜边像素化：至少存在两个宽度不同的水平段（纯矩形不可能）
        assert!(
            parts
                .iter()
                .map(|part| (part.y, part.w))
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| pair[0].1 != pair[1].1)
                || parts.len() > 1,
            "斜边应产生阶梯或分段：{parts:?}"
        );
    }

    /// 面积为正但窄到不含任何像素中心的薄片：栅格化结果为空（V22 的输入）。
    #[test]
    fn sliver_without_pixel_centres_rasterizes_empty() {
        let shape = CollisionShape {
            vertices: vec![(0, 0), (1, 0), (0, 1)],
        };
        assert!(shape.raster_parts().is_empty());
        assert!(shape.geometry_errors().is_empty(), "结构本身合法");
    }

    #[test]
    fn geometry_errors_report_bad_polygons() {
        let too_few = CollisionShape {
            vertices: vec![(0, 0), (1, 1)],
        };
        assert!(
            too_few
                .geometry_errors()
                .iter()
                .any(|error| error.contains("至少需要 3"))
        );

        let out_of_range = CollisionShape {
            vertices: vec![(0, 0), (33, 0), (0, 1)],
        };
        assert!(
            out_of_range
                .geometry_errors()
                .iter()
                .any(|error| error.contains("超出瓦片范围"))
        );

        let duplicate = CollisionShape {
            vertices: vec![(0, 0), (8, 0), (0, 0)],
        };
        assert!(
            duplicate
                .geometry_errors()
                .iter()
                .any(|error| error.contains("重合"))
        );

        let zero_area = CollisionShape {
            vertices: vec![(0, 0), (8, 0), (16, 0)],
        };
        assert!(
            zero_area
                .geometry_errors()
                .iter()
                .any(|error| error.contains("面积为 0"))
        );

        // 蝴蝶结：非相邻边相交
        let bowtie = CollisionShape {
            vertices: vec![(0, 0), (32, 32), (32, 0), (0, 32)],
        };
        assert!(
            bowtie
                .geometry_errors()
                .iter()
                .any(|error| error.contains("自交")),
            "{:?}",
            bowtie.geometry_errors()
        );

        assert!(CollisionShape::full().geometry_errors().is_empty());
        assert!(
            CollisionShape::rectangle(0, 16, 32, 16)
                .geometry_errors()
                .is_empty()
        );
    }
}
