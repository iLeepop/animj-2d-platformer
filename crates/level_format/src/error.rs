//! 错误与校验问题类型。见 `docs/03-level-format.md` §3.7 / §3.8

use std::fmt::{Display, Formatter, Result as FmtResult};

/// 校验问题的严重度；加载/保存策略与 CLI 退出码见 `docs/03-level-format.md` §3.7
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// 结构不变式被破坏，不可能由合法编辑产生：加载与保存都必须拒绝。
    Fatal,
    /// 数据自相矛盾（如条件要求 3 个果实但房间里只有 2 个）：加载拒绝，保存允许。
    Error,
    /// 可疑但可用（如实体卡在实心瓦片里）：仅提示，退出码仍为 0。
    Warning,
}

impl Display for Severity {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(match self {
            Severity::Fatal => "Fatal",
            Severity::Error => "Error",
            Severity::Warning => "Warning",
        })
    }
}

/// 一条校验问题。见 `docs/03-level-format.md` §3.7
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelIssue {
    /// 严重度；决定加载/保存是否拒绝与 CLI 退出码。
    pub severity: Severity,
    /// 出错字段的路径，如 `rooms[13].goals[1].count`；瓦片类问题精确到「房间 + 文件行」。
    pub path: String,
    /// 人类可读描述，自带规则编号与字段路径，形如
    /// `V10 rooms[13].goals[1].count: 要求 3 个果实，房间内只有 2 个`。
    pub message: String,
}

impl Display for LevelIssue {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} {}", self.severity, self.message)
    }
}

/// `read()` 的失败原因。见 `docs/03-level-format.md` §3.1 / §3.8
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelError {
    /// RON 解析失败（缺字段、类型不符、未知字段、尾随垃圾等）；文本来自 ron，含行列号。
    Parse(String),
    /// 文件版本不是本引擎支持的版本：必须硬失败，禁止猜测、禁止宽容解析。
    UnsupportedVersion {
        /// 文件里写的版本号。
        found: u32,
        /// 本引擎支持的版本号（`VERSION`）。
        expected: u32,
    },
}

impl Display for LevelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            LevelError::Parse(message) => write!(f, "关卡解析失败：{message}"),
            LevelError::UnsupportedVersion { found, expected } => {
                write!(f, "文件版本 {found}，本引擎支持 {expected}")
            }
        }
    }
}

impl std::error::Error for LevelError {}
