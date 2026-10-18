# AGENTS — 本仓库的开发规范（人类与 AI 共同遵守）

> 适用对象：**所有**在本仓库内改动的开发者与 AI agent。本文件是流程与纪律的权威来源；技术细节在 `docs/` 各专题文档。
> 与任何其它来源（模型记忆、博客、旧版 Bevy 示例、其它项目的 AGENTS.md）冲突时，**以本仓库文档为准**。

---

## 1. 开工前必读

| 你的任务 | 必须先读 |
|---|---|
| 任何任务 | 本文件 + `00-conventions.md` |
| 动玩法/关卡设计 | `01-game-design.md` |
| 动房间、相机、重生 | `02-world-and-rooms.md` |
| 动 `.level.ron`、校验 | `03-level-format.md` |
| 动美术资源、动画、装饰品 | `04-assets.md` |
| 动模块、插件、调度 | `05-architecture.md` |
| 动移动、跳跃、碰撞 | `06-physics-and-movement.md` |
| 动相机管线、瓦片渲染、HUD | `07-rendering.md` |
| 动编辑器 | `08-level-editor.md` |
| 加依赖、改 Cargo 配置 | `09-dependencies.md` |
| **写任何 Bevy 代码** | **`10-bevy-0.20-api.md` —— 强制，见 §3.1** |

---

## 2. 完成定义（Definition of Done）

一个改动只有在**全部**满足时才可交付：

1. 行为已实现，**没有** stub / `todo!()` / 空实现 / 未接线的函数。
2. **实际运行过**并观察到预期结果（§4 的验证命令矩阵）。
3. 受影响的所有调用点、测试、文档已在**同一次改动**内更新。
4. `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` 干净。
5. 新增/修改的纯函数有不变量测试（§5.10 的类型）。
6. 没有引入本文件 §3 禁止的任何东西。

**"编译通过"不是完成。"跑起来了但没看结果"不是完成。测试全绿但没跑过真实场景不是完成。**

---

## 3. 红线（违反了就必须回退重做）

### 3.1 禁止凭记忆写 Bevy API

Bevy 0.17 → 0.20 之间发生了大量破坏性重命名（buffered event 体系改名、observer 参数改形、sprite 排序后端更换、文本与 UI 类型重构、着色器语言更换）。**模型内建知识在本版本上系统性过期，凭记忆写出的代码 80% 编不过，且错误形式往往"看起来很合理"。**

强制流程：

1. 落笔前查 `docs/10-bevy-0.20-api.md` 的黑名单与速查表。
2. 表里没有 → 去 `https://docs.rs/bevy/0.20.0/bevy/…` 查（本环境 `docs.rs` 可直连；`crates.io/api/v1` 返回 403；`github.com` TLS 失败）。
3. 官方 example 源码可从 `https://docs.rs/bevy/0.20.0/src/<example>.rs.html` 读到，这是最可靠的写法来源。
4. 仍查不到 → 写**最小验证代码**跑通，然后把结论回填 `10`（标记升级为【已验证】+ URL）。
5. 无法验证的 API 一律标【UNVERIFIED】，**不得**在未验证的情况下铺开使用。

### 3.2 禁止越界引入依赖

- 只允许 `docs/09-dependencies.md` 列出的依赖与版本。
- **禁止**引入物理引擎（`avian2d` / `bevy_rapier2d`）、瓦片渲染（`bevy_ecs_tilemap` / `bevy_ecs_ldtk` / `bevy_ecs_tiled`）、补间（`bevy_tweening` / `bevy_easings`）、输入映射（`leafwing-input-manager`）、资源加载器（`bevy_asset_loader`）、调试 HUD（`iyes_perf_ui` / `bevy_screen_diagnostics`）、像素相机（`bevy_pixel_camera`）、检查器（`bevy-inspector-egui` / `bevy_editor_pls`）。
  理由：本项目**已决定自研**这些能力，且它们在 Bevy 0.20 上多数没有可用版本（矩阵见 `09`）。
- 确有必要新增依赖 → 先改 `09`，说明用途、版本、与 0.20 的兼容证据，再动代码。

### 3.3 禁止绕过既有机制

| 禁止 | 必须改为 |
|---|---|
| 手写房间/世界坐标换算 | 调 `coords::room_local_to_world` / `world_to_room` |
| 手写瓦片索引 | 调 `tiles::tile_at_world` |
| 在 `Update` 里改逻辑位置 | 只在 `FixedUpdate` 的 `GameSet::Physics` 内改 |
| 用 `Transform` 当碰撞真相 | 逻辑真相是 `Aabb`；`Transform` 只是渲染投影 |
| 让可能重叠的 sprite 共享 Z | 用 `00` §0.4 的 Z 表，必要时新增一档并回填该表 |
| 在业务文件里写裸数字 | 提到 `config.rs`，并加 `/// 见 docs/<所有者> §…` |
| 自行 `ron::to_string` 关卡 | 调 `level_format::write` |
| 自行解析 `.level.ron` | 调 `level_format::read` + `validate` |
| 用 `sed`/`perl`/`python` 批量改 Rust 源码 | 用结构化编辑工具逐处改 |

### 3.4 禁止交付造假与降级

- 禁止把"能编译的空壳"当作功能交付；禁止用 `mock` / `fake` / 特殊分支**掩盖**问题。
- 禁止为了让检查变绿而删除、跳过、弱化测试（`#[ignore]`、断言放宽、`--no-fail-fast` 掩盖）。
- 禁止修改既有测试的**期望值**去迎合错误实现。测试描述的是契约，不是现状快照。
- 发现问题（包括文档里已存在的错误）→ **明确报告**，不要悄悄绕过。

### 3.5 禁止文档发散

- `docs/` 下只有 11 个文件 + `AGENTS.md` + `README.md`。**禁止新增文档文件**。
- 新知识必须并入既有文档的正确章节；文档结构变化需先说明理由。
- 禁止在 `docs/` 之外散落设计笔记、临时 md、注释块化的"设计文档"。

---

## 4. 验证命令矩阵

改动的**最低**验证要求（未列出的组合取并集）。命令一律走 `.cargo/config.toml` 的别名 —— 它们都带 `--features dev`；**一次会话里只用一类入口**（单包的 `cargo game`/`cargo editor`/`cargo dev-cli`，或全仓的 `cargo test-all`/`cargo build-all`），混用会让 Bevy 全量重编（`09` §9.4.4）：

| 改动类型 | 必须执行 | 通过判据 |
|---|---|---|
| `level_format` 任何改动 | `cargo test -p level_format`（无 bevy 依赖，秒级） | 全绿；新增校验规则必须配正/反例 |
| 30 房间内容（示例关卡） | `cargo dev-cli -- validate-levels --deny-warnings` + `cargo test -p game`（R4 逐房重生点） | 0 error / 0 warning；30 个房间的重生点全部落在实地上且不压在 `Hazard` 上 |
| 关卡格式 v2（地块调色板） | `cargo test -p level_format`（v2 迁移/自定义字符/V19 三条）+ `cargo dev-cli -- validate-levels --deny-warnings` | 39 项全绿；示例关卡（v1）经迁移后 0 error / 0 warning |
| 关卡数据 / legend / 校验规则 | 上一行 + `cargo dev-cli -- validate-levels --deny-warnings` | 退出码 0；示例关卡无 Error/Warning |
| 占位资产 / 图集布局 | `cargo dev-cli -- gen-placeholder-assets --check` | 退出码 0（生成物与仓库文件逐字节一致） |
| 坐标换算 | `cargo test -p level_format` 与 `cargo test --workspace --features dev` 里的 `coords::` 用例 | 往返换算与四角边界全绿 |
| 物理 / 手感常量 | `cargo test -p game` + `cargo game -- --demo jump --frames 3000` | 单测全绿（T1–T21 + R1–R6 + H1–H2）**且**日志里每次 `DEMO 峰值上升` 都等于 `JUMP_HEIGHT_PX`（80.0，容差 0.5 px）；跳跃数 ≥ 1 |
| 相机 / 渲染管线 | `cargo game -- --frames 90 --screenshot target/m0` | 日志 `VERIFY: PASS`（10/10，含 `camera at room center`）；两张截图做「窗口 == 画布 × k」逐像素比较，必须 **0 差异** |
| 房间判定 / 相机过渡（M2+） | `cargo game -- --demo walk --frames 3000` | 日志出现 `当前房间：(0, 0) → (1, 0)` 与 `相机过渡：房间 (1, 0) → (768, 256)，用时 0.25s`；末尾 `VERIFY: PASS`（10/10）。滞回边界由 `cargo test -p game` 的 `camera::rig` 用例锁定（7.9 px 不切 / 8.0 px 切） |
| 关卡热重载（M3+） | 运行中改 `assets/levels/*.level.ron`（进程必须仍存活：窗口失焦时 >200 fps，`--frames N` 会**远早于**预期结束，见 `10` E9） | 日志出现 `检测到关卡文件变化` + `关卡热重载：…（N 个瓦片）`，且 N 与改动一致；非法改动则出现 `[字段路径] 规则编号 …` 与 `热重载被拒绝`，且旧关卡继续生效 |
| 角色帧动画（M6） | `cargo game -- --demo jump --frames 3000` | 日志有 `动画数据已加载：4 个状态`（资产生效）与 `动画：Idle → Run → Jump → Fall`（行选择由 grounded/速度符号驱动）；`cargo test -p game` 覆盖内置默认值与自定义帧序列（`04` §5.5） |
| 动画数据资产（工具） | 改 `assets/anim/player.anim.ron` 后重启或热重载 | 日志 `动画数据已加载`；校验失败（空帧/越界/时长越界）时回退内置默认并只 warn |
| 音效（M6） | `cargo dev-cli -- gen-placeholder-assets --check` + `cargo game -- --demo jump --frames 3000` | 11 个资产逐字节一致（含 7 个 WAV）；日志出现 `音效：audio/jump.wav`、`audio/land.wav`（walk 还会出现 `audio/death.wav`）；无 panic |
| 存档（M6） | `cargo test -p game` + 两次运行：`ANIMJ_SAVE_DIR=target/save cargo game -- --demo walk --frames 3000` 再 `ANIMJ_SAVE_DIR=target/save cargo game -- --frames 90` | 第一次出现 `存档已写`；第二次出现 `读档：房间 1/2 已通关`，且**不再**出现 `房间 (0, 0) 通关` |
| 暂停菜单（M6） | `cargo game -- --demo pause --frames 120 --screenshot target/pause` | 日志 `[demo pause] 已打开暂停菜单`、退出码 0、两张截图生成（`Esc` 在无头会话不可注入，按钮交互需人工点一次） |
| 存档点 | `cargo game -- --demo checkpoint --frames 3000`，随后 `cargo game -- --frames 30` | 第一次日志出现 `存档点已设置` 与 `角色死亡：回到存档点`；第二次出现 `读档：…存档点已恢复` 且玩家从存档点位置开始；`cargo test -p game` 覆盖激活/失效回退/存档往返 |
| 房间 / 条件 / 奖励（M4+） | `cargo test -p game`（`room::tests::m4_pipeline_*`）+ `cargo game -- --demo walk --frames 3000` | 无头链路：拾取→通关→授奖（幂等）→`GameComplete` 全绿；真实 App 里走过出口出现 `房间 … 通关（N/M）` 且自检 11/11 PASS。⚠️ **不要用 `--demo hop` 当判据**：脚本输入无法可靠地跳过错落在地面的出口、也无法从平台右缘避开尖刺 |
| 编辑器 | `cargo test -p editor` + `cargo editor -- --frames 90 --screenshot target/editor` + `cargo editor -- --frames 140 --demo-play` | 单测全绿（撤销/保存/索引/选区矩形/批量改属性/实体命中/**外部改动确认/另存为**）；退出码 0、无 panic、截图非空；**试玩往返**日志出现 `试玩往返后编辑缓冲一致 = true（dirty = false）`，且 `--demo-play` 注入右移后日志 `试玩诊断：… 投影误差 ≤ 0.5px`（证明试玩时贴图跟随物理位置） |
| 依赖 / Cargo 配置 | `cargo build-all` | 全绿；`09` 已同步 |
| 仅文档 | 交叉引用自查（§ 编号存在、无自相矛盾） | — |

**物理与相机类改动必须实际启动游戏**：单测只能证明不变量，证不了「手感」和「看起来对不对」。

**⚠️ `--demo` 系列测不到输入时序**：demo 在 `FixedPreUpdate` 里**直接写 `PlayerInput`**（每个固定步一份），
绕过了 `PreUpdate` 的按键采样。因此"按键被吞 / 延迟"这类问题必须靠**真实按键**或
`player::input` 的采样级单测（例如 `jump_press_is_latched_until_consumed`）来验证。

**怎么验证「像素完美」**（相机/渲染改动的硬要求）：

```bash
cargo game -- --frames 90 --screenshot target/m0   # 产出 m0-canvas.png(512x512) 与 m0-window.png(1024x1024)
```

然后比对：窗口截图必须**逐像素**等于画布截图的整数倍放大（`k = 窗口宽 / 画布宽`），且画布内不得出现调色板之外的颜色（抗锯齿文本除外，它只该出现在 HUD 区域）。
这条比肉眼看截图可靠得多 —— M0 就是靠它证明了「严格 2 倍整数放大 + 最近邻、1 048 576 像素 0 差异」（`09` §9.8.1 V4）。

---

## 5. 文档同步规则

| 你改了 | 必须同时更新 |
|---|---|
| 任何手感数值（重力、速度、coyote…） | `06` 的常量表 + `config.rs` 注释 |
| 内层分辨率、缩放、Z 值、采样/Msaa | `07`（Z 值同时在 `00` §0.4） |
| `.level.ron` 字段、legend、校验规则 | `03`；破坏性变更还要写迁移 + 版本历史 |
| 坐标系统、命名、目录布局、常量归属 | `00`（并检查全部文档的引用） |
| 房间/相机行为、重生点默认值 | `02` |
| 装饰品、目标条件、里程碑、范围 | `01` |
| 依赖版本、feature、profile | `09` |
| Bevy API 结论（已验证或推翻） | `10`（含标记升降级与 URL） |
| 模块树、插件、调度集合、消息 | `05` |

**规则：常量只有一个所有者文档。** 引用写「见 `06` §6.2」，**不得复制数值**。两个文档出现同一个数字 = bug。

---

## 6. 标准流程 Checklist

### 6.1 新增一种瓦片类型

1. `03` §3.3 legend 表加字符与 tileset 索引。
2. `03` §3.8 变更历史记一条（破坏性）。
3. `level_format`：`TileKind` 加变体、legend 映射、校验规则、单测正反例。
4. `06`：写清碰撞语义（阻挡/单向/致死/无）。
5. `04`：tileset.png 加一格，更新图集布局与占位生成清单。
6. `crates/level_format/src/migrate.rs` 写迁移。
7. 编辑器的瓦片调色板（`08` §工具集）加一项。

### 6.2 新增一种实体 kind

1. `03` §3.4 注册表加行（默认尺寸、数量约束）。
2. `03` §3.7 校验规则加对应条目（如数量/位置约束）。
3. `level_format`：常量 + 校验 + 单测。
4. `05` §5.2 模块树与插件职责（若需要新系统组）。
5. 运行时：生成逻辑 + 需要的话新组件与消息（`05` §5.6 加行）。
6. `01`：若与玩法目标相关，更新目标/范围章节。
7. 编辑器：实体放置工具的可选项（`08`）。

### 6.3 新增一种目标条件

1. `01` §1.4 表加条件（语义 + 与其它条件的组合规则）。
2. `03` §3.5 枚举加变体；§3.7 加校验规则（可满足性）。
3. `05` §5.7 的 `goals::evaluate` 纯函数加分支 + 单测（含边界：0 / 恰好 / 超量）。
4. 若需要新的房间事实（Facts），扩展 `RoomFacts` 并在 `05` §5.7 记录。
5. 编辑器：条件编辑 UI 的选项（`08`）。

### 6.4 新增一件装饰品

1. `assets/sprites/decorations/<id>.png`（严格 32×32，无需图集）。
2. 关卡数据的 `reward: Some("<id>")`（`03` §3.2）。
3. 校验通过（`03` V11 的唯一性规则会自动覆盖）。
4. `04` 的占位资产清单加一行（否则 `gen-placeholder-assets` 与真资产会漂移）。

### 6.5 新增一种玩家能力（跳跃之外的移动手段）

**先停下来。** 能力会改变关卡设计约束、动画帧、输入、测试面与里程碑。必须先：
1. 得到明确的用户批准（`01` §1.3 的排除清单是刻意的）。
2. 改 `01` §1.3 与 §1.9 里程碑。
3. 再按物理/渲染/资源三条流程各自落地。

---

## 7. 不确定性处理

1. 任何**未被本项目文档覆盖**的技术决策，先查 `00`→`10`，再查官方文档。
2. 仍不确定 → 选**最保守**的选项（官方示例写法 > 社区博客；少依赖 > 多依赖；显式 > 隐式），并把结论写进对应文档。
3. 遇到会改变**范围**的不确定性（要不要加敌人？要不要多关卡？）→ **必须问用户**，不得自行扩大或缩小范围。
4. 遇到无法用工具解决、且阻塞交付的问题 → 明确报告：缺什么、已试过什么、需要什么；不要假装完成。

---

## 8. 反加戏守则

以下都是"看起来顺手"但会污染代码库的行为，**在未被要求时一律禁止**：

- 顺手加抽象层 / trait / trait object / 泛型参数（先用具体类型；第二个用例出现时再抽象）。
- 顺手加日志、遥测、性能计时、诊断叠层（用 `DebugPlugin` 的既有手段）。
- 顺手加"更安全"的输入校验、重试、兜底状态（数据错误应在**加载期**一次性暴露，不在运行期层层祈祷）。
- 顺手优化（缓存、批处理、提前计算）——**先有测量，再有优化**；无 profiling 数据的"优化"一律拒绝。
- 顺手改格式、改命名、重排文件、调整无关的缩进与引号。
- 顺手写"给未来用"的字段、参数、配置项。格式里已预留的扩展位除外（`03` §3.4 / §3.5 的 Roadmap）。

**只解决被要求解决的问题。** 额外范围必须先获得批准。

---

## 9. 风格要点

- 标识符英文；文档与注释中文。
- 常量必须带指向所有者文档的 `///` 注释。
- 注释解释**为什么**，不解释**是什么**。
- 测试名描述**不变量**（`单向平台在下落时阻挡，在上升时放行`），不描述现状（`test_fix_1234`）。
- 提交信息：`scope: 中文祈使句`，例 `physics: 修正单向平台在上升期的穿透`；一事一提交。
- 详细代码规范见 `05` §5.12。
