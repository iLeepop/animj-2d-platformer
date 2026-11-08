//! `.level.ron` 的读写单入口。见 `docs/03-level-format.md` §3.1
//!
//! 版本迁移：v1/v2 的房间级目标（`goals`）与装饰品奖励（`reward`）在 v3 已移除；
//! 旧文件的 `exit` 实体迁移为终点 `goal`。迁移在 [`read`] 内完成，调用方永远只看到当前版本。

use ron::ser::PrettyConfig;
use serde::Deserialize;

use crate::error::LevelError;
use crate::types::{Background, EntitySpec, Level, PaletteEntry, Room, VERSION};

/// 解析 `.level.ron` 文本：先解析、再校验版本，旧版本就地迁移到 [`VERSION`]。
///
/// 失败一律硬失败，不做任何宽容解析或猜测：解析失败 → [`LevelError::Parse`]，
/// 版本不匹配 → [`LevelError::UnsupportedVersion`]。见 `docs/03-level-format.md` §3.1 / §3.8
pub fn read(text: &str) -> Result<Level, LevelError> {
    #[derive(Deserialize)]
    struct VersionOnly {
        version: u32,
    }
    let probe: VersionOnly =
        ron::from_str(text).map_err(|error| LevelError::Parse(error.to_string()))?;
    match probe.version {
        VERSION => {
            ron::from_str::<Level>(text).map_err(|error| LevelError::Parse(error.to_string()))
        }
        // v1 是 v2 的子集（新增字段都有默认值），v1/v2 共用同一份旧结构。
        1 | 2 => {
            let legacy: LegacyLevel =
                ron::from_str(text).map_err(|error| LevelError::Parse(error.to_string()))?;
            Ok(migrate(legacy))
        }
        found => Err(LevelError::UnsupportedVersion {
            found,
            expected: VERSION,
        }),
    }
}

/// 序列化关卡为 RON 文本，**全项目唯一的序列化入口**（调用方禁止自行 `ron::to_string`，
/// 否则会产生无意义的巨大 diff）。固定参数：4 空格缩进、`\\n` 换行、不枚举数组。
///
/// 保证 `read(&write(level)) == Ok(level.clone())`。见 `docs/03-level-format.md` §3.1
pub fn write(level: &Level) -> String {
    let config = PrettyConfig::new()
        .indentor("    ")
        .new_line("\n")
        .enumerate_arrays(false);
    // 这些纯数据类型（无 Map、无浮点、无自定义 Serialize）不存在序列化失败路径
    ron::ser::to_string_pretty(level, config).expect("Level 的 RON 序列化不会失败")
}

/// v1/v2 的关卡结构（只用于迁移读取）。字段与历史格式一致。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLevel {
    version: u32,
    name: String,
    grid: crate::types::Grid,
    start: crate::types::RoomCoord,
    #[serde(default)]
    tileset: Option<String>,
    #[serde(default = "crate::types::default_tileset_columns")]
    tileset_columns: u32,
    #[serde(default)]
    palette: Vec<PaletteEntry>,
    #[serde(default)]
    background: Option<Background>,
    rooms: Vec<LegacyRoom>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRoom {
    #[serde(default)]
    name: Option<String>,
    tiles: Vec<String>,
    #[serde(default)]
    goals: Vec<LegacyGoal>,
    #[serde(default)]
    reward: Option<String>,
    #[serde(default)]
    spawn: Option<(i32, i32)>,
    entities: Vec<EntitySpec>,
}

/// v1/v2 的房间目标（v3 起移除）。保留定义只为读取旧文件。
#[derive(Deserialize)]
#[allow(dead_code)]
enum LegacyGoal {
    ReachExit,
    CollectAll,
    Collect { count: u32 },
}

/// 旧关卡 → 当前关卡：删除 `goals` / `reward`，`exit` 实体改名为 `goal`。
fn migrate(legacy: LegacyLevel) -> Level {
    let _ = legacy.version;
    Level {
        version: VERSION,
        name: legacy.name,
        grid: legacy.grid,
        start: legacy.start,
        tileset: legacy.tileset,
        tileset_columns: legacy.tileset_columns,
        palette: legacy.palette,
        background: legacy.background,
        rooms: legacy
            .rooms
            .into_iter()
            .map(|room| {
                let _ = (&room.goals, &room.reward);
                Room {
                    name: room.name,
                    tiles: room.tiles,
                    spawn: room.spawn,
                    entities: room
                        .entities
                        .into_iter()
                        .map(|mut entity| {
                            if entity.kind == "exit" {
                                entity.kind = "goal".to_string();
                            }
                            entity
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}
