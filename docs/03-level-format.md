# 03 — 关卡数据格式（`.level.ron`）

> 坐标与网格定义见 `00-conventions.md` §0.3。**本文拥有**：文件 schema、legend、实体 kind 注册表、道具类型枚举、校验规则、读写入口。
> 本文定义的一切字段名都是契约：编辑器（`08`）、运行时（`05`）、美术（`04`）都依赖它们。

## 3.1 文件与读写

| 项 | 约定 |
|---|---|
| 路径 | `assets/levels/<name>.level.ron` |
| 扩展名 | **`ron`**。Bevy 的 loader 按最后一个 `.` 后的扩展名匹配，因此自定义 loader 注册 `extensions() -> &["ron"]`；`*.level.ron` 的双后缀仅为人眼可读 |
| 唯一性约束 | 项目内**只有关卡**使用 `.ron` 资产。若将来引入第二种 RON 资产，必须改为注册专用扩展名（见 `10` 的 loader 取证结果） |
| 解析入口 | `level_format::read(&str) -> Result<Level, LevelError>` |
| 序列化入口 | `level_format::write(&Level) -> String` |
| 序列化格式 | `ron::ser::PrettyConfig` 的具体参数**只允许**在 `level_format::write()` 内设定。任何调用方（编辑器、CLI、测试）必须调用它，**禁止**自行 `ron::to_string` |
| 编码 | UTF-8，LF 换行，无 BOM |
| `name` 一致性 | 文件内 `name` 必须等于文件名的 `<name>` 段，否则校验失败（V2） |

**为什么写死单一 `write()`**：编辑器（`08`）与 CLI（`08`）都要写文件，若各自序列化，会产生无意义的巨大 diff 与"谁写得更规范"的分歧。

**热重载**：debug 构建下保存 `*.level.ron` 即时生效；校验不通过的改动会被拒绝并打印字段级错误，运行中的会话继续使用旧关卡。语义见 `02` §2.3。

## 3.2 顶层结构

```rust
pub struct Level {
    pub version: u32,            // 必须 == 3
    pub name: String,            // snake_case，等于文件名
    pub grid: Grid,              // (cols, rows)
    pub start: RoomCoord,        // 起始房间
    pub tileset: Option<String>, // 自定义贴图（§3.9）
    pub tileset_columns: u32,    // 贴图列数（§3.9）
    pub palette: Vec<PaletteEntry>, // 地块调色板（§3.9）
    pub background: Option<Background>, // 背景视差层（§3.10）；None = 无背景
    pub rooms: Vec<Room>,        // 行主序，len == cols * rows，row 0 = 地图最上一行
}

pub struct Background { pub layers: Vec<BackgroundLayer> }       // 最多 3 层（V20）
pub struct BackgroundLayer {
    pub image: String,        // 相对 assets/ 的 PNG；加载时规范化
    pub scroll_x: u8,         // 0..=100（百分比）：0 = 屏幕固定，100 = 与世界同速
    pub scroll_y: u8,         // 同上
}

pub struct Grid { pub cols: u32, pub rows: u32 }
pub struct RoomCoord { pub col: u32, pub row: u32 }

pub struct Room {
    pub name: Option<String>,    // 可选，便于编辑器与 diff 阅读；全关卡唯一
    pub tiles: Vec<String>,      // 恰好 16 个字符串，每个恰好 16 个 legend 字符
    pub spawn: Option<(i32, i32)>, // 重生点（房间局部坐标，AABB 中心）；None = 用 02 的默认值
    pub entities: Vec<EntitySpec>,
}

pub struct EntitySpec {
    pub kind: String,            // 必须在 §3.4 注册表中
    pub at: (i32, i32),          // 房间局部坐标，AABB 中心，单位像素
    pub size: Option<(i32, i32)>,// 默认 (32, 32)
    pub id: Option<String>,      // 可选稳定标识，供编辑器与后续逻辑引用
    pub item: Option<ItemKind>,  // 仅 item / item_spawner 时有效（V24）
    pub sprite: Option<String>,  // 道具贴图（相对 assets/ 的 PNG）；仅 item 类有效（V25）
}
```

> **v3 破坏性变更**：房间级 `goals` / `reward` 已删除；房间不再有通关目标。通关由**全关卡唯一的 `goal` 实体**触发（§3.4）。格式迁移见 §3.8。

**为什么坐标是 `i32` 而不是 `f32`**：像素风不存在亚像素摆放；整数能消除浮点格式化噪声（diff 干净）并让"是否对齐网格"成为编译期不可表达的错误。

## 3.3 瓦片编码与 legend

`tiles` 是 16 行字符串，**第 0 行 = 房间最上一行**（屏幕视角，见 `00` §0.3）。

| 字符 | 名称 | 语义 | tileset 索引 | 碰撞 |
|---|---|---|---|---|
| `.` | `Air` | 空 | 不渲染 | 无 |
| `#` | `Solid` | 实心地形 | `0` | 实心，四面阻挡 |
| `=` | `OneWay` | 单向平台（**隐藏通道专用**，`01` §1.8） | `1` | 仅当垂直下落且上一帧脚底在平台上沿之上时可站；从下方可穿过。普通平台请用 `#` |
| `^` | `Spike` | 危险 | `2` | 无阻挡；玩家 AABB 相交即死亡 |
| `*` | `Decor` | 无碰撞装饰 | `3` | 无碰撞；渲染在贴花层（Z 见 `00` §0.4） |

- **legend 是固定映射**，不支持按文件自定义。增加字符 = 新增一种 `TileKind` = 一次破坏性格式变更（走 §3.8）。
- **`Air` 不占用图集格**，也不生成任何实体（`07` 的瓦片渲染规模裁剪依赖这一点）。本表中除 `Air` 外的条目按其出现次序即为 `tileset.png` 的行主序索引。
- tileset 图的**物理布局**（网格尺寸、列数、行数算法、总尺寸）归 `04` §10；本表只定义**索引语义**。

## 3.9 v2：关卡自带贴图与地块调色板（新增地块）
> **实现状态（2026-10-18）**：**格式层已落地**——`Behavior`/`PaletteEntry`/`tileset`/`tileset_columns`、
> v1→v2 迁移（v1 是 v2 的子集，读取时补默认值并升版本号）、`Level::palette()/behavior_of()/cell_of()`、
> V6 改用调色板、新增 V19；单测 3 项（迁移、自定义字符解析、V19 三条反例）。
> ✅ **渲染与物理已接入（2026-10-18）**：`RoomTiles` 由关卡调色板构建——`behavior` 决定物理类别
> （`Solid/OneWay/Hazard/None`），`cell` 决定图集格；关卡带 `tileset` 时按 `tileset_columns` 建图集
> （行数 = `ceil((max_cell+1)/columns)`），否则用内置图集。单测覆盖"自定义 `S` 实心可碰撞 + 贴图格正确"。
> ✅ **自定义碰撞范围（2026-11-08）**：`PaletteEntry.collision` 可选字段 + V21/V22；物理按栅格块求解，
> 编辑器面板可画/拖顶点（`08` §8.5.7 B）。示例关卡未使用该字段，读取为 `None`。



> 目标：**新增地块**（新名称 + 新贴图）而**不扩展物理语义**——行为仍只有 4 种（`Solid` / `OneWay` / `Hazard` / `None`）。

```ron
(
    version: 3,
    name: "world_01",
    tileset: Some("sprites/tiles/my_sheet.png"),   // 相对 assets/；32×32 网格。也接受 "assets/..." 写法（加载时统一规范化）
    tileset_columns: 6,                            // 贴图列数；行数 = ceil(cells / columns)。编辑器加载贴图后按宽度自动校正
    palette: [
        (char: '#', name: "stone",  behavior: Solid,  cell: 0),
        (char: '=', name: "wood",   behavior: OneWay, cell: 3),
        (char: '^', name: "spikes", behavior: Hazard, cell: 5,
         // 可选：碰撞范围只占瓦片下半格（瓦片局部像素，左上原点，+Y 向下）
         collision: Some((vertices: [(0, 16), (32, 16), (32, 32), (0, 32)]))),
        (char: 'g', name: "grass",  behavior: None,   cell: 7),
    ],
    grid: (cols: 10, rows: 3),
    start: (col: 0, row: 0),
    rooms: [ /* 同 v1；tiles 字符取自 palette */ ],
)
```

| 规则 | 说明 |
|---|---|
| 版本 | `version: 3`。v1/v2 文件由 `io::read` 自动迁移（补 `tileset: None` / `palette` = 内置 4 条 / `tileset_columns: 8`，删除 `goals`/`reward`，`exit` → `goal`） |
| `palette` 唯一性 | `char` 唯一且不得为 `.`（空气固定）；`name` 非空；`cell` 在贴图范围内（由编辑器/加载器检查，校验器只看结构） |
| `behavior` | 只允许 4 种；**新增行为 = 破坏性变更**，必须同时改 `06` 的物理内核与本节 |
| `collision` | **可选**（`None`/缺省 = 整格 32×32）。`vertices` 是 3..=32 个顶点的**简单多边形**，坐标为瓦片局部像素：原点 = 瓦片左上角、+Y 向下，取值 `0..=32`。同一字符在任意房间共享同一碰撞范围 |
| `collision` 的物理语义 | 物理按**像素中心**把多边形栅格化为轴对齐块（斜边呈像素阶梯，见 `06` §6.4.7 / §6.6.5）；`Solid` 阻挡与可站立、`OneWay` 只在其覆盖范围上可站、`Hazard` 只在其覆盖范围内致死。`None` 行为不使用该字段（V22） |
| 渲染 | 瓦片按 `cell` 从 `tileset` 取图；`tileset: None` ⇒ 用内置 `assets/sprites/tiles/tileset.png` 与内置 palette（等价 v1 外观）。碰撞范围不影响渲染 |
| 校验 | V6 改为"字符 ∈ palette"；**V19**：`palette` 字符唯一、非 `.`、`name` 非空；**V21**：`collision` 结构非法；**V22**：`collision` 实际不生效（Warning） |
| 存档兼容 | `SaveData` 只依赖 `level_name` + `grid`，与 palette 无关 |

## 3.10 背景视差（v2 可选字段）

```ron
(
    version: 3,
    // ...
    background: Some((
        layers: [
            (image: "sprites/backgrounds/sky.png",   scroll_x: 10, scroll_y: 0),
            (image: "sprites/backgrounds/hills.png", scroll_x: 40, scroll_y: 20),
        ],
    )),
)
```

| 规则 | 说明 |
|---|---|
| 缺省 | 字段缺失或 `None`/空层列表 = 无背景：只显示 `z::ROOM_BG` 的房间背景板 |
| 顺序 | `layers[0]` 最远，索引越大越靠近镜头；渲染 Z = `z::PARALLAX + index`（`00` §0.4） |
| 滚动 | `scroll_x/y` 是**屏幕滚动百分比**：`0` = 屏幕固定（无限远），`100` = 与世界同速；物理意义与渲染公式见 `07` §7.13 |
| 贴图 | 相对 `assets/` 的 PNG，按**自身像素尺寸**平铺；建议放 `sprites/backgrounds/` |
| 数量 | `layers.len() <= 3`（V20）；上限常量是 `MAX_BACKGROUND_LAYERS` |
| 非破坏性 | 新增 `Option` 字段（serde 默认值），**不升版本**；旧文件读取后 `background = None` |

## 3.4 实体 kind 注册表（v3）

| `kind` | 默认 `size` | 上限/约束 | 说明 |
|---|---|---|---|
| `player_spawn` | `(32, 32)` | 全关卡**恰好 1 个**，**可以位于任意房间**（V12）；游戏从它所在房间开始 | 玩家初始位置 |
| `goal` | `(32, 32)` | 全关卡**恰好 1 个**（V23） | **终点**：玩家 AABB 与它相交即该玩家到达终点（`01` §1.4） |
| `item` | `(12, 12)` | 每房间 `0..=255` 个；**必须**带 `item` 字段（V24）；`sprite` 可选（V25） | 可拾取道具；拾取后覆盖角色当前道具并**销毁**（`01` §1.5、`06` §6.12） |
| `item_spawner` | `(12, 12)` | 每房间 `0..=255` 个；**必须**带 `item` 字段（V24）；`sprite` 可选（V25） | **道具生成器**：在自身位置生成道具；被拾取后隐藏 1.5 s 再重新生成（`01` §1.5） |
| `fruit` | `(12, 12)` | 每房间 `0..=255` 个 | 可选收集品（当前只播放拾取反馈与计入存档，不参与通关） |
| `checkpoint` | `(32, 32)` | 每房间 `0..=255` 个 | 存档点区域；玩家在内按 `E` 激活并写盘（`02` §2.5）。`at` = 重生时的 AABB 中心，必须摆在地面上 |

Roadmap（**格式保留位，v3 校验会拒绝**）：`enemy`、`switch`、`moving_platform`。

## 3.5 道具类型枚举

```rust
pub enum ItemKind {
    SuperJump,   // 跳得更高（起跳初速倍率）
    Speed,       // 跑得更快（水平最大速度 / 加速度倍率）
    DoubleJump,  // 二段跳（离地后额外起跳一次）
}
```

- 角色**同一时刻至多携带一个道具**；再次拾取直接覆盖，不叠加（`01` §1.5）。
- `item` 实体在拾取后销毁；`item_spawner` 生成的实体拾取后隐藏，`ITEM_RESPAWN_SECS = 1.5` 秒后重新生成。
- 道具贴图：`sprite: Some("sprites/items/speed.png")`（相对 `assets/` 的 PNG）。未设置时按类型用占位色块。
- 死亡**不清空**当前道具；避免一次性道具被消耗后无法继续。
- 具体倍率归 `06` §6.12；这里是 RON 枚举名（`item: Some(SuperJump)`）。

## 3.6 完整示例

以 `assets/levels/world_01.level.ron` 的**形状**为例（`cols: 2, rows: 1`；左房间 = `(0,0)`，右房间 = `(1,0)`）。

> ⚠️ 真实文件与本示例**不完全相同**：示例里有一处**故意的 V5 错误**（用来演示报错格式），
> 且真实关卡的平台位置随后续调整过。示例的作用是展示语法与校验行为，不是关卡的权威副本。

```ron
(
    version: 3,
    name: "world_01",
    grid: (cols: 2, rows: 1),
    start: (col: 0, row: 0),
    rooms: [
        // ---- rooms[0] = (col 0, row 0)，起始房间 ----
        (
            name: Some("start"),
            spawn: None,
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
                (kind: "goal", at: (448, 464)),
            ],
        ),
        // ---- rooms[1] = (col 1, row 0) ----
        (
            name: Some("fruit_room"),
            spawn: None,
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
                (kind: "item", item: Some(DoubleJump), at: (160, 432)),
                (kind: "fruit", at: (208, 432)),
                (kind: "fruit", at: (272, 432)),
                (kind: "fruit", at: (336, 432)),
            ],
        ),
    ],
)
```

> 例中 `rooms[0].tiles[10]` 故意写成 17 个字符，用于演示 V5 报错格式（编号 + 字段路径 + 说明）：
> `V5 rooms[0].tiles[10]: 行长度 17，期望 16`。
> 除该行外，本示例可通过全部 Error 规则；实体 AABB 均不落在 `Solid` 瓦片上，故也不产生 V17 警告。

### grid 索引语义（以 `10 × 3` 为例）

| `rooms` 下标 | `(col, row)` | 屏幕上的位置 |
|---|---|---|
| `0` | `(0, 0)` | 左上 |
| `9` | `(9, 0)` | 右上 |
| `10` | `(0, 1)` | 左中 |
| `20` | `(0, 2)` | **左下**（`row == rows - 1` ⇒ 世界 Y 最小） |
| `29` | `(9, 2)` | 右下 |

## 3.7 校验规则

### 精确契约

```rust
pub enum Severity { Fatal, Error, Warning }

pub struct LevelIssue {
    pub severity: Severity,
    pub path: String,     // 字段路径，见下
    pub message: String,
}

/// 一次调用返回**全部**问题（不短路），纯函数。
pub fn validate(level: &Level) -> Vec<LevelIssue>
```

| 严重度 | 含义 | 加载时 | 保存时 | `animj-dev validate-levels` 退出码 |
|---|---|---|---|---|
| `Fatal` | **结构不变式**被破坏。不可能由合法编辑产生，出现即视为 bug | 拒绝 | 拒绝 | 非零 |
| `Error` | 语义违规：数据自相矛盾（例：关卡没有终点 / 道具实体缺类型） | 拒绝 | **允许**（编辑器必须能存半成品，见 `08` §8.8.3） | 非零 |
| `Warning` | 可疑但可用（例：实体卡在实心瓦片里） | 允许 | 允许 | `0` |

- `path` 格式：结构字段用 `rooms[13].entities[1].item`；瓦片类问题用 `rooms[3].tiles[12]`；实体类问题用 `rooms[7].entities[2].at`。
- 校验必须是**纯函数**：不读文件、不依赖 Bevy，可直接单测。规则实现必须遍历全部房间，**不得**在第一个错误处返回。
- 规则编号（`V1`…`V24`；`V7`/`V8`/`V9`/`V10`/`V11` 已移除且不复用）是对外契约：错误消息必须带上编号，便于文档、编辑器高亮与 CI 输出互相定位。

| # | 规则 | 严重度 |
|---|---|---|
| V1 | `version == VERSION`（当前 `3`） | Fatal |
| V2 | `name` 匹配 `^[a-z0-9_]+$`，且等于文件名主干 | Error |
| V3 | `grid.cols >= 1 && grid.rows >= 1 && cols * rows <= 256` | Error |
| V3b | `cols * rows > 64` | Warning（见 `02` §2.4 的裁剪 Roadmap） |
| V4 | `rooms.len() == cols * rows` | Error |
| V5 | 每个房间 `tiles.len() == 16`；每行字符数（**按 `char` 计**）`== 16` | Fatal |
| V6 | 每个字符都在 legend 中 | Error |
| V12 | 全关卡 `player_spawn` 实体数 `== 1`（房间不限） | Error |
| V13 | 实体 `at` 满足 `0 <= x < 512 && 0 <= y < 512`；`size` 各分量 `>= 1` 且 `<= 512` | Error |
| V14 | `kind` 在 §3.4 注册表中 | Error |
| V15 | 房间 `spawn` 若为 `Some`，坐标满足 V13 的范围 | Error |
| V16 | 房间 `name` 若为 `Some`，全关卡唯一；`start` 的 `col < cols && row < rows` | Error |
| V17 | 实体的 AABB（`at ± size/2`）与任何 `Solid` 瓦片的**实际碰撞范围**重叠（有 `collision` 时按多边形栅格化结果，否则整格） | Warning |
| V18 | 相邻房间边界不连续（一侧 `Solid`、另一侧 `Air`，形成单像素墙） | Warning |
| V19 | `palette`：`char` 唯一且非 `.`；`name` 非空 | Error |
| V20 | `background`：`layers.len() <= 3`；每层 `image` 非空；`scroll_x/y <= 100` | Error |
| V21 | `palette[*].collision` 结构非法：顶点数不在 `3..=32`、坐标超出 `0..=32`、顶点重合、面积为 0、或边自交 | Error |
| V22 | `palette[*].collision` 实际不生效：`behavior == None`，或栅格化后不覆盖任何像素中心 | Warning |
| V23 | 全关卡 `goal` 实体数 `== 1` | Error |
| V24 | `kind == "item" / "item_spawner"` 必须带 `item` 字段；其它 `kind` 不得带 `item` | Error |
| V25 | `sprite` 仅 `item` / `item_spawner` 可设置；必须是相对 `assets/` 的 `.png` 路径 | Error |

> **V7–V11 已移除**：房间目标机制与装饰品奖励在 v3 删除。编号不复用，避免与历史错误输出混淆。
>
> 上表的严重度语义见本节开头三态表。`V4`（房间数与网格不符）同样属于结构不变式，但可以由编辑器改变 `cols/rows` 而合法地瞬时出现，故定为 `Error` 而非 `Fatal`。

## 3.8 版本化与迁移

| 规则 | 说明 |
|---|---|
| 版本字段 | `version` 是首个字段，缺省即非法 |
| 未知版本 | Loader **必须硬失败**，错误信息包含"文件版本 X，本引擎支持 3"。禁止宽容解析或猜测 |
| 破坏性变更 | legend 增删字符、字段改名/删除、坐标语义改变、实体 kind 语义改变 |
| 非破坏性变更 | 新增 `Option` 字段（必须带 serde 默认值）、新增 Roadmap 中已预留的 `kind` |
| 迁移 | 破坏性变更必须在 `crates/level_format/src/io.rs` 的 `read()` 内提供旧版本结构并迁移到当前 `VERSION`，同时更新本节变更历史表 |

### 变更历史

| 版本 | 日期 | 变更 |
|---|---|---|
| `1` | 2026-10-16 | 初版：2D 房间网格 + legend 瓦片 + 实体列表 + 三种房间条件 |
| `2` | 2026-10-18 | 关卡自带贴图 `tileset`/`tileset_columns` + 地块调色板 `palette`（§3.9） |
| `2`（增补） | 2026-10-19 | 新增可选字段 `background`（§3.10，V20）；**不升版本**，旧文件读取为 `None` |
| `2`（校验放宽） | 2026-10-19 | **移除 V7**（`goals` 非空）：每房至少一个目标改为纯设计约束；空列表运行时永不通关 |
| `2`（增补） | 2026-11-08 | `palette[]` 新增可选字段 `collision`（§3.9，V21/V22）；**不升版本**，旧文件读取为 `None`（整格） |
| `3` | 2026-11-08 | **破坏性**：删除房间级 `goals` / `reward`；新增全关卡唯一 `goal` 终点（V23）与 `item` 道具实体 + `ItemKind`（V24）；旧文件迁移时 `exit` → `goal`、丢弃目标/奖励字段 |
| `3`（增补） | 2026-11-08 | 新增 `item_spawner` 实体与 `sprite` 可选字段（V24/V25）；旧文件 `sprite` 缺省 = 占位色块 |
