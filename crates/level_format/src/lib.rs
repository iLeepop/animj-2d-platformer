//! `.level.ron` 的数据结构、解析与校验。见 `docs/03-level-format.md`。
//!
//! 本 crate 是**纯数据 + 纯函数**：只依赖 `serde` / `ron` / `std`，不依赖 Bevy、不读文件，
//! 因此解析与校验规则可以直接单测（见 `docs/05-architecture.md` §5.7 / §5.10）。
//!
//! - 数据形状：`Level` / `Grid` / `RoomCoord` / `Room` / `EntitySpec` / `Goal`（§3.2、§3.5）
//! - 瓦片 legend：`TileKind` / `LEGEND` / `RENDERABLE_TILE_KINDS`（§3.3）
//! - 读写单入口：`read` / `write`（§3.1）
//! - 校验：`validate`（§3.7）与 `name_matches_file`
//! - 版本：`VERSION`（§3.8；v1 无迁移可言，故本 crate 不提供 `migrate` 模块）

mod error;
mod io;
mod legend;
mod types;
mod validate;

pub use error::{LevelError, LevelIssue, Severity};
pub use io::{read, write};
pub use legend::{LEGEND, RENDERABLE_TILE_KINDS, TileKind};
pub use types::{
    Background, BackgroundLayer, Behavior, EntitySpec, Goal, Grid, Level, MAX_BACKGROUND_LAYERS,
    PaletteEntry, ROOM_PX, ROOM_TILES, Room, RoomCoord, TILE_PX, VERSION, columns_from_sheet_width,
    normalize_asset_path, room_index,
};
pub use validate::{name_matches_file, validate};
