//! 关卡校验：纯函数、不读文件、不依赖 Bevy、一次返回**全部**问题。见 `docs/03-level-format.md` §3.7

use crate::error::{LevelIssue, Severity};
use crate::legend::TileKind;
use crate::types::{
    EntitySpec, Goal, Level, ROOM_PX, ROOM_TILES, Room, TILE_PX, VERSION,
};

/// 房间总数上限，见 `docs/03-level-format.md` §3.7 V3。
const MAX_ROOMS: u64 = 256;
/// 超过该房间总数即给裁剪警告，见 `docs/03-level-format.md` §3.7 V3b 与 `docs/02-world-and-rooms.md` §2.4。
const ROOMS_WARN_THRESHOLD: u64 = 64;
/// v1 注册的实体 kind，见 `docs/03-level-format.md` §3.4；Roadmap 里的 kind 在 v1 会被 V14 拒绝。
const ENTITY_KINDS: &[&str] = &["player_spawn", "exit", "fruit", "checkpoint"];
/// 玩家出生点实体 kind，见 `docs/03-level-format.md` §3.4。
const PLAYER_SPAWN_KIND: &str = "player_spawn";

/// 校验关卡，返回**全部**问题（不短路）。
///
/// 规则编号 `V1`…`V18`、严重度与 `path` 格式见 `docs/03-level-format.md` §3.7。
/// 每条问题的 `message` 都带规则编号与字段路径，`path` 是纯字段路径。
pub fn validate(level: &Level) -> Vec<LevelIssue> {
    let mut issues = Vec::new();

    check_version(level, &mut issues);
    check_level_name(level, &mut issues);
    check_grid(level, &mut issues);
    check_room_count(level, &mut issues);

    for (i, room) in level.rooms.iter().enumerate() {
        check_tiles(i, room, level, &mut issues);
        check_goals(i, room, &mut issues);
        check_reward(i, level, &mut issues);
        check_entity_specs(i, room, &mut issues);
        check_room_spawn(i, room, &mut issues);
        check_room_name(i, level, &mut issues);
        check_entity_solid_overlap(i, room, &mut issues);
    }

    check_player_spawn(level, &mut issues);
    check_start(level, &mut issues);
    check_palette(level, &mut issues);
    check_room_boundaries(level, &mut issues);

    issues
}

/// 关卡 `name` 是否等于文件名主干（`assets/levels/world_01.level.ron` → `"world_01"`）。
///
/// V2 的文件名部分只在此校验：`validate` 是纯数据函数，看不到文件名。见 `docs/03-level-format.md` §3.1 / §3.7
pub fn name_matches_file(level: &Level, file_stem: &str) -> bool {
    level.name == file_stem
}

/// 追加一条问题：`message` 自动带上 `V<编号> <path>: ` 前缀，
/// 这样 CI 输出、文档与编辑器高亮能靠规则编号互相定位。见 `docs/03-level-format.md` §3.7
fn push(
    issues: &mut Vec<LevelIssue>,
    severity: Severity,
    path: &str,
    rule: &str,
    detail: impl Into<String>,
) {
    issues.push(LevelIssue {
        severity,
        path: path.to_owned(),
        message: format!("{rule} {path}: {}", detail.into()),
    });
}

/// `^[a-z0-9_]+$`：至少 1 个字符，且只含 ASCII 小写字母、数字、下划线。
fn is_snake_id(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// V1：`version == 1`（Fatal）。
fn check_version(level: &Level, issues: &mut Vec<LevelIssue>) {
    if level.version != VERSION {
        push(
            issues,
            Severity::Fatal,
            "version",
            "V1",
            format!("要求 version == {VERSION}，实际 {}", level.version),
        );
    }
}

/// V2：`name` 匹配 `^[a-z0-9_]+$`（文件名一致性见 [`name_matches_file`]）。
fn check_level_name(level: &Level, issues: &mut Vec<LevelIssue>) {
    if !is_snake_id(&level.name) {
        push(
            issues,
            Severity::Error,
            "name",
            "V2",
            format!("'{}' 不匹配 ^[a-z0-9_]+$", level.name),
        );
    }
}

/// V3 / V3b：网格尺寸合法性与房间总数警告。
fn check_grid(level: &Level, issues: &mut Vec<LevelIssue>) {
    let cols = level.grid.cols;
    let rows = level.grid.rows;
    let total = u64::from(cols) * u64::from(rows);

    let mut violations = Vec::new();
    if cols == 0 {
        violations.push(format!("cols >= 1（实际 {cols}）"));
    }
    if rows == 0 {
        violations.push(format!("rows >= 1（实际 {rows}）"));
    }
    if total > MAX_ROOMS {
        violations.push(format!("cols * rows <= {MAX_ROOMS}（实际 {total}）"));
    }
    if !violations.is_empty() {
        push(
            issues,
            Severity::Error,
            "grid",
            "V3",
            format!("要求 {}", violations.join("、")),
        );
    }

    if total > ROOMS_WARN_THRESHOLD {
        let message = format!(
            "房间总数 {total} > {ROOMS_WARN_THRESHOLD}，超出裁剪 Roadmap（见 docs/02 §2.4）"
        );
        push(issues, Severity::Warning, "grid", "V3b", message);
    }
}

/// V4：`rooms.len() == cols * rows`。
fn check_room_count(level: &Level, issues: &mut Vec<LevelIssue>) {
    let expected = u64::from(level.grid.cols) * u64::from(level.grid.rows);
    let actual = level.rooms.len() as u64;
    if actual != expected {
        push(
            issues,
            Severity::Error,
            "rooms",
            "V4",
            format!(
                "房间数 {actual} 与 grid {}x{} = {expected} 不符",
                level.grid.cols, level.grid.rows
            ),
        );
    }
}

/// V5 / V6：瓦片行数、行长度与 legend 字符集。
fn check_tiles(i: usize, room: &Room, level: &Level, issues: &mut Vec<LevelIssue>) {
    if room.tiles.len() != ROOM_TILES {
        push(
            issues,
            Severity::Fatal,
            &format!("rooms[{i}].tiles"),
            "V5",
            format!("行数 {}，期望 {ROOM_TILES}", room.tiles.len()),
        );
    }
    for (file_row, line) in room.tiles.iter().enumerate() {
        let path = format!("rooms[{i}].tiles[{file_row}]");
        let len = line.chars().count();
        if len != ROOM_TILES {
            push(
                issues,
                Severity::Fatal,
                &path,
                "V5",
                format!("行长度 {len}，期望 {ROOM_TILES}"),
            );
        }
        for (col, c) in line.chars().enumerate() {
            if c != '.' && level.behavior_of(c).is_none() {
                push(
                    issues,
                    Severity::Error,
                    &path,
                    "V6",
                    format!("列 {col} 的字符 '{c}' 不在调色板中"),
                );
            }
        }
    }
}

/// 房间内某个 kind 的实体数量；与 `Collect { count: u32 }` 直接可比，故用 `u32`。
fn count_kind(room: &Room, kind: &str) -> u32 {
    room.entities
        .iter()
        .filter(|spec| spec.kind == kind)
        .count() as u32
}

/// V7 / V8 / V9 / V10：目标列表与实体的可满足性。
fn check_goals(i: usize, room: &Room, issues: &mut Vec<LevelIssue>) {
    if room.goals.is_empty() {
        push(
            issues,
            Severity::Error,
            &format!("rooms[{i}].goals"),
            "V7",
            "目标列表为空，至少需要 1 个目标",
        );
    }

    let exit_count = count_kind(room, "exit");
    let fruit_count = count_kind(room, "fruit");

    if room.goals.contains(&Goal::ReachExit) && exit_count != 1 {
        push(
            issues,
            Severity::Error,
            &format!("rooms[{i}].entities"),
            "V8",
            format!("含 ReachExit，要求恰好 1 个 exit 实体，实际 {exit_count} 个"),
        );
    }
    if room.goals.contains(&Goal::CollectAll) && fruit_count == 0 {
        push(
            issues,
            Severity::Error,
            &format!("rooms[{i}].entities"),
            "V9",
            "含 CollectAll，要求至少 1 个 fruit 实体，实际 0 个",
        );
    }

    for (goal_index, goal) in room.goals.iter().enumerate() {
        let Goal::Collect { count } = goal else {
            continue;
        };
        let path = format!("rooms[{i}].goals[{goal_index}].count");
        if *count == 0 {
            push(
                issues,
                Severity::Error,
                &path,
                "V10",
                "要求 0 个果实，count 必须 >= 1",
            );
        } else if *count > fruit_count {
            push(
                issues,
                Severity::Error,
                &path,
                "V10",
                format!("要求 {count} 个果实，房间内只有 {fruit_count} 个"),
            );
        }
    }
}

/// V11：`reward` 的字符集与全关卡唯一性。
fn check_reward(i: usize, level: &Level, issues: &mut Vec<LevelIssue>) {
    let Some(reward) = level.rooms[i].reward.as_deref() else {
        return;
    };
    let path = format!("rooms[{i}].reward");
    if !is_snake_id(reward) {
        push(
            issues,
            Severity::Error,
            &path,
            "V11",
            format!("'{reward}' 不匹配 ^[a-z0-9_]+$"),
        );
    }
    let duplicate = level.rooms[..i]
        .iter()
        .position(|room| room.reward.as_deref() == Some(reward));
    if let Some(j) = duplicate {
        push(
            issues,
            Severity::Error,
            &path,
            "V11",
            format!("装饰品 id '{reward}' 与 rooms[{j}].reward 重复"),
        );
    }
}

/// V13 / V14：实体位置、尺寸范围与 kind 注册表。
fn check_entity_specs(i: usize, room: &Room, issues: &mut Vec<LevelIssue>) {
    for (e, spec) in room.entities.iter().enumerate() {
        if !ENTITY_KINDS.contains(&spec.kind.as_str()) {
            push(
                issues,
                Severity::Error,
                &format!("rooms[{i}].entities[{e}].kind"),
                "V14",
                format!(
                    "未知实体 kind '{}'，v1 注册表：{}",
                    spec.kind,
                    ENTITY_KINDS.join(" / ")
                ),
            );
        }

        let (x, y) = spec.at;
        if !(0..ROOM_PX).contains(&x) || !(0..ROOM_PX).contains(&y) {
            push(
                issues,
                Severity::Error,
                &format!("rooms[{i}].entities[{e}].at"),
                "V13",
                format!("at ({x}, {y}) 超出房间范围 0..{ROOM_PX}"),
            );
        }

        if let Some((w, h)) = spec.size {
            let out_of_range = !(1..=ROOM_PX).contains(&w) || !(1..=ROOM_PX).contains(&h);
            if out_of_range {
                push(
                    issues,
                    Severity::Error,
                    &format!("rooms[{i}].entities[{e}].size"),
                    "V13",
                    format!("size ({w}, {h}) 超出范围 1..={ROOM_PX}"),
                );
            }
        }
    }
}

/// V15：房间重生点范围。
fn check_room_spawn(i: usize, room: &Room, issues: &mut Vec<LevelIssue>) {
    let Some((x, y)) = room.spawn else {
        return;
    };
    if !(0..ROOM_PX).contains(&x) || !(0..ROOM_PX).contains(&y) {
        push(
            issues,
            Severity::Error,
            &format!("rooms[{i}].spawn"),
            "V15",
            format!("重生点 ({x}, {y}) 超出房间范围 0..{ROOM_PX}"),
        );
    }
}

/// V16（房间名部分）：房间名全关卡唯一。
fn check_room_name(i: usize, level: &Level, issues: &mut Vec<LevelIssue>) {
    let Some(name) = level.rooms[i].name.as_deref() else {
        return;
    };
    let duplicate = level.rooms[..i]
        .iter()
        .position(|room| room.name.as_deref() == Some(name));
    if let Some(j) = duplicate {
        push(
            issues,
            Severity::Error,
            &format!("rooms[{i}].name"),
            "V16",
            format!("房间名 '{name}' 与 rooms[{j}].name 重复"),
        );
    }
}

/// V12：全关卡恰好 1 个 `player_spawn`（**可以位于任意房间**；游戏从该房间开始）。
fn check_player_spawn(level: &Level, issues: &mut Vec<LevelIssue>) {
    let mut spawns = Vec::new();
    for (room_index_in_level, room) in level.rooms.iter().enumerate() {
        for (entity_index, spec) in room.entities.iter().enumerate() {
            if spec.kind == PLAYER_SPAWN_KIND {
                spawns.push((room_index_in_level, entity_index));
            }
        }
    }

    if spawns.len() != 1 {
        push(
            issues,
            Severity::Error,
            "rooms",
            "V12",
            format!(
                "全关卡 {PLAYER_SPAWN_KIND} 实体数 {}，要求恰好 1 个",
                spawns.len()
            ),
        );
    }
}

/// V16（start 部分）：起始房间坐标在网格内。
fn check_start(level: &Level, issues: &mut Vec<LevelIssue>) {
    if level.start.col >= level.grid.cols || level.start.row >= level.grid.rows {
        push(
            issues,
            Severity::Error,
            "start",
            "V16",
            format!(
                "起始房间 ({}, {}) 超出 grid {}x{}",
                level.start.col, level.start.row, level.grid.cols, level.grid.rows
            ),
        );
    }
}

/// 实体的房间局部 AABB（左闭右开像素区间）：`at` 是中心，`size` 缺省为 `(32, 32)`。
fn entity_aabb(spec: &EntitySpec) -> (i32, i32, i32, i32) {
    let (w, h) = spec.size_or_default();
    let (center_x, center_y) = spec.at;
    let min_x = center_x - w / 2;
    let min_y = center_y - h / 2;
    (min_x, min_y, min_x + w, min_y + h)
}

/// 两个左闭右开矩形的重叠判定；仅共边不算重叠（实体恰好站在地面之上是合法的）。
fn rects_overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// 取文件坐标上的瓦片（`file_row 0` = 房间最上一行）。
///
/// 校验期间 `tiles` 可能尚未满足 16×16 不变式（V5 会单独报错），故越界与非法字符按 `Air` 处理。
fn tile_at_file(room: &Room, file_row: usize, col: usize) -> TileKind {
    let Some(line) = room.tiles.get(file_row) else {
        return TileKind::Air;
    };
    match line.chars().nth(col) {
        Some(c) => TileKind::from_char(c).unwrap_or(TileKind::Air),
        None => TileKind::Air,
    }
}

/// V17：实体 AABB 与 `Solid` 瓦片重叠（每实体最多报一条，给出首个重叠瓦片）。
fn check_entity_solid_overlap(i: usize, room: &Room, issues: &mut Vec<LevelIssue>) {
    for (e, spec) in room.entities.iter().enumerate() {
        let aabb = entity_aabb(spec);
        let Some((col, file_row)) = first_solid_overlap(room, aabb) else {
            continue;
        };
        push(
            issues,
            Severity::Warning,
            &format!("rooms[{i}].entities[{e}].at"),
            "V17",
            format!(
                "实体包围盒 x[{}, {}) y[{}, {}) 与 Solid 瓦片重叠（列 {col}、文件行 {file_row}）",
                aabb.0, aabb.2, aabb.1, aabb.3
            ),
        );
    }
}

/// 返回与该 AABB 重叠的第一个 `Solid` 瓦片（列, 文件行）。
fn first_solid_overlap(room: &Room, aabb: (i32, i32, i32, i32)) -> Option<(usize, usize)> {
    for file_row in 0..ROOM_TILES {
        for col in 0..ROOM_TILES {
            if !tile_at_file(room, file_row, col).is_solid() {
                continue;
            }
            let min_x = col as i32 * TILE_PX;
            let min_y = file_row as i32 * TILE_PX;
            let tile = (min_x, min_y, min_x + TILE_PX, min_y + TILE_PX);
            if rects_overlap(aabb, tile) {
                return Some((col, file_row));
            }
        }
    }
    None
}

/// 边界不连续：一侧 `Solid`、另一侧 `Air`，形成单像素墙。
fn is_boundary_gap(a: TileKind, b: TileKind) -> bool {
    matches!(
        (a, b),
        (TileKind::Solid, TileKind::Air) | (TileKind::Air, TileKind::Solid)
    )
}

/// V19：调色板自身合法——字符唯一、非 `.`、名称非空。
fn check_palette(level: &Level, issues: &mut Vec<LevelIssue>) {
    let mut seen: Vec<char> = Vec::new();
    for (index, entry) in level.palette.iter().enumerate() {
        let path = format!("palette[{index}]");
        if entry.char == '.' {
            push(
                issues,
                Severity::Error,
                &path,
                "V19",
                "字符 '.' 是空气，保留不可作为地块".to_string(),
            );
        }
        if seen.contains(&entry.char) {
            push(
                issues,
                Severity::Error,
                &path,
                "V19",
                format!("字符 '{}' 重复", entry.char),
            );
        }
        seen.push(entry.char);
        if entry.name.trim().is_empty() {
            push(
                issues,
                Severity::Error,
                &path,
                "V19",
                "地块名称不能为空".to_string(),
            );
        }
    }
}

/// V18：相邻房间接缝处的瓦片不连续。
///
/// 只比较真正相邻的房间对（右邻 / 下邻各报一次，避免重复）。
fn check_room_boundaries(level: &Level, issues: &mut Vec<LevelIssue>) {
    let cols = level.grid.cols;
    let rows = level.grid.rows;
    if cols == 0 || rows == 0 || level.rooms.len() != cols as usize * rows as usize {
        // grid 或房间数已由 V3 / V4 报错，此处房间索引不可靠
        return;
    }

    for (i, room) in level.rooms.iter().enumerate() {
        let col = i as u32 % cols;
        let row = i as u32 / cols;

        if col + 1 < cols {
            let j = i + 1;
            let right = &level.rooms[j];
            for file_row in 0..ROOM_TILES {
                let here = tile_at_file(room, file_row, ROOM_TILES - 1);
                let neighbor = tile_at_file(right, file_row, 0);
                if is_boundary_gap(here, neighbor) {
                    push(
                        issues,
                        Severity::Warning,
                        &format!("rooms[{j}].tiles[{file_row}]"),
                        "V18",
                        format!(
                            "与左邻 rooms[{i}] 的边界不连续（本侧 '{}'，左邻 '{}'），形成单像素墙",
                            here.to_char(),
                            neighbor.to_char()
                        ),
                    );
                }
            }
        }

        if row + 1 < rows {
            let j = i + cols as usize;
            let below = &level.rooms[j];
            for tile_col in 0..ROOM_TILES {
                let here = tile_at_file(below, 0, tile_col);
                let above = tile_at_file(room, ROOM_TILES - 1, tile_col);
                if is_boundary_gap(here, above) {
                    push(
                        issues,
                        Severity::Warning,
                        &format!("rooms[{j}].tiles[0]"),
                        "V18",
                        format!(
                            "与上方 rooms[{i}] 的边界在第 {tile_col} 列不连续（本侧 '{}'，上方 '{}'），形成单像素墙",
                            here.to_char(),
                            above.to_char()
                        ),
                    );
                }
            }
        }
    }
}
