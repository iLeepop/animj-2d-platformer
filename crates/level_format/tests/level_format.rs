//! `level_format` 的契约测试：`docs/03-level-format.md` §3.1–§3.8 的每一条规则各配正例与反例。
//!
//! 测试只使用 crate 的公共 API（`tests/` 是独立 crate），因此也顺带验证了导出面。

use level_format::{
    Background, BackgroundLayer, EntitySpec, Goal, Grid, LEGEND, Level, LevelError, LevelIssue,
    MAX_BACKGROUND_LAYERS, RENDERABLE_TILE_KINDS, ROOM_PX, ROOM_TILES, Room, RoomCoord, Severity,
    TILE_PX, TileKind, VERSION, name_matches_file, read, room_index, validate, write,
};

// ---------------------------------------------------------------- 构造辅助

/// 16 行全 `Air` 的瓦片。
fn air_tiles() -> Vec<String> {
    vec![".".repeat(ROOM_TILES); ROOM_TILES]
}

/// 带天花板与地面的房间瓦片：文件第 0 行与第 15 行实心，中间为空气。
///
/// 这样水平与竖直接缝两侧都是同类瓦片，不会触发 V18，便于构造「本应零问题」的关卡。
fn sealed_room_tiles() -> Vec<String> {
    let mut tiles = air_tiles();
    tiles[0] = "#".repeat(ROOM_TILES);
    tiles[15] = "#".repeat(ROOM_TILES);
    tiles
}

fn entity(kind: &str, at: (i32, i32)) -> EntitySpec {
    EntitySpec {
        kind: kind.to_string(),
        at,
        size: None,
        id: None,
    }
}

/// 一个「密封」房间：天花板 + 地面，无房间名、无奖励、无重生点。
fn plain_room(goals: Vec<Goal>, entities: Vec<EntitySpec>) -> Room {
    Room {
        name: None,
        tiles: sealed_room_tiles(),
        goals,
        reward: None,
        spawn: None,
        entities,
    }
}

/// 最小合法关卡：1×1，地面在第 15 行，`ReachExit` + 1 个 `player_spawn` + 1 个 `exit`。
fn valid_level() -> Level {
    let mut tiles = air_tiles();
    tiles[15] = "#".repeat(ROOM_TILES);
    Level {
        version: VERSION,
        name: "test_level".to_string(),
        grid: Grid { cols: 1, rows: 1 },
        start: RoomCoord { col: 0, row: 0 },
        tileset: None,
        tileset_columns: 8,
        palette: Vec::new(),
        background: None,
        rooms: vec![Room {
            name: None,
            tiles,
            goals: vec![Goal::ReachExit],
            reward: None,
            spawn: None,
            entities: vec![
                entity("player_spawn", (64, 464)),
                entity("exit", (448, 464)),
            ],
        }],
    }
}

/// 2×1 合法关卡：`rooms[0]` 是 start。
fn valid_level_2x1() -> Level {
    Level {
        version: VERSION,
        name: "two_rooms".to_string(),
        grid: Grid { cols: 2, rows: 1 },
        start: RoomCoord { col: 0, row: 0 },
        tileset: None,
        tileset_columns: 8,
        palette: Vec::new(),
        background: None,
        rooms: vec![
            plain_room(
                vec![Goal::ReachExit],
                vec![
                    entity("player_spawn", (64, 464)),
                    entity("exit", (448, 464)),
                ],
            ),
            plain_room(vec![Goal::ReachExit], vec![entity("exit", (448, 464))]),
        ],
    }
}

/// 1×2 合法关卡：`rooms[0]`（上）是 start。
fn valid_level_1x2() -> Level {
    Level {
        version: VERSION,
        name: "stacked".to_string(),
        grid: Grid { cols: 1, rows: 2 },
        start: RoomCoord { col: 0, row: 0 },
        tileset: None,
        tileset_columns: 8,
        palette: Vec::new(),
        background: None,
        rooms: vec![
            plain_room(
                vec![Goal::ReachExit],
                vec![
                    entity("player_spawn", (64, 464)),
                    entity("exit", (448, 464)),
                ],
            ),
            plain_room(vec![Goal::ReachExit], vec![entity("exit", (448, 464))]),
        ],
    }
}

/// 1×1 关卡：`ReachExit` + `Collect { count }`，并放好 `count` 个 fruit。
fn level_with_fruits(count: u32) -> Level {
    let mut level = valid_level();
    level.rooms[0].goals = vec![Goal::ReachExit, Goal::Collect { count }];
    for i in 0..count {
        level.rooms[0]
            .entities
            .push(entity("fruit", (64 + 32 * i as i32, 464)));
    }
    level
}

/// 覆盖全部字段（房间名、奖励、重生点、三种目标、显式 size/id）且完全合法的 2×2 关卡。
fn featureful_level() -> Level {
    let mut left_room = plain_room(vec![Goal::ReachExit], vec![entity("exit", (448, 464))]);
    left_room.name = Some("a".to_string());

    let mut start_room = plain_room(
        vec![Goal::ReachExit, Goal::CollectAll],
        vec![
            entity("player_spawn", (64, 464)),
            entity("exit", (448, 464)),
            EntitySpec {
                kind: "fruit".to_string(),
                at: (208, 464),
                size: Some((24, 24)),
                id: Some("f1".to_string()),
            },
            entity("fruit", (272, 464)),
        ],
    );
    start_room.name = Some("b".to_string());
    start_room.reward = Some("straw_hat".to_string());
    start_room.spawn = Some((256, 256));

    let mut collect_room = plain_room(
        vec![Goal::Collect { count: 2 }],
        vec![
            entity("exit", (448, 464)),
            entity("fruit", (208, 464)),
            entity("fruit", (272, 464)),
        ],
    );
    collect_room.reward = Some("scarf".to_string());

    Level {
        version: VERSION,
        name: "featureful".to_string(),
        grid: Grid { cols: 2, rows: 2 },
        start: RoomCoord { col: 1, row: 0 },
        tileset: None,
        tileset_columns: 8,
        palette: Vec::new(),
        background: None,
        rooms: vec![
            left_room,
            start_room,
            collect_room,
            plain_room(vec![Goal::ReachExit], vec![entity("exit", (448, 464))]),
        ],
    }
}

/// 取 `validate` 中某条规则（`V5 ` / `V18 ` 这样的前缀，避免 `V1` 命中 `V10`）报出的全部问题。
fn issues_of(level: &Level, rule: &str) -> Vec<LevelIssue> {
    let prefix = format!("{rule} ");
    validate(level)
        .into_iter()
        .filter(|issue| issue.message.starts_with(&prefix))
        .collect()
}

/// `docs/03-level-format.md` §3.6 的完整示例，逐字复制（含 `rooms[0].tiles[10]` 故意写错的 17 字符行）。
///
/// raw string 的分隔符必须长于 16 个 `#`：示例里的 `"################"` 会让 `"#` 之类的分隔符提前闭合。
const EXAMPLE_LEVEL_RON: &str = r########################################"(
    version: 1,
    name: "world_01",
    grid: (cols: 2, rows: 1),
    start: (col: 0, row: 0),
    rooms: [
        // ---- rooms[0] = (col 0, row 0)，起始房间 ----
        (
            name: Some("start"),
            reward: None,
            spawn: None,
            goals: [ ReachExit ],
            tiles: [
                "................",  // 文件第 0 行 = 房间最上一行（本地 y ∈ [0, 32)）
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                ".................",  // ← 故意错误：17 个字符，触发 V5
                "................",
                "......====......",  // 单向平台，列 6..9
                "................",
                "................",
                "################",  // 地面，本地 y ∈ [480, 512)
            ],
            entities: [
                // at 是 AABB 中心：32x32 实体 ⇒ 该 AABB 为 x[48,80] y[448,480]，正好站在地面上
                (kind: "player_spawn", at: (64, 464)),
                (kind: "exit", at: (448, 464)),
            ],
        ),
        // ---- rooms[1] = (col 1, row 0) ----
        (
            name: Some("fruit_room"),
            reward: Some("straw_hat"),
            spawn: None,
            goals: [ ReachExit, Collect(count: 2) ],
            tiles: [
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
                ".......==.......",  // 单向平台，列 7..8
                "................",
                "................",
                "................",
                "#####.^^^.######",  // 地面高度比 rooms[0] 高一格，接缝处形成 1 格台阶
                "################",
            ],
            entities: [
                (kind: "exit",           at: (448, 432)),
                (kind: "fruit",          at: (208, 432)),
                (kind: "fruit",          at: (272, 432)),
                (kind: "fruit",          at: (336, 432)),
            ],
        ),
    ],
)"########################################;

// ---------------------------------------------------------------- 常量与数据形状

#[test]
fn constants_match_conventions() {
    assert_eq!(VERSION, 2);
    assert_eq!(TILE_PX, 32);
    assert_eq!(ROOM_TILES, 16);
    assert_eq!(ROOM_PX, 512);
    assert_eq!(ROOM_PX, TILE_PX * ROOM_TILES as i32);
}

#[test]
fn legend_covers_all_tile_kinds() {
    assert_eq!(
        LEGEND.to_vec(),
        vec![
            ('.', TileKind::Air),
            ('#', TileKind::Solid),
            ('=', TileKind::OneWay),
            ('^', TileKind::Spike),
            ('*', TileKind::Decor),
        ]
    );
    assert_eq!(
        RENDERABLE_TILE_KINDS.to_vec(),
        vec![
            TileKind::Solid,
            TileKind::OneWay,
            TileKind::Spike,
            TileKind::Decor,
        ]
    );

    for (index, kind) in RENDERABLE_TILE_KINDS.iter().enumerate() {
        assert_eq!(kind.atlas_index(), Some(index), "图集索引即本常量的顺序");
        assert_eq!(TileKind::from_char(kind.to_char()), Some(*kind));
        assert_eq!(kind.is_solid(), *kind == TileKind::Solid);
    }
    assert_eq!(TileKind::Air.atlas_index(), None, "Air 不占图集格");
    assert_eq!(TileKind::from_char('.'), Some(TileKind::Air));
    assert_eq!(TileKind::from_char('x'), None);

    // legend 覆盖 Air 与全部可渲染瓦片，字符不重复，且与 from_char / to_char 一致
    let mut chars: Vec<char> = LEGEND.iter().map(|(c, _)| *c).collect();
    let legend_len = chars.len();
    chars.sort_unstable();
    chars.dedup();
    assert_eq!(chars.len(), legend_len, "legend 字符不得重复");
    assert!(
        LEGEND
            .iter()
            .any(|(c, kind)| *c == '.' && *kind == TileKind::Air)
    );
    for kind in RENDERABLE_TILE_KINDS {
        assert!(
            LEGEND.iter().any(|(_, k)| k == kind),
            "{kind:?} 必须出现在 legend 中"
        );
    }
    assert!(
        LEGEND
            .iter()
            .all(|(c, kind)| TileKind::from_char(*c) == Some(*kind) && kind.to_char() == *c)
    );
}

#[test]
fn size_or_default_is_tile_px() {
    let mut spec = entity("fruit", (64, 464));
    assert_eq!(spec.size_or_default(), (TILE_PX, TILE_PX));
    assert_eq!(spec.size_or_default(), (32, 32));
    spec.size = Some((64, 8));
    assert_eq!(spec.size_or_default(), (64, 8));
}

#[test]
fn room_lookup_follows_row_major_index() {
    let level = valid_level_2x1();
    assert_eq!(
        level.room(0, 0),
        Some(&level.rooms[room_index(0, 0, level.grid.cols)])
    );
    assert_eq!(
        level.room(1, 0),
        Some(&level.rooms[room_index(1, 0, level.grid.cols)])
    );
    assert_eq!(level.room(2, 0), None, "col 越界");
    assert_eq!(level.room(0, 1), None, "row 越界");

    // docs/03 §3.6 的 10×3 索引语义：row 0 是最上一行，index = row * cols + col
    assert_eq!(room_index(0, 0, 10), 0);
    assert_eq!(room_index(9, 0, 10), 9);
    assert_eq!(room_index(0, 1, 10), 10);
    assert_eq!(room_index(0, 2, 10), 20);
    assert_eq!(room_index(9, 2, 10), 29);

    let mut broken = valid_level_2x1();
    broken.rooms.pop();
    assert_eq!(broken.room(1, 0), None, "rooms 长度不足时不得越界");
}

#[test]
fn tile_at_flips_y_axis_and_clamps_to_air() {
    // 文件行 r 的 '#' 放在第 r 列 ⇒ 该瓦片的房间局部坐标必然是 (tx = r, ty = ROOM_TILES - 1 - r)
    let mut tiles = air_tiles();
    for (r, line) in tiles.iter_mut().enumerate() {
        let mut chars: Vec<char> = line.chars().collect();
        chars[r] = '#';
        *line = chars.into_iter().collect();
    }
    let room = Room {
        name: None,
        tiles,
        goals: vec![Goal::ReachExit],
        reward: None,
        spawn: None,
        entities: Vec::new(),
    };

    for r in 0..ROOM_TILES {
        let ty = ROOM_TILES - 1 - r;
        assert_eq!(
            room.tile_at(r, ty),
            TileKind::Solid,
            "文件行 {r} 的实心瓦片应落在 ty = {ty}"
        );
        assert_eq!(
            room.tile_at((r + 1) % ROOM_TILES, ty),
            TileKind::Air,
            "同一文件行的其它列应为 Air"
        );
    }
    assert_eq!(
        room.tile_at(0, 0),
        TileKind::Air,
        "ty = 0 是底行：文件第 15 行第 0 列"
    );
    assert_eq!(room.tile_at(ROOM_TILES - 1, 0), TileKind::Solid);
    assert_eq!(room.tile_at(ROOM_TILES, 0), TileKind::Air, "tx 越界");
    assert_eq!(room.tile_at(0, ROOM_TILES), TileKind::Air, "ty 越界");
    assert_eq!(
        room.tile_at(usize::MAX, usize::MAX),
        TileKind::Air,
        "越界不得 panic"
    );
    assert_eq!(room.tile_at(0, usize::MAX), TileKind::Air, "越界不得 panic");

    // 坏数据（行缺失 / 行过短 / 非 legend 字符）一律按 Air 兜底，不得 panic
    let broken = Room {
        tiles: vec!["#".to_string()],
        ..plain_room(Vec::new(), Vec::new())
    };
    assert_eq!(broken.tile_at(0, ROOM_TILES - 1), TileKind::Solid);
    assert_eq!(broken.tile_at(1, ROOM_TILES - 1), TileKind::Air);
    assert_eq!(broken.tile_at(0, 0), TileKind::Air, "文件第 15 行不存在");
    let invalid = Room {
        tiles: vec!["x".to_string(); ROOM_TILES],
        ..plain_room(Vec::new(), Vec::new())
    };
    assert_eq!(invalid.tile_at(0, 0), TileKind::Air);
}

// ---------------------------------------------------------------- 读写与版本

#[test]
fn read_parses_example_level_from_docs() {
    let level = read(EXAMPLE_LEVEL_RON).expect("docs/03 §3.6 的示例必须能被 read 解析");

    assert_eq!(level.version, VERSION);
    assert_eq!(level.name, "world_01");
    assert_eq!(level.grid, Grid { cols: 2, rows: 1 });
    assert_eq!(level.start, RoomCoord { col: 0, row: 0 });
    assert_eq!(level.rooms.len(), 2);

    let start_room = &level.rooms[0];
    assert_eq!(start_room.name.as_deref(), Some("start"));
    assert_eq!(start_room.reward, None);
    assert_eq!(start_room.spawn, None);
    assert_eq!(start_room.goals, vec![Goal::ReachExit]);
    assert_eq!(start_room.tiles.len(), ROOM_TILES);
    assert_eq!(start_room.tiles[10].chars().count(), 17, "示例的故意错误");
    assert_eq!(start_room.entities.len(), 2);

    let fruit_room = &level.rooms[1];
    assert_eq!(fruit_room.name.as_deref(), Some("fruit_room"));
    assert_eq!(fruit_room.reward.as_deref(), Some("straw_hat"));
    assert_eq!(
        fruit_room.goals,
        vec![Goal::ReachExit, Goal::Collect { count: 2 }]
    );
    assert_eq!(fruit_room.entities.len(), 4);
    assert_eq!(fruit_room.entities[1].at, (208, 432));

    // 往返：示例即使带 V5 错误也必须能无损写回、读回
    assert_eq!(read(&write(&level)), Ok(level));
}

#[test]
fn example_level_only_reports_v5_on_the_17_char_line() {
    let level = read(EXAMPLE_LEVEL_RON).expect("示例必须可解析");
    let issues = validate(&level);

    let v5 = issues_of(&level, "V5");
    assert_eq!(v5.len(), 1, "示例只应有一条 V5：{issues:?}");
    assert_eq!(v5[0].severity, Severity::Fatal);
    assert_eq!(v5[0].path, "rooms[0].tiles[10]");
    assert_eq!(v5[0].message, "V5 rooms[0].tiles[10]: 行长度 17，期望 16");

    // 除那一条 V5 外，示例可通过全部 Error 规则（docs/03 §3.6 的断言）
    let fatal_or_error: Vec<&LevelIssue> = issues
        .iter()
        .filter(|issue| issue.severity != Severity::Warning)
        .collect();
    assert_eq!(
        fatal_or_error.len(),
        1,
        "示例除 V5 外不得有 Fatal/Error：{issues:?}"
    );
    assert!(issues.iter().all(|issue| issue.severity != Severity::Error));

    // 实体 AABB 均不落在 Solid 瓦片上（docs/03 §3.6 的断言）
    assert!(
        issues_of(&level, "V17").is_empty(),
        "示例不应产生 V17：{issues:?}"
    );

    // 唯一剩下的告警来自 rooms[1] 比 rooms[0] 高一格的地面在接缝处形成的单像素墙
    let v18 = issues_of(&level, "V18");
    assert_eq!(v18.len(), 1, "示例的接缝台阶应产生 1 条 V18：{issues:?}");
    assert_eq!(v18[0].severity, Severity::Warning);
    assert_eq!(v18[0].path, "rooms[1].tiles[14]");

    for issue in &issues {
        assert!(
            issue.message.starts_with("V5 ") || issue.message.starts_with("V18 "),
            "示例出现意料外的问题：{issue}"
        );
    }
}

#[test]
fn write_read_round_trip() {
    // 边界值也要能无损往返：空 goals/entities、0 起点、最小 size、空 id
    let mut sparse = valid_level();
    sparse.rooms[0].name = Some("edge".to_string());
    sparse.rooms[0].goals.clear();
    sparse.rooms[0].reward = Some("hat".to_string());
    sparse.rooms[0].spawn = Some((0, 0));
    sparse.rooms[0].entities = vec![EntitySpec {
        kind: "player_spawn".to_string(),
        at: (0, 0),
        size: Some((1, 1)),
        id: Some(String::new()),
    }];

    for level in [
        valid_level(),
        valid_level_2x1(),
        valid_level_1x2(),
        featureful_level(),
        sparse,
        read(EXAMPLE_LEVEL_RON).expect("示例必须可解析"),
    ] {
        let text = write(&level);
        let parsed = read(&text).expect("write 的输出必须能被 read 解析");
        assert_eq!(parsed, level, "往返必须无损");
        assert_eq!(write(&parsed), text, "write 必须确定性可复现");
    }
}

#[test]
fn write_uses_four_space_indent_and_lf() {
    let text = write(&featureful_level());
    assert!(
        text.contains("\n    version: 2,"),
        "顶层字段应为 4 空格缩进：{text}"
    );
    assert!(!text.contains('\r'), "必须使用 LF 换行：{text}");
    assert!(!text.contains("/*"), "不得枚举数组下标：{text}");
    assert!(text.contains("goals: [\n"), "数组应逐项换行：{text}");
    assert!(
        text.contains("    name: \"featureful\","),
        "字符串按双引号输出：{text}"
    );
}

#[test]
fn read_rejects_lenient_input() {
    assert!(matches!(read(""), Err(LevelError::Parse(_))), "空文本");

    // version 是首个字段，缺省即非法
    let no_version = r#"(
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [],
    )"#;
    assert!(
        matches!(read(no_version), Err(LevelError::Parse(_))),
        "缺 version"
    );

    // 类型不符
    let bad_type = r#"(
        version: "1",
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [],
    )"#;
    assert!(
        matches!(read(bad_type), Err(LevelError::Parse(_))),
        "version 类型不符"
    );

    // 缺非 Option 字段
    let no_rooms = r#"(version: 1, name: "n", grid: (cols: 1, rows: 1), start: (col: 0, row: 0))"#;
    assert!(
        matches!(read(no_rooms), Err(LevelError::Parse(_))),
        "缺 rooms"
    );

    let no_tiles = r#"(
        version: 1,
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(goals: [ReachExit], entities: [])],
    )"#;
    assert!(
        matches!(read(no_tiles), Err(LevelError::Parse(_))),
        "缺 tiles"
    );

    // entities / goals 同样是契约字段，不是「可选」
    let no_entities = r#"(
        version: 1,
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(tiles: [], goals: [ReachExit])],
    )"#;
    assert!(
        matches!(read(no_entities), Err(LevelError::Parse(_))),
        "缺 entities"
    );

    let no_goals = r#"(
        version: 1,
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(tiles: [], entities: [])],
    )"#;
    assert!(
        matches!(read(no_goals), Err(LevelError::Parse(_))),
        "缺 goals"
    );

    // 未知 Goal 变体
    let bad_goal = r#"(
        version: 1,
        name: "n",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(tiles: [], goals: [Teleport], entities: [])],
    )"#;
    assert!(
        matches!(read(bad_goal), Err(LevelError::Parse(_))),
        "未知 Goal 变体"
    );

    // 拼错的字段名不得被静默忽略（字段名是契约）
    let typo = write(&featureful_level()).replace("reward", "rewrd");
    assert_ne!(typo, write(&featureful_level()));
    assert!(
        matches!(read(&typo), Err(LevelError::Parse(_))),
        "拼错字段名"
    );

    // 解析失败优先于版本检查：这份文本同时「版本未知」且「结构不合法」
    assert!(matches!(read("(version: 2)"), Err(LevelError::Parse(_))));

    // 尾随垃圾不得被忽略
    let trailing = format!("{} 垃圾", write(&valid_level()));
    assert!(
        matches!(read(&trailing), Err(LevelError::Parse(_))),
        "尾随垃圾"
    );

    // read 只做解析 + 版本检查；语义违规由 validate 负责（此处 V9/V10 违规但 read 成功）
    let mut level = valid_level();
    level.rooms[0].goals = vec![Goal::Collect { count: 9 }];
    assert_eq!(read(&write(&level)), Ok(level));
}

#[test]
fn read_rejects_unsupported_version() {
    let mut level = valid_level();
    level.version = 99;
    let error = read(&write(&level)).unwrap_err();
    assert_eq!(
        error,
        LevelError::UnsupportedVersion {
            found: 99,
            expected: VERSION
        }
    );
    assert_eq!(
        error.to_string(),
        format!("文件版本 99，本引擎支持 {VERSION}")
    );

    let text =
        r#"(version: 7, name: "n", grid: (cols: 1, rows: 1), start: (col: 0, row: 0), rooms: [])"#;
    let error = read(text).unwrap_err();
    assert!(matches!(
        error,
        LevelError::UnsupportedVersion {
            found: 7,
            expected
        } if expected == VERSION
    ));
    assert!(
        error
            .to_string()
            .contains(&format!("文件版本 7，本引擎支持 {VERSION}"))
    );
}

#[test]
fn option_fields_default_to_none() {
    let tiles = vec!["\"................\""; ROOM_TILES].join(", ");
    let text = format!(
        r#"(
            version: 1,
            name: "omitted",
            grid: (cols: 1, rows: 1),
            start: (col: 0, row: 0),
            rooms: [(
                tiles: [{tiles}],
                goals: [CollectAll],
                entities: [(kind: "fruit", at: (64, 464))],
            )],
        )"#
    );
    let level = read(&text).expect("Option 字段缺省必须按 None 解析");
    let room = &level.rooms[0];
    assert_eq!(room.name, None);
    assert_eq!(room.reward, None);
    assert_eq!(room.spawn, None);
    assert_eq!(room.entities[0].size, None);
    assert_eq!(room.entities[0].id, None);
    assert_eq!(room.entities[0].size_or_default(), (32, 32));
    assert_eq!(room.goals, vec![Goal::CollectAll]);
    assert_eq!(level.rooms.len(), 1);
}

#[test]
fn option_fields_parse_from_explicit_some() {
    let tiles = vec!["\"................\""; ROOM_TILES].join(", ");
    let text = format!(
        r#"(
            version: 1,
            name: "explicit",
            grid: (cols: 1, rows: 1),
            start: (col: 0, row: 0),
            rooms: [(
                name: Some("home"),
                reward: Some("straw_hat"),
                spawn: Some((256, 256)),
                tiles: [{tiles}],
                goals: [ReachExit, CollectAll, Collect(count: 3)],
                entities: [
                    (kind: "player_spawn", at: (64, 464), size: Some((16, 24)), id: Some("spawn")),
                    (kind: "exit", at: (448, 464), size: None, id: None),
                ],
            )],
        )"#
    );
    let level = read(&text).expect("显式 Some 必须解析");
    let room = &level.rooms[0];
    assert_eq!(room.name.as_deref(), Some("home"));
    assert_eq!(room.reward.as_deref(), Some("straw_hat"));
    assert_eq!(room.spawn, Some((256, 256)));
    assert_eq!(
        room.goals,
        vec![
            Goal::ReachExit,
            Goal::CollectAll,
            Goal::Collect { count: 3 }
        ]
    );
    assert_eq!(room.entities[0].size, Some((16, 24)));
    assert_eq!(room.entities[0].id.as_deref(), Some("spawn"));
    assert_eq!(room.entities[0].size_or_default(), (16, 24));
    assert_eq!(room.entities[1].size_or_default(), (32, 32));
}

#[test]
fn name_matches_file_exact_segment() {
    let level = valid_level();
    assert!(name_matches_file(&level, "test_level"));
    assert!(!name_matches_file(&level, "world_01"));
    assert!(!name_matches_file(&level, "test_level.level"));
    assert!(!name_matches_file(&level, ""));

    let example = read(EXAMPLE_LEVEL_RON).expect("示例必须可解析");
    assert!(name_matches_file(&example, "world_01"));
    assert!(!name_matches_file(&example, "world_02"));

    // validate 只看字符集，不看文件名
    let mut level = valid_level();
    level.name = "world_01".to_string();
    assert!(issues_of(&level, "V2").is_empty());
    assert!(!name_matches_file(&level, "world_02"));
}

// ---------------------------------------------------------------- 校验规则 V1–V18

#[test]
fn validate_accepts_valid_levels() {
    for (label, level) in [
        ("1x1", valid_level()),
        ("2x1", valid_level_2x1()),
        ("1x2", valid_level_1x2()),
        ("2x2 featureful", featureful_level()),
    ] {
        let issues = validate(&level);
        assert!(issues.is_empty(), "{label} 合法关卡不应有问题：{issues:?}");
    }
}

#[test]
fn validate_returns_all_issues_without_short_circuit() {
    let mut level = valid_level();
    level.name = "Bad Name".to_string();
    level.rooms[0].goals.clear();
    level.rooms[0].entities.clear();
    level.rooms[0].entities.push(entity("enemy", (96, 464)));

    let issues = validate(&level);
    // 空 goals 不再是问题（V7 已移除），只要求其余规则不被短路
    assert!(issues_of(&level, "V7").is_empty());
    for rule in ["V2", "V12", "V14"] {
        assert!(
            !issues_of(&level, rule).is_empty(),
            "{rule} 未被报告：{issues:?}"
        );
    }
    assert!(issues.len() >= 3, "必须一次返回全部问题：{issues:?}");
}

#[test]
fn validate_tolerates_malformed_tiles() {
    let mut level = valid_level();
    level.rooms[0].tiles.clear();
    let issues = validate(&level);
    assert_eq!(issues_of(&level, "V5").len(), 1);
    assert!(!issues.is_empty());

    let mut level = valid_level();
    level.rooms[0].tiles = vec!["################".to_string(); 32];
    assert!(!validate(&level).is_empty(), "行数过多同样应报 V5");
}

#[test]
fn v1_version_must_be_current() {
    assert!(issues_of(&valid_level(), "V1").is_empty());

    let mut level = valid_level();
    level.version = 1;
    let issues = issues_of(&level, "V1");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Fatal);
    assert_eq!(issues[0].path, "version");
    assert!(
        issues[0].message.contains("要求 version == 2"),
        "{}",
        issues[0].message
    );
}

#[test]
fn v2_name_charset() {
    assert!(issues_of(&valid_level(), "V2").is_empty());

    let mut level = valid_level();
    level.name = "world_01".to_string();
    assert!(issues_of(&level, "V2").is_empty());
    level.name = "a1_".to_string();
    assert!(issues_of(&level, "V2").is_empty());

    for bad in ["", "World", "world-01", "world 01", "world.01", "世界"] {
        let mut level = valid_level();
        level.name = bad.to_string();
        let issues = issues_of(&level, "V2");
        assert_eq!(issues.len(), 1, "'{bad}' 应触发 V2");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].path, "name");
        assert!(issues[0].message.contains("不匹配 ^[a-z0-9_]+$"));
    }
}

#[test]
fn v3_grid_bounds() {
    assert!(issues_of(&valid_level(), "V3").is_empty());

    // 正例边界：16×16 = 256 恰好合法
    let mut level = valid_level();
    level.grid = Grid { cols: 16, rows: 16 };
    assert!(issues_of(&level, "V3").is_empty());

    let mut level = valid_level();
    level.grid = Grid { cols: 0, rows: 1 };
    let issues = issues_of(&level, "V3");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "grid");
    assert!(issues[0].message.contains("cols >= 1（实际 0）"));

    let mut level = valid_level();
    level.grid = Grid { cols: 1, rows: 0 };
    let issues = issues_of(&level, "V3");
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("rows >= 1（实际 0）"));

    // 反例：17×16 = 272 > 256
    let mut level = valid_level();
    level.grid = Grid { cols: 17, rows: 16 };
    let issues = issues_of(&level, "V3");
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("cols * rows <= 256（实际 272）"));
}

#[test]
fn v3b_room_count_warning() {
    assert!(issues_of(&valid_level(), "V3b").is_empty());

    // 正例边界：64 不告警
    let mut level = valid_level();
    level.grid = Grid { cols: 8, rows: 8 };
    assert!(issues_of(&level, "V3b").is_empty());

    // 反例：65 > 64，仅告警
    let mut level = valid_level();
    level.grid = Grid { cols: 13, rows: 5 };
    let issues = issues_of(&level, "V3b");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Warning);
    assert_eq!(issues[0].path, "grid");
    assert!(issues[0].message.contains("房间总数 65 > 64"));
}

#[test]
fn v4_room_count_matches_grid() {
    assert!(issues_of(&valid_level(), "V4").is_empty());

    let mut level = valid_level();
    level.rooms.push(plain_room(
        vec![Goal::ReachExit],
        vec![entity("exit", (448, 464))],
    ));
    let issues = issues_of(&level, "V4");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms");
    assert!(issues[0].message.contains("房间数 2 与 grid 1x1 = 1 不符"));

    // 反例：房间少于网格
    let mut level = valid_level_2x1();
    level.rooms.pop();
    assert_eq!(issues_of(&level, "V4").len(), 1);
}

#[test]
fn v5_tile_rows_and_line_length() {
    assert!(issues_of(&valid_level(), "V5").is_empty());

    // 反例：行数不足
    let mut level = valid_level();
    level.rooms[0].tiles.pop();
    let issues = issues_of(&level, "V5");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Fatal);
    assert_eq!(issues[0].path, "rooms[0].tiles");
    assert!(issues[0].message.contains("行数 15，期望 16"));

    // 反例：某行 17 个字符（docs/03 §3.6 的故意错误）
    let mut level = valid_level();
    level.rooms[0].tiles[10] = ".".repeat(17);
    let issues = issues_of(&level, "V5");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Fatal);
    assert_eq!(issues[0].path, "rooms[0].tiles[10]");
    assert_eq!(
        issues[0].message,
        "V5 rooms[0].tiles[10]: 行长度 17，期望 16"
    );

    // 反例：某行 15 个字符
    let mut level = valid_level();
    level.rooms[0].tiles[3] = ".".repeat(15);
    let issues = issues_of(&level, "V5");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "rooms[0].tiles[3]");
    assert!(issues[0].message.contains("行长度 15，期望 16"));

    // 行长度按 char 计（多字节字符不能按字节数蒙混过关）
    let mut level = valid_level();
    level.rooms[0].tiles[0] = format!("{}{}", "．".repeat(8), ".".repeat(8));
    assert_eq!(level.rooms[0].tiles[0].chars().count(), 16);
    let issues = issues_of(&level, "V5");
    assert!(issues.is_empty(), "16 个 char 即合法：{issues:?}");
    assert_eq!(issues_of(&level, "V6").len(), 8, "全角句点不在 legend 中");
}

#[test]
fn v6_chars_must_be_in_legend() {
    assert!(issues_of(&valid_level(), "V6").is_empty());

    let mut level = valid_level();
    level.rooms[0].tiles[0] = "x...............".to_string();
    let issues = issues_of(&level, "V6");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[0].tiles[0]");
    assert!(issues[0].message.contains("不在调色板中"));

    // 同一行的两个非法字符 → 两条问题，且都不影响 V5
    let mut level = valid_level();
    level.rooms[0].tiles[0] = "xy..............".to_string();
    assert_eq!(issues_of(&level, "V6").len(), 2);
    assert!(issues_of(&level, "V5").is_empty());

    // 正例：全部 5 种 legend 字符都合法
    let mut level = valid_level();
    level.rooms[0].tiles[0] = ".#=^*...........".to_string();
    assert!(issues_of(&level, "V6").is_empty());
}

/// 「每房至少一个目标」是**设计约束**，不由代码强制：空 goals 必须能通过校验。
#[test]
fn empty_goals_are_allowed_by_validation() {
    let mut level = valid_level();
    level.rooms[0].goals.clear();
    let issues = validate(&level);
    assert!(issues.is_empty(), "空 goals 不应再报错：{issues:?}");
    assert!(issues_of(&level, "V7").is_empty(), "V7 已移除且不得复用");

    // 正例：单一非 ReachExit 条件仍然合法
    let mut level = valid_level();
    level.rooms[0].goals = vec![Goal::CollectAll];
    level.rooms[0].entities.push(entity("fruit", (96, 464)));
    assert!(validate(&level).is_empty());
}

#[test]
fn v8_reach_exit_needs_exactly_one_exit() {
    assert!(issues_of(&valid_level(), "V8").is_empty());

    let mut level = valid_level();
    level.rooms[0].entities.retain(|spec| spec.kind != "exit");
    let issues = issues_of(&level, "V8");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[0].entities");
    assert!(
        issues[0]
            .message
            .contains("要求恰好 1 个 exit 实体，实际 0 个")
    );

    let mut level = valid_level();
    level.rooms[0].entities.push(entity("exit", (320, 464)));
    let issues = issues_of(&level, "V8");
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("实际 2 个"));

    // 正例：没有 ReachExit 时不要求 exit
    let mut level = valid_level();
    level.rooms[0].goals = vec![Goal::CollectAll];
    level.rooms[0].entities.retain(|spec| spec.kind != "exit");
    level.rooms[0].entities.push(entity("fruit", (96, 464)));
    assert!(issues_of(&level, "V8").is_empty());
}

#[test]
fn v9_collect_all_needs_at_least_one_fruit() {
    let mut level = valid_level();
    level.rooms[0].goals = vec![Goal::CollectAll];
    let issues = issues_of(&level, "V9");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[0].entities");
    assert!(
        issues[0]
            .message
            .contains("至少 1 个 fruit 实体，实际 0 个")
    );

    // 正例
    level.rooms[0].entities.push(entity("fruit", (96, 464)));
    assert!(issues_of(&level, "V9").is_empty());
    assert!(
        validate(&level)
            .iter()
            .all(|issue| issue.severity != Severity::Error)
    );
}

#[test]
fn v10_collect_count_within_fruit_count() {
    // 正例：count == fruit 数
    assert!(issues_of(&level_with_fruits(2), "V10").is_empty());
    // 正例：count < fruit 数
    let mut level = level_with_fruits(3);
    level.rooms[0].goals = vec![Goal::ReachExit, Goal::Collect { count: 2 }];
    assert!(issues_of(&level, "V10").is_empty());
    // 正例：三房间示例的形状（3 个 fruit、要求 2）
    let example = read(EXAMPLE_LEVEL_RON).expect("示例必须可解析");
    assert!(issues_of(&example, "V10").is_empty());

    // 反例：count 超过 fruit 数
    let mut level = level_with_fruits(2);
    level.rooms[0].goals = vec![Goal::ReachExit, Goal::Collect { count: 3 }];
    let issues = issues_of(&level, "V10");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[0].goals[1].count");
    assert_eq!(
        issues[0].message,
        "V10 rooms[0].goals[1].count: 要求 3 个果实，房间内只有 2 个"
    );

    // 反例：count == 0
    let mut level = level_with_fruits(2);
    level.rooms[0].goals = vec![Goal::ReachExit, Goal::Collect { count: 0 }];
    let issues = issues_of(&level, "V10");
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("count 必须 >= 1"));
}

#[test]
fn v11_reward_charset_and_uniqueness() {
    assert!(issues_of(&valid_level(), "V11").is_empty());

    // 正例：合法且全关卡唯一
    let mut level = valid_level_2x1();
    level.rooms[0].reward = Some("straw_hat".to_string());
    level.rooms[1].reward = Some("scarf".to_string());
    assert!(issues_of(&level, "V11").is_empty());

    // 反例：字符集
    let mut level = valid_level_2x1();
    level.rooms[0].reward = Some("Straw Hat".to_string());
    let issues = issues_of(&level, "V11");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[0].reward");
    assert!(issues[0].message.contains("不匹配 ^[a-z0-9_]+$"));

    // 反例：重复
    let mut level = valid_level_2x1();
    level.rooms[0].reward = Some("straw_hat".to_string());
    level.rooms[1].reward = Some("straw_hat".to_string());
    let issues = issues_of(&level, "V11");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "rooms[1].reward");
    assert!(issues[0].message.contains("与 rooms[0].reward 重复"));

    // 反例：三个房间的第三个重复只报一次
    let mut level = featureful_level();
    level.rooms[3].reward = Some("straw_hat".to_string());
    let issues = issues_of(&level, "V11");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "rooms[3].reward");
}

#[test]
fn v12_exactly_one_player_spawn_in_start_room() {
    assert!(issues_of(&valid_level(), "V12").is_empty());
    assert!(issues_of(&valid_level_2x1(), "V12").is_empty());
    assert!(issues_of(&featureful_level(), "V12").is_empty());

    // 反例：没有 player_spawn
    let mut level = valid_level();
    level.rooms[0]
        .entities
        .retain(|spec| spec.kind != "player_spawn");
    let issues = issues_of(&level, "V12");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms");
    assert!(
        issues[0]
            .message
            .contains("player_spawn 实体数 0，要求恰好 1 个")
    );

    // 反例：两个 player_spawn
    let mut level = valid_level();
    level.rooms[0]
        .entities
        .push(entity("player_spawn", (96, 464)));
    let issues = issues_of(&level, "V12");
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("player_spawn 实体数 2"));

    // 正例：player_spawn 可以在任意房间（不限于 start 房间）
    let mut level = valid_level_2x1();
    level.rooms[0]
        .entities
        .retain(|spec| spec.kind != "player_spawn");
    level.rooms[1]
        .entities
        .push(entity("player_spawn", (64, 464)));
    assert!(
        issues_of(&level, "V12").is_empty(),
        "出生点允许放在任意房间"
    );

    // 正例：start 在非零角落，只要全关恰好一个出生点即可
    let mut level = featureful_level();
    level.start = RoomCoord { col: 0, row: 1 };
    assert!(issues_of(&level, "V12").is_empty());
}

#[test]
fn v13_entity_at_and_size_bounds() {
    assert!(issues_of(&valid_level(), "V13").is_empty());

    // 正例边界：at = ROOM_PX - 1、size = ROOM_PX
    let mut level = valid_level();
    level.rooms[0].entities[1].at = (ROOM_PX - 1, ROOM_PX - 1);
    level.rooms[0].entities[1].size = Some((ROOM_PX, ROOM_PX));
    assert!(issues_of(&level, "V13").is_empty());

    let bad_ats = [
        ((ROOM_PX, 0), "x 上界"),
        ((-1, 0), "x 为负"),
        ((0, ROOM_PX), "y 上界"),
        ((0, -1), "y 为负"),
    ];
    for (at, label) in bad_ats {
        let mut level = valid_level();
        level.rooms[0].entities[1].at = at;
        let issues = issues_of(&level, "V13");
        assert_eq!(issues.len(), 1, "at {at:?}（{label}）应触发 V13");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].path, "rooms[0].entities[1].at");
    }

    for bad in [(0, 32), (ROOM_PX + 1, 32), (32, 0), (32, ROOM_PX + 1)] {
        let mut level = valid_level();
        level.rooms[0].entities[1].size = Some(bad);
        let issues = issues_of(&level, "V13");
        assert_eq!(issues.len(), 1, "size {bad:?} 应触发 V13");
        assert_eq!(issues[0].path, "rooms[0].entities[1].size");
    }
}

#[test]
fn v14_kind_must_be_registered() {
    // 正例：v1 注册表内的 kind
    let mut level = valid_level();
    level.rooms[0].entities.push(entity("fruit", (96, 464)));
    level.rooms[0]
        .entities
        .push(entity("checkpoint", (128, 464)));
    assert!(issues_of(&level, "V14").is_empty());
    assert!(issues_of(&featureful_level(), "V14").is_empty());

    // 反例：Roadmap 里预留、但 v1 会被拒绝的 kind，以及任意未知 kind
    for kind in ["enemy", "switch", "moving_platform", "Player", ""] {
        let mut level = valid_level();
        level.rooms[0].entities.push(entity(kind, (96, 464)));
        let issues = issues_of(&level, "V14");
        assert_eq!(issues.len(), 1, "'{kind}' 应触发 V14");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].path, "rooms[0].entities[2].kind");
        assert!(
            issues[0]
                .message
                .contains(&format!("未知实体 kind '{kind}'"))
        );
        assert!(
            issues[0]
                .message
                .contains("player_spawn / exit / fruit / checkpoint")
        );
    }
}

#[test]
fn v15_room_spawn_bounds() {
    assert!(issues_of(&valid_level(), "V15").is_empty());

    let mut level = valid_level();
    level.rooms[0].spawn = Some((256, 256));
    assert!(issues_of(&level, "V15").is_empty());

    for bad in [(ROOM_PX, 0), (-1, 0), (0, ROOM_PX), (0, -1)] {
        let mut level = valid_level();
        level.rooms[0].spawn = Some(bad);
        let issues = issues_of(&level, "V15");
        assert_eq!(issues.len(), 1, "spawn {bad:?} 应触发 V15");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].path, "rooms[0].spawn");
    }
}

#[test]
fn v16_room_names_unique_and_start_in_grid() {
    assert!(issues_of(&valid_level(), "V16").is_empty());
    assert!(issues_of(&valid_level_2x1(), "V16").is_empty());
    assert!(issues_of(&featureful_level(), "V16").is_empty());

    // 正例：不同房间名
    let mut level = valid_level_2x1();
    level.rooms[0].name = Some("start".to_string());
    level.rooms[1].name = Some("fruit_room".to_string());
    assert!(issues_of(&level, "V16").is_empty());

    // 反例：房间名重复
    let mut level = valid_level_2x1();
    level.rooms[0].name = Some("dup".to_string());
    level.rooms[1].name = Some("dup".to_string());
    let issues = issues_of(&level, "V16");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Error);
    assert_eq!(issues[0].path, "rooms[1].name");
    assert!(issues[0].message.contains("与 rooms[0].name 重复"));

    // 反例：start 越界
    let mut level = valid_level();
    level.start = RoomCoord { col: 1, row: 0 };
    let issues = issues_of(&level, "V16");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "start");
    assert!(issues[0].message.contains("起始房间 (1, 0) 超出 grid 1x1"));
}

#[test]
fn v17_entity_overlapping_solid_is_warning() {
    assert!(issues_of(&valid_level(), "V17").is_empty());
    assert!(issues_of(&featureful_level(), "V17").is_empty());
    assert!(issues_of(&read(EXAMPLE_LEVEL_RON).expect("示例必须可解析"), "V17").is_empty());

    // 正例边界：AABB 下沿正好贴在地面上沿（y 上界 == 480）不算重叠
    let mut level = valid_level();
    level.rooms[0].entities[1].at = (64, 464);
    assert!(issues_of(&level, "V17").is_empty());

    // 反例：下移 1 像素即压进地面
    let mut level = valid_level();
    level.rooms[0].entities[1].at = (64, 465);
    let issues = issues_of(&level, "V17");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Warning);
    assert_eq!(issues[0].path, "rooms[0].entities[1].at");
    assert!(issues[0].message.contains("与 Solid 瓦片重叠"));

    // 反例：size 变大后压进地面
    let mut level = valid_level();
    level.rooms[0].entities[1].size = Some((32, 64));
    let issues = issues_of(&level, "V17");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Warning);

    // 反例：天花板
    let mut level = valid_level();
    level.rooms[0].tiles[0] = "#".repeat(ROOM_TILES);
    level.rooms[0].entities[1].at = (448, 16);
    let issues = issues_of(&level, "V17");
    assert_eq!(issues.len(), 1);
}

#[test]
fn v18_room_boundary_discontinuity_is_warning() {
    assert!(issues_of(&valid_level_2x1(), "V18").is_empty());
    assert!(issues_of(&valid_level_1x2(), "V18").is_empty());
    assert!(issues_of(&featureful_level(), "V18").is_empty());

    // 反例：水平接缝——右房间左边缘多一格实心，形成单像素墙
    let mut level = valid_level_2x1();
    level.rooms[1].tiles[14] = "#...............".to_string();
    let issues = issues_of(&level, "V18");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Warning);
    assert_eq!(issues[0].path, "rooms[1].tiles[14]");
    assert!(issues[0].message.contains("与左邻 rooms[0] 的边界不连续"));

    // 正例：非 Solid 瓦片（Spike）贴在接缝上不构成单像素墙
    let mut level = valid_level_2x1();
    level.rooms[1].tiles[14] = "^...............".to_string();
    assert!(issues_of(&level, "V18").is_empty());

    // 反例：垂直接缝——下房间顶行整行空气，与上方地面整行不连续
    let mut level = valid_level_1x2();
    level.rooms[1].tiles[0] = "................".to_string();
    let issues = issues_of(&level, "V18");
    assert_eq!(issues.len(), ROOM_TILES, "每一列都是一处不连续：{issues:?}");
    assert_eq!(issues[0].path, "rooms[1].tiles[0]");
    assert!(
        issues[0]
            .message
            .contains("与上方 rooms[0] 的边界在第 0 列不连续")
    );

    // 反例：垂直接缝只缺一格 → 只有该列报
    let mut level = valid_level_1x2();
    level.rooms[1].tiles[0] = "########.#######".to_string();
    let issues = issues_of(&level, "V18");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "rooms[1].tiles[0]");
    assert!(issues[0].message.contains("第 8 列不连续"));

    // 正例：接缝两侧都实心（连续地形）不报
    let mut level = valid_level_2x1();
    level.rooms[0].tiles[14] = "...............#".to_string();
    level.rooms[1].tiles[14] = "#...............".to_string();
    assert!(issues_of(&level, "V18").is_empty());
}

// ---------- v2：调色板 + 迁移（`03` §3.9）----------

#[test]
fn v1_files_migrate_to_v2_with_builtin_palette() {
    let text = r#"(
        version: 1,
        name: "w",
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(
            name: None, reward: None, spawn: None, goals: [ ReachExit ],
            tiles: ["................", "................", "................", "................",
                    "................", "................", "................", "................",
                    "................", "................", "................", "................",
                    "................", "................", "................", "PLACEHOLDER"],
            entities: [
                (kind: "player_spawn", at: (64, 464)),
                (kind: "exit", at: (448, 464)),
            ],
        )],
    )"#;
    let text = text.replace("PLACEHOLDER", "#".repeat(16).as_str());
    let level = level_format::read(&text).expect("v1 必须能读");
    assert_eq!(level.version, level_format::VERSION);
    assert!(level.tileset.is_none());
    assert_eq!(level.tileset_columns, 8);
    assert_eq!(level.palette(), level_format::Level::builtin_palette());
    assert_eq!(level.behavior_of('#'), Some(level_format::Behavior::Solid));
    assert_eq!(level.cell_of('^'), Some(2));
}

#[test]
fn v2_custom_palette_resolves_behavior_and_cell() {
    let text = r#"(
        version: 2,
        name: "w",
        tileset: Some("sprites/tiles/my_sheet.png"),
        tileset_columns: 4,
        palette: [
            (char: 'g', name: "grass", behavior: None, cell: 7),
            (char: 'S', name: "stone", behavior: Solid, cell: 1),
        ],
        grid: (cols: 1, rows: 1),
        start: (col: 0, row: 0),
        rooms: [(
            name: None, reward: None, spawn: None, goals: [ ReachExit ],
            tiles: ["................", "................", "................", "................",
                    "................", "................", "................", "................",
                    "................", "................", "................", "................",
                    "................", "................", "......gS........", "PLACEHOLDER"],
            entities: [
                (kind: "player_spawn", at: (64, 464)),
                (kind: "exit", at: (448, 464)),
            ],
        )],
    )"#;
    let text = text.replace("PLACEHOLDER", "#".repeat(16).as_str());
    let level = level_format::read(&text).expect("v2 必须能读");
    assert_eq!(level.tileset.as_deref(), Some("sprites/tiles/my_sheet.png"));
    assert_eq!(level.tileset_columns, 4);
    assert_eq!(level.behavior_of('g'), Some(level_format::Behavior::None));
    assert_eq!(level.cell_of('S'), Some(1));
    // 自定义字符被调色板接受（V6 的调色板分支由 v6 用例覆盖）
    assert_eq!(level.behavior_of('S'), Some(level_format::Behavior::Solid));
}

#[test]
fn v19_rejects_bad_palette_entries() {
    let mut level = level_format::Level {
        version: level_format::VERSION,
        name: "w".into(),
        tileset: None,
        tileset_columns: 8,
        palette: vec![
            level_format::PaletteEntry {
                char: '.',
                name: "air".into(),
                behavior: level_format::Behavior::None,
                cell: 0,
            },
            level_format::PaletteEntry {
                char: 'x',
                name: "".into(),
                behavior: level_format::Behavior::Solid,
                cell: 1,
            },
            level_format::PaletteEntry {
                char: 'x',
                name: "dup".into(),
                behavior: level_format::Behavior::Solid,
                cell: 2,
            },
        ],
        background: None,
        grid: level_format::Grid { cols: 1, rows: 1 },
        start: level_format::RoomCoord { col: 0, row: 0 },
        rooms: vec![level_format::Room {
            name: None,
            reward: None,
            spawn: None,
            goals: vec![level_format::Goal::ReachExit],
            tiles: vec![".".repeat(16); 16],
            entities: vec![
                level_format::EntitySpec {
                    kind: "player_spawn".into(),
                    at: (64, 464),
                    size: None,
                    id: None,
                },
                level_format::EntitySpec {
                    kind: "exit".into(),
                    at: (448, 464),
                    size: None,
                    id: None,
                },
            ],
        }],
    };
    for row in 0..16 {
        level.rooms[0].tiles[row] = if row == 15 {
            "#".repeat(16)
        } else {
            ".".repeat(16)
        };
    }
    let issues = level_format::validate(&level);
    let v19: Vec<_> = issues
        .iter()
        .filter(|issue| issue.message.starts_with("V19"))
        .collect();
    assert_eq!(v19.len(), 3, "{issues:?}");
}

#[test]
fn background_defaults_to_none_when_field_missing() {
    // 既有 v1 示例没有 `background` 字段，必须继续可读（加可选字段不升版本）。
    let example = read(EXAMPLE_LEVEL_RON).expect("v1 示例必须可读");
    assert_eq!(example.background, None);

    // v2 写出再删掉该行，模拟旧文件。
    let text = write(&valid_level());
    let without = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("background:"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = read(&without).expect("缺少 background 字段必须可读");
    assert_eq!(parsed.background, None);
}

#[test]
fn background_layers_round_trip() {
    let mut level = valid_level();
    level.background = Some(Background {
        layers: vec![
            BackgroundLayer {
                image: "sprites/backgrounds/sky.png".to_string(),
                scroll_x: 10,
                scroll_y: 0,
            },
            BackgroundLayer {
                image: "sprites/backgrounds/hills.png".to_string(),
                scroll_x: 40,
                scroll_y: 20,
            },
        ],
    });
    let text = write(&level);
    assert!(text.contains("background: Some("), "{text}");
    let parsed = read(&text).expect("背景字段必须可读回");
    assert_eq!(parsed, level);
    assert!(validate(&parsed).is_empty(), "{:?}", validate(&parsed));
}

#[test]
fn v20_rejects_bad_background_layers() {
    let mut level = valid_level();
    level.background = Some(Background {
        layers: (0..=MAX_BACKGROUND_LAYERS)
            .map(|index| BackgroundLayer {
                image: if index == 0 {
                    "  ".to_string()
                } else {
                    format!("sprites/backgrounds/{index}.png")
                },
                scroll_x: if index == 1 { 101 } else { 30 },
                scroll_y: if index == 2 { 101 } else { 0 },
            })
            .collect(),
    });
    let v20: Vec<_> = issues_of(&level, "V20");
    assert_eq!(v20.len(), 4, "{v20:?}");
    assert!(v20.iter().any(|issue| issue.message.contains("超过上限")));
    assert!(
        v20.iter()
            .any(|issue| issue.message.contains("image 不能为空"))
    );
    assert_eq!(
        v20.iter()
            .filter(|issue| issue.message.contains("超出 0..=100"))
            .count(),
        2
    );
}
