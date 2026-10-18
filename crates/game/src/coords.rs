//! 坐标换算。**这两个函数是 `docs/00-conventions.md` §0.3 的唯一实现入口**：
//! 其它任何地方都禁止手写房间/瓦片算术。

use bevy::prelude::*;
use level_format::ROOM_PX;

/// 房间局部坐标（原点 = 房间**左上角**，+X 右、+Y **下**）→ 世界像素坐标（原点 = 地图左下角，+Y 上）。
///
/// `grid_rows` 是关卡的行数（房间网格的行数），用于把「行号自上而下」翻成「世界 Y 自下而上」。
pub fn room_local_to_world(col: u32, row: u32, grid_rows: u32, local: Vec2) -> Vec2 {
    Vec2::new(
        col as f32 * ROOM_PX as f32 + local.x,
        (grid_rows - 1 - row) as f32 * ROOM_PX as f32 + ROOM_PX as f32 - local.y,
    )
}

/// 世界像素坐标在房间网格中的归属。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomPos {
    pub col: u32,
    pub row: u32,
    /// 房间局部坐标（原点 = 房间左上角，+Y 向下），见 `00` §0.3。
    pub local: Vec2,
}

/// 世界像素坐标 → `(col, row, local)`。落在地图 AABB 之外时返回 `None`。
pub fn world_to_room(world: Vec2, grid_cols: u32, grid_rows: u32) -> Option<RoomPos> {
    let size = Vec2::new(
        grid_cols as f32 * ROOM_PX as f32,
        grid_rows as f32 * ROOM_PX as f32,
    );
    if world.x < 0.0 || world.y < 0.0 || world.x >= size.x || world.y >= size.y {
        return None;
    }
    let col = (world.x / ROOM_PX as f32).floor() as u32;
    let row_from_bottom = (world.y / ROOM_PX as f32).floor() as u32;
    let row = grid_rows - 1 - row_from_bottom;
    let local = Vec2::new(
        world.x - col as f32 * ROOM_PX as f32,
        ROOM_PX as f32 - (world.y - row_from_bottom as f32 * ROOM_PX as f32),
    );
    Some(RoomPos { col, row, local })
}

/// 房间 `(col, row)` 的**中心**（世界像素，整像素）＝ 相机落点。见 `02` §2.2。
pub fn room_center(col: u32, row: u32, grid_rows: u32) -> Vec2 {
    room_local_to_world(col, row, grid_rows, Vec2::splat(ROOM_PX as f32 / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use level_format::{ROOM_TILES, TILE_PX};

    /// 往返换算在房间内点与四个边界上成立。
    ///
    /// 注意 `local.y = 0`（房间**上**边界）**不在**往返范围内：按 `floor` 语义它归属于上方房间，
    /// 单独由 `top_edge_belongs_to_room_above` 断言（`00` §0.3）。
    #[test]
    fn room_local_round_trip() {
        let px = ROOM_PX as f32;
        for &(col, row) in &[(0u32, 0u32), (0, 2), (9, 0), (9, 2)] {
            for &local in &[
                Vec2::new(0.0, 1.0),      // 左上角内侧
                Vec2::new(px - 1.0, 1.0), // 右上角内侧
                Vec2::new(0.0, px),       // 左下角（下边界属于本房间）
                Vec2::new(px - 1.0, px),  // 右下角
                Vec2::splat(px / 2.0),    // 房间中心
            ] {
                let world = room_local_to_world(col, row, 3, local);
                let back = world_to_room(world, 10, 3).expect("地图内");
                assert_eq!((back.col, back.row), (col, row), "local={local:?}");
                assert!((back.local - local).length() < 1e-3, "local={local:?}");
            }
        }
    }

    /// 房间局部原点是房间**左上角**（`00` §0.3）：`local.y = 0` 是房间顶边，`local.y = ROOM_PX` 是底边。
    #[test]
    fn room_local_origin_is_top_left() {
        let px = ROOM_PX as f32;
        assert_eq!(room_local_to_world(0, 0, 1, Vec2::ZERO), Vec2::new(0.0, px));
        assert_eq!(room_local_to_world(0, 0, 1, Vec2::new(0.0, px)), Vec2::ZERO);
    }

    /// 上边界按 `floor` 语义归属**上方**房间：`(0, 1)` 的顶边同时是 `(0, 0)` 的底边。
    #[test]
    fn top_edge_belongs_to_room_above() {
        let boundary = room_local_to_world(0, 1, 3, Vec2::ZERO);
        let pos = world_to_room(boundary, 1, 3).expect("地图内");
        assert_eq!((pos.col, pos.row), (0, 0));
        assert_eq!(pos.local.y, ROOM_PX as f32);
    }

    /// `row = 0` 在地图**最上面**，因此它的世界 Y 最大（`00` §0.3）。
    #[test]
    fn row_zero_is_top_row() {
        let top = room_center(0, 0, 3);
        let bottom = room_center(0, 2, 3);
        assert!(top.y > bottom.y);
        assert_eq!(bottom.y, ROOM_PX as f32 / 2.0);
        assert_eq!(top.y, 2.0 * ROOM_PX as f32 + ROOM_PX as f32 / 2.0);
    }

    /// 房间中心是整像素，且相邻房间中心相距一个房间边长。
    #[test]
    fn room_centers_are_integral_and_adjacent() {
        let a = room_center(0, 1, 3);
        let b = room_center(1, 1, 3);
        assert_eq!(a.x.fract(), 0.0);
        assert_eq!(a.y.fract(), 0.0);
        assert_eq!(b.x - a.x, ROOM_PX as f32);
    }

    /// 地图边界之外没有归属。
    #[test]
    fn outside_map_has_no_room() {
        assert!(world_to_room(Vec2::new(-1.0, 0.0), 10, 3).is_none());
        assert!(world_to_room(Vec2::new(0.0, -1.0), 10, 3).is_none());
        assert!(world_to_room(Vec2::new(10.0 * ROOM_PX as f32, 0.0), 10, 3).is_none());
        assert!(world_to_room(Vec2::new(0.0, 3.0 * ROOM_PX as f32), 10, 3).is_none());
    }

    /// 瓦片坐标 `ty = 0` 是房间底行：房间局部 `ty = 0` 对应世界 Y 最小的一行。
    #[test]
    fn tile_row_zero_is_bottom() {
        let px = level_format::ROOM_PX as f32;
        // 房间局部 (0, ROOM_PX) = 房间左下角 = 世界原点（1×1 关卡）
        assert_eq!(room_local_to_world(0, 0, 1, Vec2::new(0.0, px)), Vec2::ZERO);
        // 底行的瓦片索引是 ty = 0（世界 Y 最小）
        assert_eq!(
            ROOM_TILES - 1 - ((px - 1.0) / TILE_PX as f32).floor() as usize,
            0
        );
        // 顶行的瓦片索引是 ty = ROOM_TILES - 1
        assert_eq!(
            ROOM_TILES - 1 - (1.0 / TILE_PX as f32).floor() as usize,
            ROOM_TILES - 1
        );
    }
}
