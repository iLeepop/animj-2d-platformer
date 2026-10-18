//! `.level.ron` 的读写单入口。见 `docs/03-level-format.md` §3.1

use ron::ser::PrettyConfig;

use crate::error::LevelError;
use crate::types::{Level, VERSION};

/// 解析 `.level.ron` 文本：先解析、再校验版本。
///
/// 失败一律硬失败，不做任何宽容解析或猜测：解析失败 → [`LevelError::Parse`]，
/// 版本不匹配 → [`LevelError::UnsupportedVersion`]。见 `docs/03-level-format.md` §3.1 / §3.8
pub fn read(text: &str) -> Result<Level, LevelError> {
    #[derive(serde::Deserialize)]
    struct VersionOnly {
        version: u32,
    }
    let probe: VersionOnly =
        ron::from_str(text).map_err(|error| LevelError::Parse(error.to_string()))?;
    match probe.version {
        // 当前版本
        VERSION => {
            ron::from_str::<Level>(text).map_err(|error| LevelError::Parse(error.to_string()))
        }
        // v1 → v2：v1 是 v2 的子集（新字段都有默认值），解析后把版本号升上来即可
        1 => {
            let mut level: Level =
                ron::from_str(text).map_err(|error| LevelError::Parse(error.to_string()))?;
            level.version = VERSION;
            Ok(level)
        }
        found => Err(LevelError::UnsupportedVersion {
            found,
            expected: VERSION,
        }),
    }
}

/// 序列化关卡为 RON 文本，**全项目唯一的序列化入口**（调用方禁止自行 `ron::to_string`，
/// 否则会产生无意义的巨大 diff）。固定参数：4 空格缩进、`\n` 换行、不枚举数组。
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
