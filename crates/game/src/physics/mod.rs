//! 物理内核。所有者文档：`docs/06-physics-and-movement.md`。
//!
//! 本模块是**纯函数**，不含 ECS 依赖：ECS 适配层在 `crate::player`，渲染投影在 `crate::render`。

pub mod aabb;
pub mod core;
#[cfg(test)]
mod tests;
pub mod tile;

pub use aabb::Aabb;
pub use core::{Body, StepInput, StepOutcome, TILE, overlaps_tile_kind, step, step_with_players};
pub use tile::{TileKind, TileWorld};
