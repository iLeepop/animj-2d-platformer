//! `.level.ron` 的数据结构、解析与校验。见 `docs/03-level-format.md`。
//!
//! 本 crate 是**纯数据 + 纯函数**：只依赖 `serde` / `ron` / `std`，不依赖 Bevy、不读文件，
//! 因此解析与校验规则可以直接单测（见 `docs/05-architecture.md` §5.7 / §5.10）。
//!
//! - 数据形状：`Level` / `Grid` / `RoomCoord` / `Room` / `EntitySpec` / `ItemKind`（§3.2、§3.4）
//! - 瓦片 legend：`TileKind` / `LEGEND` / `RENDERABLE_TILE_KINDS`（§3.3）
//! - 读写单入口：`read` / `write`（§3.1；v1/v2 → v3 迁移在同文件内完成）
//! - 校验：`validate`（§3.7）与 `name_matches_file`
//! - 版本：`VERSION`（§3.8）

mod error;
mod io;
mod legend;
mod types;
mod validate;

pub use error::{LevelError, LevelIssue, Severity};
pub use io::{read, write};
pub use legend::{LEGEND, RENDERABLE_TILE_KINDS, TileKind};
pub use types::{
    Background, BackgroundLayer, Behavior, CollisionPart, CollisionShape,
    DEFAULT_ITEM_DURATION_SECS, EntitySpec, Grid, ItemConfig, ItemDef, ItemKind, Level,
    MAX_BACKGROUND_LAYERS, MAX_COLLISION_VERTICES, PICKUP_SIZE, PaletteEntry, ROOM_PX, ROOM_TILES,
    Room, RoomCoord, TILE_PX, VERSION, columns_from_sheet_width, normalize_asset_path, room_index,
};
pub use validate::{name_matches_file, validate};
