//! 瓦片存储与查询。见 `docs/05-architecture.md` §5.2、`docs/06-physics-and-movement.md` §6.6。
//!
//! 索引约定：`ty = 0` 是房间**底行**（+Y 向上），与 `00` §0.3 的瓦片坐标一致；
//! 翻转逻辑只在 `level_format::Room::tile_at` 里实现一次。

use bevy::prelude::*;
use level_format::{Level, ROOM_TILES, Room, TILE_PX, TileKind, room_index};

use crate::physics::{TileKind as PhysTileKind, TileWorld};

/// 单个房间的瓦片快照（16×16，`ty = 0` 为底行）。
#[derive(Clone)]
pub struct RoomTiles {
    kinds: [TileKind; ROOM_TILES * ROOM_TILES],
    /// 每个格子的图集贴图格（`None` = 不渲染）。
    cells: [Option<u32>; ROOM_TILES * ROOM_TILES],
}

impl RoomTiles {
    /// 由房间 + 关卡调色板构建（`03` §3.9）：行为走 `Behavior`，渲染走 `cell`。
    fn from_room(room: &Room, level: &Level) -> Self {
        let mut kinds = [TileKind::Air; ROOM_TILES * ROOM_TILES];
        let mut cells = [None; ROOM_TILES * ROOM_TILES];
        for file_row in 0..ROOM_TILES {
            let ty = ROOM_TILES - 1 - file_row;
            let Some(line) = room.tiles.get(file_row) else {
                continue;
            };
            for (tx, ch) in line.chars().enumerate().take(ROOM_TILES) {
                if ch == '.' {
                    continue;
                }
                let Some(entry) = level.entry_of(ch) else {
                    continue;
                };
                let kind = match entry.behavior {
                    level_format::Behavior::Solid => TileKind::Solid,
                    level_format::Behavior::OneWay => TileKind::OneWay,
                    level_format::Behavior::Hazard => TileKind::Spike,
                    level_format::Behavior::None => TileKind::Decor,
                };
                kinds[ty * ROOM_TILES + tx] = kind;
                cells[ty * ROOM_TILES + tx] = Some(entry.cell);
            }
        }
        Self { kinds, cells }
    }

    /// 该格的图集贴图格（`None` = 不渲染）。
    pub fn cell(&self, tx: usize, ty: usize) -> Option<u32> {
        if tx >= ROOM_TILES || ty >= ROOM_TILES {
            return None;
        }
        self.cells[ty * ROOM_TILES + tx]
    }

    /// 房间局部瓦片坐标 → 瓦片类型。越界返回 `Air`。
    pub fn get(&self, tx: usize, ty: usize) -> TileKind {
        if tx >= ROOM_TILES || ty >= ROOM_TILES {
            return TileKind::Air;
        }
        self.kinds[ty * ROOM_TILES + tx]
    }
}

/// 整关的瓦片查询入口。由关卡加载时构建，之后只读。
#[derive(Resource, Clone)]
pub struct LevelTiles {
    grid: (u32, u32),
    rooms: Vec<RoomTiles>,
}

impl LevelTiles {
    pub fn from_level(level: &Level) -> Self {
        Self {
            grid: (level.grid.cols, level.grid.rows),
            rooms: level
                .rooms
                .iter()
                .map(|room| RoomTiles::from_room(room, level))
                .collect(),
        }
    }

    pub fn grid(&self) -> (u32, u32) {
        self.grid
    }

    pub fn room(&self, col: u32, row: u32) -> Option<&RoomTiles> {
        let (cols, rows) = self.grid;
        if col >= cols || row >= rows {
            return None;
        }
        self.rooms.get(room_index(col, row, cols))
    }

    /// 世界像素坐标处的瓦片类型。
    ///
    /// **地图 AABB 之外一律返回 `Solid`**：那是地图的隐形外框墙，避免玩家掉出世界，
    /// 也避免在每个调用点各写一遍越界判断（`02` §2.1 的引擎级不变量）。
    pub fn tile_at_world(&self, world: Vec2) -> TileKind {
        let Some(pos) = crate::coords::world_to_room(world, self.grid.0, self.grid.1) else {
            return TileKind::Solid;
        };
        let tx = (pos.local.x / TILE_PX as f32).floor();
        let file_row = (pos.local.y / TILE_PX as f32).floor();
        let ty = (ROOM_TILES - 1) as f32 - file_row;
        self.room(pos.col, pos.row)
            .map(|r| r.get(tx as usize, ty as usize))
            .unwrap_or(TileKind::Solid)
    }
}

/// 供物理内核使用的**只读**瓦片视图（`06` §6.6）。
///
/// 坐标是**全局**瓦片坐标（原点 = 地图左下角，+Y 向上）；房间映射按 `06` §6.6.4 的整数公式。
/// `LevelTiles` 常驻全部房间（`02` §2.4：本阶段不做逻辑层房间激活），因此相邻房间天然返回真实数据。
impl TileWorld for LevelTiles {
    fn tile(&self, tx: i32, ty: i32) -> PhysTileKind {
        if tx < 0 || ty < 0 {
            return PhysTileKind::Solid; // 地图外 = 硬墙
        }
        let (cols, rows) = self.grid;
        let col = tx.div_euclid(ROOM_TILES as i32);
        let left = tx.rem_euclid(ROOM_TILES as i32);
        let row_from_bottom = ty.div_euclid(ROOM_TILES as i32);
        let bottom = ty.rem_euclid(ROOM_TILES as i32);
        if col >= cols as i32 || row_from_bottom >= rows as i32 {
            return PhysTileKind::Solid; // grid 范围外
        }
        let row = rows as i32 - 1 - row_from_bottom;
        match self
            .room(col as u32, row as u32)
            .map(|room| room.get(left as usize, bottom as usize))
        {
            Some(TileKind::Solid) => PhysTileKind::Solid,
            Some(TileKind::OneWay) => PhysTileKind::OneWay,
            Some(TileKind::Spike) => PhysTileKind::Hazard,
            // `Air` 与 `Decor` 对物理都是空
            Some(TileKind::Air) | Some(TileKind::Decor) => PhysTileKind::Empty,
            None => PhysTileKind::Solid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use level_format::{Grid, RoomCoord};

    /// 用一张「底行全实心、其余全空」的 1×1 关卡验证查询的上下翻转。
    fn one_room_level() -> Level {
        let mut tiles = vec!["................".to_string(); ROOM_TILES];
        tiles[ROOM_TILES - 1] = "#".repeat(ROOM_TILES);
        Level {
            version: level_format::VERSION,
            name: "t".to_string(),
            grid: Grid { cols: 1, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![Room {
                name: None,
                tiles,
                goals: vec![level_format::Goal::ReachExit],
                reward: None,
                spawn: None,
                entities: vec![
                    level_format::EntitySpec {
                        kind: "player_spawn".to_string(),
                        at: (64, 64),
                        size: None,
                        id: None,
                    },
                    level_format::EntitySpec {
                        kind: "exit".to_string(),
                        at: (128, 64),
                        size: None,
                        id: None,
                    },
                ],
            }],
        }
    }

    #[test]
    fn bottom_row_is_solid_top_row_is_air() {
        let tiles = LevelTiles::from_level(&one_room_level());
        // 房间局部 y = 16（贴底）→ 底行 = Solid
        assert_eq!(tiles.tile_at_world(Vec2::new(16.0, 16.0)), TileKind::Solid);
        // 房间局部 y = 496（顶部）→ Air
        assert_eq!(tiles.tile_at_world(Vec2::new(16.0, 496.0)), TileKind::Air);
    }

    #[test]
    fn outside_map_is_solid_wall() {
        let tiles = LevelTiles::from_level(&one_room_level());
        assert_eq!(tiles.tile_at_world(Vec2::new(-1.0, 16.0)), TileKind::Solid);
        assert_eq!(tiles.tile_at_world(Vec2::new(16.0, -1.0)), TileKind::Solid);
        assert_eq!(
            tiles.tile_at_world(Vec2::new(16.0, level_format::ROOM_PX as f32 + 1.0)),
            TileKind::Solid
        );
    }

    /// 造一个 2×1 关卡：左房底行全实心，右房底行只有最左一格实心。
    fn two_room_level() -> Level {
        let mut left = vec!["................".to_string(); ROOM_TILES];
        left[ROOM_TILES - 1] = "#".repeat(ROOM_TILES);
        let mut right = vec!["................".to_string(); ROOM_TILES];
        right[ROOM_TILES - 1] = format!("#{}", ".".repeat(ROOM_TILES - 1));

        let make_room = |tiles: Vec<String>| Room {
            name: None,
            tiles,
            goals: vec![level_format::Goal::ReachExit],
            reward: None,
            spawn: None,
            entities: vec![
                level_format::EntitySpec {
                    kind: "player_spawn".to_string(),
                    at: (64, 64),
                    size: None,
                    id: None,
                },
                level_format::EntitySpec {
                    kind: "exit".to_string(),
                    at: (128, 64),
                    size: None,
                    id: None,
                },
            ],
        };

        Level {
            version: level_format::VERSION,
            name: "t".to_string(),
            grid: Grid { cols: 2, rows: 1 },
            start: RoomCoord { col: 0, row: 0 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![make_room(left), make_room(right)],
        }
    }

    /// R6：**物理网格**与**坐标/渲染网格**必须指向同一格。
    ///
    /// 物理走 `floor(world / TILE_PX)`（全局整数瓦片），渲染与关卡数据走 `coords` + `Room::tile_at`。
    /// 两条路径若差半格，玩家就会"看着站在砖上、实际踩空"（实机表现为半格没有碰撞）。
    #[test]
    fn r6_physics_grid_matches_coord_grid() {
        use crate::physics::{TileKind as Phys, TileWorld};

        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/levels/world_01.level.ron"
        ));
        let level = level_format::read(text).expect("出厂关卡必须能解析");
        let tiles = LevelTiles::from_level(&level);
        let tile = TILE_PX as f32;

        for row in 0..level.grid.rows {
            for col in 0..level.grid.cols {
                let room = tiles.room(col, row).expect("房间存在");
                for ty in 0..ROOM_TILES {
                    for tx in 0..ROOM_TILES {
                        // 该瓦片的**中心**（房间局部，原点左上 +Y 向下）→ 世界
                        let local = Vec2::new(
                            tx as f32 * tile + tile / 2.0,
                            (level_format::ROOM_PX - TILE_PX) as f32 - ty as f32 * tile + tile / 2.0,
                        );
                        let world = crate::coords::room_local_to_world(col, row, level.grid.rows, local);

                        // 路径一：坐标换算 + 瓦片存储（渲染语义）
                        let stored = tiles.tile_at_world(world);
                        // 本测试比较的是**格位与碰撞语义**：`Decor` 的 behavior 是 None，
                        // 物理网格里等价于 Air（外观种类不参与物理）。
                        let via_coords = match stored {
                            TileKind::Decor => TileKind::Air,
                            other => other,
                        };
                        // 路径二：物理网格（整数瓦片 + TileWorld）
                        let gx = (world.x / tile).floor() as i32;
                        let gy = (world.y / tile).floor() as i32;
                        let via_physics = match tiles.tile(gx, gy) {
                            Phys::Solid => TileKind::Solid,
                            Phys::OneWay => TileKind::OneWay,
                            Phys::Hazard => TileKind::Spike,
                            Phys::Empty => TileKind::Air,
                        };
                        assert_eq!(
                            via_coords, via_physics,
                            "房间 ({col},{row}) 瓦片 ({tx},{ty}) 世界 {world:?}：坐标路径 {via_coords:?} ≠ 物理路径 {via_physics:?}"
                        );
                        assert_eq!(room.get(tx, ty), stored, "瓦片存储与坐标路径不一致");
                    }
                }
            }
        }
    }

    /// T20b 垂直相邻房间（`01` §1.8 的"上下纵深"）：上房底行与下房顶行在瓦片网格上**直接相邻**，
    /// 跨界查询必须取到相邻房间的真实数据（`06` §6.6.4）。
    #[test]
    fn t20b_vertical_neighbours_share_the_grid_boundary() {
        use crate::physics::{TileKind as Phys, TileWorld};

        let make_room = |tiles: Vec<String>| Room {
            name: None,
            tiles,
            goals: vec![level_format::Goal::ReachExit],
            reward: None,
            spawn: None,
            entities: vec![
                level_format::EntitySpec {
                    kind: "player_spawn".to_string(),
                    at: (64, 64),
                    size: None,
                    id: None,
                },
                level_format::EntitySpec {
                    kind: "exit".to_string(),
                    at: (128, 64),
                    size: None,
                    id: None,
                },
            ],
        };
        // 上房（row 0）：底行留空 ⇒ 可以从这里掉进下房
        let mut top = vec!["................".to_string(); ROOM_TILES];
        top[ROOM_TILES - 1] = "................".to_string();
        // 下房（row 1）：顶行实心 ⇒ 与该边界比较，证明取的是**各自房间**的数据
        let mut bottom = vec!["................".to_string(); ROOM_TILES];
        bottom[0] = "#".repeat(ROOM_TILES);

        let level = Level {
            version: level_format::VERSION,
            name: "t".to_string(),
            grid: Grid { cols: 1, rows: 2 },
            start: RoomCoord { col: 0, row: 1 },
            tileset: None,
            tileset_columns: 8,
            palette: Vec::new(),
            background: None,
            rooms: vec![make_room(top), make_room(bottom)],
        };
        let tiles = LevelTiles::from_level(&level);

        // 下房的顶行（全局 ty = 15）= Solid
        assert_eq!(tiles.tile(3, 15), Phys::Solid);
        // 上房的底行（全局 ty = 16）= Empty（相邻房间，不是下房的顶行）
        assert_eq!(tiles.tile(3, 16), Phys::Empty);
        // 地图顶/底之外恒为 Solid
        assert_eq!(tiles.tile(3, 32), Phys::Solid);
        assert_eq!(tiles.tile(3, -1), Phys::Solid);

        // 坐标路径与物理路径在同一世界点一致（边界两侧各取一次）
        let tile = TILE_PX as f32;
        assert_eq!(
            tiles.tile_at_world(Vec2::new(3.0 * tile + 1.0, 15.0 * tile + 1.0)),
            TileKind::Solid
        );
        assert_eq!(
            tiles.tile_at_world(Vec2::new(3.0 * tile + 1.0, 16.0 * tile + 1.0)),
            TileKind::Air
        );
    }

    /// R4：**出厂关卡的每个房间**都必须能把角色放在实地上，且碰撞盒不变形。
    ///
    /// 实机 bug：`rooms[1]` 没有 `spawn` 字段 ⇒ 走默认重生点 ⇒ 身体埋进地板 14 px ⇒
    /// 分离把盒子压反 ⇒ 之后穿地板、掉出相机。
    #[test]
    fn shipped_level_spawns_land_with_intact_box() {
        use crate::config::physics::{DEFAULT_PHYSICS, PLAYER_SIZE};
        use crate::physics::{Body, StepInput, StepOutcome, step};

        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/levels/world_01.level.ron"
        ));
        let level = level_format::read(text).expect("出厂关卡必须能解析");
        let tiles = LevelTiles::from_level(&level);

        for row in 0..level.grid.rows {
            for col in 0..level.grid.cols {
                let center = crate::world::room_spawn_center(&level, col, row);
                let mut body = Body::at_center(center);
                let mut out = StepOutcome::default();
                for i in 0..180 {
                    step(
                        &DEFAULT_PHYSICS,
                        &tiles,
                        &mut body,
                        StepInput::default(),
                        &mut out,
                    );
                    let size = body.aabb.size();
                    assert!(
                        (size - PLAYER_SIZE).abs().max_element() < 1e-3,
                        "房间 ({col},{row}) 第 {i} 步碰撞盒变形：{size:?}"
                    );
                }
                assert!(body.grounded, "房间 ({col},{row}) 重生点没能落到实地上");
                // 重生点不得压在尖刺上：否则死亡→重生→立刻再死，形成无限循环
                assert!(
                    !crate::physics::overlaps_tile_kind(
                        &tiles,
                        body.aabb,
                        -crate::config::physics::HAZARD_INSET,
                        crate::physics::TileKind::Hazard,
                    ),
                    "房间 ({col},{row}) 的重生点落在危险瓦片上"
                );
            }
        }
    }

    /// v2：自定义字符按调色板的 `behavior` 走物理、按 `cell` 走渲染（`03` §3.9）。
    #[test]
    fn v2_custom_palette_drives_behavior_and_cell() {
        use crate::physics::{TileKind as Phys, TileWorld};
        use level_format::{Behavior, PaletteEntry};

        let mut level = one_room_level();
        level.tileset = Some("sprites/tiles/custom.png".to_string());
        level.tileset_columns = 4;
        level.palette = vec![
            PaletteEntry { char: 'S', name: "stone".into(), behavior: Behavior::Solid, cell: 5 },
            PaletteEntry { char: 'g', name: "grass".into(), behavior: Behavior::None, cell: 7 },
            PaletteEntry { char: 'p', name: "plank".into(), behavior: Behavior::OneWay, cell: 2 },
            PaletteEntry { char: 'h', name: "hurt".into(), behavior: Behavior::Hazard, cell: 9 },
        ];
        // 底行用自定义实心；再放一格台阶/危险/装饰
        level.rooms[0].tiles[15] = "SSSSSSSSSSSSSSSS".to_string();
        level.rooms[0].tiles[14] = "................".to_string();
        level.rooms[0].tiles[14] = format!("{}p.h{}", ".".repeat(12), ".".repeat(1));

        let tiles = LevelTiles::from_level(&level);
        // 物理：自定义 `S` = 实心
        assert_eq!(tiles.tile(0, 0), Phys::Solid);
        // 渲染：贴图格来自调色板
        assert_eq!(tiles.room(0, 0).unwrap().cell(0, 0), Some(5));
        assert_eq!(tiles.room(0, 0).unwrap().cell(12, 1), Some(2));
        assert_eq!(tiles.room(0, 0).unwrap().cell(14, 1), Some(9));
        // 空格的贴图格为 None
        assert_eq!(tiles.room(0, 0).unwrap().cell(1, 5), None);
    }

    /// T20 跨房间 / 越界查询（`06` §6.9）：房界外侧 1 格返回**相邻房间的真实瓦片**；`grid` 外恒为 `Solid`。
    #[test]
    fn t20_cross_room_and_out_of_bounds() {
        use crate::physics::{TileKind as Phys, TileWorld};

        let tiles = LevelTiles::from_level(&two_room_level());
        let room = ROOM_TILES as i32;

        // 左房底行全实心（全局 ty = 0）：tx = 15 是左房最右一格
        assert_eq!(tiles.tile(15, 0), Phys::Solid);
        // 跨过房界的外侧 1 格 = 右房左下角 ⇒ 必须返回右房真实数据（Solid），而不是 Empty
        assert_eq!(tiles.tile(room, 0), Phys::Solid);
        // 右房底行的第二格是空的
        assert_eq!(tiles.tile(room + 1, 0), Phys::Empty);
        // 右房底行以外的部分也是空的
        assert_eq!(tiles.tile(room + 1, 1), Phys::Empty);

        // 越界恒为 Solid（地图外是硬墙）
        assert_eq!(tiles.tile(-1, 0), Phys::Solid);
        assert_eq!(tiles.tile(0, -1), Phys::Solid);
        assert_eq!(tiles.tile(2 * room, 0), Phys::Solid);
        assert_eq!(tiles.tile(0, room), Phys::Solid);
    }
}
