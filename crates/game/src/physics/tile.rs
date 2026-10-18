//! 物理层的瓦片视图。所有者文档：`docs/06-physics-and-movement.md` §6.6.1。
//!
//! 注意这里的 [`TileKind`] 是**物理投影**（4 种），与 `level_format` 的 5 种 landmark 不是同一个类型：
//! `Air` / `Decor` → [`TileKind::Empty`]，`Spike` → [`TileKind::Hazard`]。映射在 ECS 适配层做（`tiles.rs`）。

/// 物理语义的瓦片类别。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileKind {
    /// 空（含 `level_format` 的 `Air` 与 `Decor`）。
    Empty,
    /// 实心：两轴都阻挡、可站立。
    Solid,
    /// 单向平台：仅 `06` §6.4.5 的三条件成立时在 Y 轴阻挡；可站立。
    OneWay,
    /// 危险：不阻挡，只由传感器查询（`06` §6.5.3）。
    Hazard,
}

impl TileKind {
    pub fn is_solid(self) -> bool {
        self == TileKind::Solid
    }

    pub fn is_standable(self) -> bool {
        matches!(self, TileKind::Solid | TileKind::OneWay)
    }
}

/// 只读瓦片视图。**全局**瓦片坐标（原点 = 地图左下角，+Y 向上），转换见 `06` §6.6.4。
///
/// 只有 [`TileWorld::tile`] 是必须实现的：测试用的 `TestWorld` 只需实现一个方法。
pub trait TileWorld {
    /// 全局瓦片坐标处的物理语义。地图外必须返回 [`TileKind::Solid`]（`06` §6.6.3）。
    fn tile(&self, tx: i32, ty: i32) -> TileKind;

    /// 矩形范围内是否存在指定类别。默认朴素实现（`06` §6.4.8 保证单次查询 ≤ 4 格）。
    fn any_kind_in(&self, tx0: i32, ty0: i32, tx1: i32, ty1: i32, kind: TileKind) -> bool {
        (ty0..=ty1).any(|ty| (tx0..=tx1).any(|tx| self.tile(tx, ty) == kind))
    }
}
