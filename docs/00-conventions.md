# 00 — 项目契约与约定（SSOT）

> 本文是**唯一权威契约**：坐标系统、命名、目录布局、Z 层、常量归属、格式版本化。
> 任何其它文档与本文冲突时，以本文为准。改动本文必须同步检查 `01`–`10` 的引用。

---

## 0.1 已锁定决策

### 由产品负责人（用户）拍板

| # | 决策 | 结论 |
|---|---|---|
| D1 | 引擎 | Bevy **0.20.0**（2026-10-08 发布） |
| D2 | 玩法类型 | 2D 平台跳跃，像素风，瓦片 32×32 像素 |
| D3 | 角色移动/碰撞 | **自研 AABB + 网格瓦片碰撞**（不引入物理引擎） |
| D4 | 房间与相机 | 房间**无缝衔接**；数个房间构成一个关卡；**2D 相机跟随玩家 AABB 中心**，并钳制在关卡 AABB 内（视野不越界） |
| D5 | 房间尺寸 | **16×16 瓦片 = 512×512 像素（方屏）**；内层视野小于一个房间（尺寸见 `07` §7.1.1）——玩家看不到整间房 |
| D6 | 关卡数据 | **瓦片网格 + 实体列表（RON）** |
| D7 | 房间目标 | **通用条件列表（可组合）**；首批条件类型 = `ReachExit` + `Collect` |
| D8 | 装饰品定位 | **纯外观收集**，不附带能力，不做门禁/回溯 |
| D9 | 美术资源 | 暂无，**先用程序化占位资产**；美术后置替换 |
| D10 | 开发工具 | 自研地图设计工具，供开发快速搭平台 + 就地试玩 |

### 由本文档体系拍板（可推翻，但推翻必须同步改文档 + 代码 + 验证）

| # | 决策 | 结论与理由 |
|---|---|---|
| D11 | 时间步 | 固定步长 **60 Hz**（`FixedUpdate` + `Time<Fixed>`），物理逻辑只在 `FixedUpdate` 内推进 |
| D12 | 物理实现形态 | 物理内核 = **纯函数**（不依赖 Bevy ECS），ECS 系统只是薄适配层。理由：可无视 Bevy 测试框架做单元测试 |
| D13 | 像素完美 | 双层相机：内层渲染到**小于一个房间的方形离屏纹理**（尺寸见 `07` §7.1.1），外层整数倍放大 + 黑边。理由：视野收窄到角色周围，同时保持 1 世界单位 = 1 texel 与整数倍缩放；Bevy 无内置整数倍缩放 |
| D14 | 关卡文件坐标 | 文件内使用**屏幕空间**（房间局部，原点 = 房间左上角，+Y 向下）；运行时统一经一个转换函数进入 Bevy 世界空间。理由：手写/审阅 RON 时零心智换算 |
| D15 | 依赖策略 | 只引入 `bevy` + `serde` + `ron`；编辑器额外 `bevy_egui`。**不引入**物理/瓦片/补间/输入/HUD 第三方 crate（0.20 上多数尚不可用，见 `09`） |
| D16 | 输入抽象 | 直接用 `ButtonInput<KeyCode>`，但在 `input` 模块内收敛为 `PlayerInput` 动作结构。理由：不引依赖，同时保留后续替换实现的空间 |
| D17 | 存档 | 自研 `SaveData`（RON）写入平台数据目录，路径由 `save_path()` 单一函数决定 |

---

## 0.2 文档地图与所有权

| 文档 | 主题 | 内容所有者 |
|---|---|---|
| `00-conventions.md` | 契约、坐标、命名、Z 层、常量归属 | 本文 |
| `01-game-design.md` | 核心循环、房间目标、装饰品、通关与范围 | 玩法常量、目标类型语义 |
| `02-world-and-rooms.md` | 房间网格、世界拓扑、相机行为、房间状态 | 房间/相机逻辑常量 |
| `03-level-format.md` | `.level.ron` 数据格式与校验规则 | 格式字段与校验 |
| `04-assets.md` | 美术资源规格、命名、占位资产、图集 | 资源路径与尺寸 |
| `05-architecture.md` | 工程结构、crate 划分、插件与系统调度 | 模块与调度 |
| `06-physics-and-movement.md` | AABB 碰撞算法、移动手感参数 | **所有物理/手感常量** |
| `07-rendering.md` | 像素完美渲染管线、相机、排序 | **所有渲染常量** |
| `08-level-editor.md` | 地图设计工具 | 编辑器交互与快捷键 |
| `09-dependencies.md` | 依赖矩阵、feature、Cargo 配置 | 版本号 |
| `10-bevy-0.20-api.md` | Bevy 0.20 API 速查 + 过期名称黑名单 | API 事实 |

**常量归属规则**：一个常量只能有一个所有者文档。其它文档引用它时写「见 `06` §…」，**不得复制数值**。

---

## 0.3 坐标系统（三层）

| 层 | 单位 | 原点 | 方向 | 用途 |
|---|---|---|---|---|
| 瓦片坐标 `(tx, ty)` | 1 瓦片 | 房间左下角 | +X 右，+Y **上** | 碰撞查询、瓦片索引 |
| 房间局部（文件） | 1 像素 | 房间**左上角** | +X 右，+Y **下** | `.level.ron` 内的一切坐标 |
| 世界像素 | 1 像素 | 地图**左下角** | +X 右，+Y **上**（Bevy） | 运行时 `Transform` |

### 基础尺寸

```
TILE_PX    = 32      // 瓦片边长（像素）
ROOM_TILES = 16      // 房间边长（瓦片）
ROOM_PX    = 512     // 房间边长（像素）= TILE_PX * ROOM_TILES
```

**1 世界单位 = 1 像素**（靠 `ScalingMode` + `scale` 保证，见 `07`）。禁止把世界单位当作"米"。

### 房间网格

- 关卡由 `grid = (cols, rows)` 个房间构成，例：`10 × 3 = 30` 个房间。
- 房间由 `(col, row)` 定位，`col ∈ [0, cols)`，`row ∈ [0, rows)`。
- `.level.ron` 的 `rooms` 数组按 **行主序、自上而下** 排列：`index = row * cols + col`。**`row = 0` 是地图最上面一行。**
- 房间 `(col, row)` 的世界 AABB：

```
min_x = col * ROOM_PX
min_y = (rows - 1 - row) * ROOM_PX
max   = min + (ROOM_PX, ROOM_PX)
```

- 房间中心（几何中心；相机不再落在这里）：

```
center_x = (col + 0.5)     * ROOM_PX
center_y = (rows - 0.5 - row) * ROOM_PX
```

### 两个转换函数（唯一入口）

```
room_local_to_world(col, row, grid_rows, local: Vec2) -> Vec2
    = (col * ROOM_PX + local.x,
       (grid_rows - 1 - row) * ROOM_PX + ROOM_PX - local.y)

world_to_room(world: Vec2, grid_cols, grid_rows) -> (col, row, local: Vec2)
    col   = floor(world.x / ROOM_PX)
    row   = grid_rows - 1 - floor(world.y / ROOM_PX)
    local = (world.x - col*ROOM_PX,
             (rows - 1 - row)*ROOM_PX + ROOM_PX - world.y)
```

- **禁止**在其它任何地方手写这两段算术，必须调用 `crates/game/src/world/coords.rs` 里的这两个函数。
- `.level.ron` 里的 `tiles` 第一行 = 房间**最上一行**；瓦片 `ty = ROOM_TILES - 1 - file_row`。

### 实体位置约定

| 概念 | 表示 | 说明 |
|---|---|---|
| 关卡文件内 | `at: (x, y)` | **房间局部坐标，`(0,0)` = 房间左上角，+Y 向下**，取值为该实体 AABB 的**中心** |
| 运行时逻辑 | `Aabb { min: Vec2, max: Vec2 }`（世界像素） | 碰撞/查询一律基于 AABB，不基于 `Transform` |
| 运行时 `Transform` | 世界像素；**渲染前取整到整像素** | 见 `07`「像素吸附」 |

> 运行时 `Transform` 与逻辑 `Aabb` 可能短暂不一致（浮点）。**逻辑真相永远是 `Aabb`**，`Transform` 是渲染投影。

---

## 0.4 Z 层分配

Bevy 0.20 的 2D 透明排序依据**视图空间 Z**，且排序不稳定 ⇒ **同 Z 的重叠 sprite 绘制顺序无保证**。

**规则：任何彼此可能重叠的 sprite 不得共享 Z。**

| 用途 | Z | 所有者 |
|---|---|---|
| 远景视差 | `-100.0 + layer_index`（`layer_index ∈ 0..=2`，0 最远） | `07` |
| 房间背景板 | `-50.0` | `07` |
| 瓦片贴花（草、砖纹） | `-10.0` | `07` |
| **实体瓦片** | `0.0` | `07` |
| 可拾取物（果实） | `20.0` | `07` |
| **存档点标记** | `21.0` | `07` |
| 敌人 | `30.0` | `07` |
| **角色本体** | `40.0` | `07` |
| 装饰品图层 | `41.0 + layer_index`，`layer_index ∈ 0..=6`（层数、槽位语义与上限见 `04` §6.1） | `04` |
| 粒子 / 特效 | `60.0` | `07` |
| 前景瓦片 | `70.0` | `07` |
| 世界空间文字（`Text2d`） | `80.0` | `07` |
| 相机 | `z = 0.0`（`config::render::CAMERA_Z`）。**不是**「保持默认」：`OrthographicProjection::default_2d()` 的深度范围是 ±1000，相机必须在 0 附近，`-100.0`（远景视差）才落在范围内 | `07` |

- 关卡配置了 `background` 时**不生成**房间背景板（`z::ROOM_BG` 完全不透明，会盖住视差层，见 `07` §7.13.4）。
- 屏幕空间 UI（`Node`）不使用 Z，用 `bevy_ui` 自己的 `ZIndex`。
- 所有 sprite 的 Z **必须显式写入**、来自本表；禁止依赖 `spawn` 顺序。

---

## 0.5 命名约定

### Rust

| 类别 | 约定 | 示例 |
|---|---|---|
| 组件 | 名词，`PascalCase` | `Player`, `Solid`, `Grounded`, `Reward` |
| 标记组件 | 名词，加 `Marker` 后缀避免歧义 | `PlayerMarker` |
| 资源 | 名词，`PascalCase` | `SaveData`, `LevelHandles`, `CameraTarget` |
| 消息（buffered） | **过去式** | `RoomCleared`, `DecorationGranted` |
| 事件（observer） | **命令式/名词化动作** | `TriggerRoomGoal`, `OnPlayerDied` |
| 系统 | `动词_名词`，`snake_case` | `apply_gravity`, `resolve_solid_collisions` |
| 常量 | `SCREAMING_SNAKE_CASE`，按主题分组在 `config.rs` 的模块中 | `physics::GRAVITY` |
| 插件 | `XxxPlugin` | `PlayerPlugin`, `RoomPlugin` |

### 资产与 id

| 类别 | 约定 | 示例 |
|---|---|---|
| 关卡文件 | `assets/levels/<level_id>.level.ron`，文件内 `name` **必须**等于 `<level_id>`（校验强制） | `assets/levels/world_01.level.ron` |
| 图集 | 无 padding、无 margin、严格 32×32 网格 | `assets/sprites/player/cat_sheet.png` |
| 装饰品 id | `snake_case`，与 `assets/sprites/decorations/<id>.png` 同名 | `straw_hat` |
| 瓦片图集 | 单一图集 `assets/sprites/tiles/tileset.png`，索引由 `03` 的 legend 固定 | — |
| 房间 id | **不存字符串**，由 `(col, row)` 推导 | — |

---

## 0.6 目录布局（workspace）

```
animj/
├─ Cargo.toml                 # workspace 根
├─ AGENTS.md                  # 指针，指向 docs/AGENTS.md
├─ docs/
├─ assets/                    # 运行时资产根（两个 bin 共用，见 05 §资产路径）
│  ├─ levels/
│  ├─ sprites/{player,tiles,decorations,ui,backgrounds}/
│  └─ fonts/
└─ crates/
   ├─ level_format/           # lib：纯 serde 数据 + 解析 + 校验，不依赖 bevy
   ├─ game/                   # lib + bin `animj`：全部玩法
   └─ editor/                 # bin `animj-editor`（egui 编辑器）+ bin `animj-dev`（CLI 工具）
```

- `level_format` **不得**依赖 `bevy`。它只依赖 `serde` + `ron`。
- `game` 不得依赖 `bevy_egui`。
- `editor` 依赖 `game` + `bevy_egui`；编辑器通过复用 `game` 的插件构建 App 来做到「就地试玩」。

---

## 0.7 关卡格式版本化

- `Level.version: u32`，当前 **`2`**。
- Loader 遇到不认识的 `version` **必须硬失败并给出清晰错误**，禁止猜测或宽容解析。
- 版本升级时在 `crates/level_format/src/migrate.rs` 提供 `migrate_vN_to_vN1`，并在 `03` 记录变更历史。

---

## 0.8 术语表

| 术语 | 定义 |
|---|---|
| **Level（关卡）** | 一个 `.level.ron` 文件描述的完整地图 = `cols × rows` 个房间 |
| **Room（房间）** | 关卡网格中的一个格子 = 16×16 瓦片（视野小于房间，见 `07` §7.1.1） |
| **Tile（瓦片）** | 32×32 像素的地形单元，类型由 legend 决定 |
| **Goal（目标）** | 一个房间的通关条件集合，见 `01` §目标条件 |
| **Condition（条件）** | Goal 的一项，如 `ReachExit` / `Collect(n)` |
| **Reward（奖励）** | 房间达成目标后授予的装饰品 id |
| **Decoration（装饰品）** | 纯外观图层，叠加在角色 sprite 上 |
| **Solid / OneWay / Hazard** | 瓦片的三类可交互语义，见 `03` 与 `06` |
| **Viewport（视口）** | 内层离屏渲染目标（尺寸见 `07` §7.1.1），像素完美管线的逻辑屏幕 |

---

## 0.9 证据标记约定

本套文档中所有关于 **Bevy 0.20 API** 的断言必须带标记：

- **【已验证】**：有 `docs.rs/bevy/0.20.0/*`、0.20 官方 example 源码或 bevy.org 迁移指南直接支撑，并附 URL。
- **【UNVERIFIED】**：无 0.20 直接证据。
- **【INFERENCE】**：由已验证事实推断，非文档直述。

**禁止凭记忆书写 Bevy API**。Bevy 0.17–0.20 之间发生了大量破坏性重命名（`Event`→`Message` 拆分、observer 参数改 `On<E>`、sprite 排序后端更换等），模型内建知识在此版本上**系统性过期**。速查表与黑名单见 `10`。

代码中若必须使用 **【UNVERIFIED】** 的 API，先写最小验证代码跑通，再把结论回填 `10` 并升级标记。
