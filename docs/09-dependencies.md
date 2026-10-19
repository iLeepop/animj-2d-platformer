# 09 — 依赖矩阵与 Cargo 配置

> **本文拥有**：依赖版本号、bevy cargo feature 开关、Cargo profile、资产根路径推导规则、升级流程。
> **本文不拥有**：任何坐标/尺寸/物理/渲染常量（见 `00` §0.2 常量归属），需要时写「见 `07` §…」。
> 三态标记定义见 `00` §0.9；所有 Bevy / 第三方 crate 断言都带标记 + 证据 URL。
> 探测时间基准：**2026-10-16**。凡版本号变动，必须回填本文 §9.1、§9.2、§9.7.1。

---

## 9.0 依赖策略（三条硬规则）

| # | 规则 | 依据 |
|---|---|---|
| R1 | 运行时依赖只有 `bevy` + `serde` + `ron`；`editor` crate 额外一个 `bevy_egui`（UI 基座）与一个 **仅工具用** 的 `png`（占位资产编码）。联机只用 **`std::net::UdpSocket`**（标准库），**不新增网络 crate** | `00` §0.1 **D15**、**D18** |
| R2 | `level_format` **禁止**依赖 `bevy`，只允许 `serde` + `ron` ⇒ 它在无窗口/无 GPU 环境下也能跑单元测试 | `00` §0.6 |
| R3 | 任何新依赖进 `Cargo.toml` 前，必须先在 §9.2 的兼容矩阵里有一行且状态为「可用」；**禁止** git 依赖 | 本文 §9.2、§9.4.4 |

补充：`game` 禁止依赖 `bevy_egui`（`00` §0.6）；`[dependencies]` 里出现的每个 crate 都必须在 §9.1 或 §9.2 里被点名。

---

## 9.1 选定依赖（精确版本）

| 依赖 | Cargo 写法 | 精确版本（2026-10-16） | 使用它的 crate | 理由 | 证据 |
|---|---|---|---|---|---|
| `bevy` | `"0.20.0"` | **0.20.0**（2026-10-08） | `game`、`editor` | 产品决策锁定 0.20（`00` §0.1 **D1**）；0.20.0 是当前最新已发布版本，无 0.20.1；MSRV 政策是"紧跟最新 stable"，因此工具链用最新 stable、**不写** `rust-version`（写死数字会与上游脱节） | <https://docs.rs/crate/bevy/latest>（版本列表与发布时间） |
| `serde` | `{ version = "1.0.229", features = ["derive"] }` | **1.0.229**（2026-07-18） | `level_format`、`game`、`editor` | 关卡/存档的 `Serialize`/`Deserialize` 基座（`00` §0.6、§0.1 **D17**）；`derive` 是派生宏所需（`serde_derive ^1` 为 optional 依赖，必须显式开 feature） | <https://docs.rs/crate/serde/latest>（版本表 + 依赖行 `serde_derive ^1 optional`） |
| `ron` | `"0.12.2"` | **0.12.2**（2026-06-22） | `level_format`、`game`、`editor` | 关卡文件与存档都是 `.ron`（`00` §0.1 **D6/D17**）；RON 支持注释与尾逗号（人工审阅 `.level.ron` 必需），且 bevy 自己的 dev-dependencies 也用 `ron ^0.12`，格式版本与引擎生态一致 | <https://docs.rs/crate/ron/latest>（版本表、依赖 `serde ^1.0.193`、README 语法概览）；<https://docs.rs/crate/bevy/latest>（dev-dep `ron ^0.12`） |
| `bevy_egui` | `"0.43.1"` | **0.43.1**（2026-10-10） | **仅** `editor` | 0.20 上唯一可用的 egui 集成；全部 bevy 依赖钉在 `^0.20.0`，自 0.43.0 起与 bevy 0.20 同步发布（`bevy_editor_pls` 停在 0.14，`bevy-inspector-egui` 停在 0.19） | <https://docs.rs/crate/bevy_egui/0.43.1>（依赖清单） |
| `egui`（间接） | 不直接声明 | bevy_egui 约束 **`^0.36`**（0.36.2 = 2026-09-08） | 由 `bevy_egui` 传递 | 通过 `bevy_egui::egui` 重导出使用**同一个** egui；直接写 `egui = "0.37"` 会与 `bevy_egui` 的 `^0.36` 冲突（依赖图里出现两个 egui） | <https://docs.rs/crate/bevy_egui/0.43.1>（依赖行 `egui ^0.36`） |
| `png` | `"0.18.1"` | **0.18.1**（2026-02-14） | **仅** `editor` | `animj-dev` 生成占位资产 PNG 需要编码器（`04` §7.2）。它是**开发工具依赖**，不进运行时；纯 Rust、无 unsafe；依赖 `bitflags` / `crc32fast` / `fdeflate` / `flate2` / `miniz_oxide` | <https://docs.rs/crate/png/latest>（版本表 + 依赖清单） |
| `rustc`（工具链下限） | — | **≥ 1.97.1**（本机实测 stable **1.99.1**） | 全仓 | `bevy@0.20.0` 声明 `rust-version = "1.97.1"`，cargo 在**依赖解析阶段**就硬性拒绝更低版本（不是编译到一半才发现）。因此「不写 `rust-version`」的结论成立，但**必须**把下限记在这里 | 见 §9.8 **V9** 的实测输出 |

### 9.1.1 版本写法采用 caret（默认）语义

| 写法 | 展开 | 含义 |
|---|---|---|
| `"0.20.0"` | `>=0.20.0, <0.21.0` | 允许 0.20.x 补丁，**不会**跨到 0.21 |
| `"1.0.229"` | `>=1.0.229, <2.0.0` | serde 正常补丁 |
| `"0.12.2"` / `"0.43.1"` | `>=0.12.2, <0.13.0` / `>=0.43.1, <0.44.0` | 同上 |

【已验证】caret 语义由 Cargo 官方表格给出：`0.2.3 := >=0.2.3, <0.3.0`、`1.2.3 := >=1.2.3, <2.0.0`（"compatible if their left-most non-zero major/minor/patch component is the same"）。
证据：<https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html>（§Default requirements / §Caret requirements）

⇒ 结论：**引擎升级不可能由 `cargo update` 意外触发**，永远是显式改版本号的动作（配合 §9.7 的流程）。`Cargo.lock` 提交进仓库（本仓库是应用而非库），保证 CI 与本地解析一致。

---

## 9.2 兼容性矩阵（bevy 0.20 生态）

三态定义：

- **可用** = 存在明确声明依赖 `bevy ^0.20` 的**已发布**版本。
- **不可用** = 最新已发布版本只支持 `bevy ≤ 0.19`（或更早）。
- **存疑** = 拿不到硬证据（例如只能靠 GitHub 分支判断，而本环境无法直连该域名）。

各行结论的来源：`agent://Ecosystem` 于 2026-10-16 从 docs.rs 依赖清单取证（URL 见最后一列）；标 **★** 的行由本文在本次会话中二次直读 docs.rs 核对。

| crate | 选定版本 | 兼容的 bevy | 用途 | 状态 | 风险 / 备注 | 证据 URL |
|---|---|---|---|---|---|---|
| `bevy` ★ | **0.20.0** | 0.20.0 | 引擎 | **可用（采用）** | 0.20 引入 Profiles/Collections，feature 名与 0.19 不通用（见 §9.4） | <https://docs.rs/crate/bevy/latest> |
| `bevy_egui` ★ | **0.43.1** | `bevy_* ^0.20.0` | 编辑器 UI 基座 | **可用（采用，仅 editor）** | 传递约束 `egui ^0.36`；默认特性含 `bevy_ui`→`bevy_ui_render`、`render`、`picking`、`manage_clipboard`、`open_url`、`default_fonts` | <https://docs.rs/crate/bevy_egui/0.43.1> ・ <https://docs.rs/crate/bevy_egui/0.43.1/features> |
| `egui` | 0.36.x（最新 0.37.0） | 与 bevy 解耦，受 `bevy_egui` 约束 | 即时模式 GUI | **可用（但不直接依赖）** | 单独升到 0.37 会与 `bevy_egui 0.43.1` 的 `^0.36` 冲突 | <https://docs.rs/crate/egui/latest> |
| `bevy_rapier2d` ★ | 0.37.0 | `bevy ^0.20` | 2D 物理 | **可用（未采用）** | 0.20 上唯一可用的物理引擎（`rapier2d ^0.36.1`）；本项目自研 AABB（`00` §0.1 **D3**），见 §9.3 | <https://docs.rs/crate/bevy_rapier2d/latest> |
| `leafwing-input-manager` | 0.22.0（2026-10-15） | `bevy ^0.20.0` | 输入动作映射 | **可用（未采用）** | 见 §9.3 | <https://docs.rs/crate/leafwing-input-manager/latest> |
| `bevy_asset_loader` | 0.28.1（2026-10-13） | 全部 deps `bevy_* ^0.20.0` | 加载态 / `AssetCollection` | **可用（未采用）** | 见 §9.3 | <https://docs.rs/crate/bevy_asset_loader/latest> |
| `bevy_kira_audio` | 0.27.1（2026-10-13） | `bevy_* ^0.20` | 增强音频 | **可用（未采用）** | 与内建 `bevy_audio` 互斥（要求关掉 bevy `audio`）；见 §9.3 | <https://docs.rs/crate/bevy_kira_audio/latest> |
| `bevy_audio`（内建） | bevy 0.20 的 `audio` 集合 | 0.20.0 | BGM / 音效 | **可用（采用）** | 集合展开 = `bevy_audio` + `vorbis`；平台跳跃的音效/BGM 够用，无第三方依赖 | <https://docs.rs/bevy/0.20.0/bevy/>（Collections 表 `audio` 行） |
| `bevy_dev_tools` / `FrameTimeDiagnosticsPlugin`（内建） | bevy 0.20 的 `dev` 集合 / `bevy::diagnostic` | 0.20.0 | 调试 HUD、热重载 | **可用（采用，仅 dev feature）** | 替代已停更的 `iyes_perf_ui` / `bevy_screen_diagnostics` | <https://docs.rs/bevy/0.20.0/bevy/>（Collections 表 `dev` 行）・<https://docs.rs/bevy/0.20.0/bevy/diagnostic/index.html> |
| `avian2d` | 无（最新 0.7.0，2026-06-20） | 最新版只到 **0.19** | 2D 物理（备选） | **不可用** | 「0.20 版」不存在：`/crate/avian2d/0.8.0` 返回 **HTTP 404**；官方版本表最高只到 `0.19 → 0.7`；其 `main` 分支是否已升 0.20 属 **【UNVERIFIED】**（本环境无法直连 github.com，TLS 证书校验失败） | <https://docs.rs/crate/avian2d/latest> |
| `bevy_ecs_tilemap` | 无（最新 0.19.0，2026-07-04） | 最新版只到 **0.19** | 瓦片渲染 | **不可用** | **阻塞瓦片方案**：自身 + 其下游全部锁在 0.19；README 兼容表最高行 `0.18→0.18`（`main` 声称 track bevy，无 0.20 行）。⇒ 瓦片渲染必须自研（见 `07`） | <https://docs.rs/crate/bevy_ecs_tilemap/latest> |
| `bevy_ecs_ldtk` | 无（最新 0.15.0，2026-07-05） | 最新版只到 **0.19** | LDtk 关卡导入 | **不可用** | 双阻塞：自身 `bevy ^0.19` + 依赖 `bevy_ecs_tilemap ^0.19.0`。⇒ **LDtk 工作流在 0.20 上不可选** | <https://docs.rs/crate/bevy_ecs_ldtk/latest> |
| `bevy_ecs_tiled` | 无（最新 0.13.4，2026-08-24） | 最新版只到 **0.19** | Tiled 地图导入 | **不可用** | 双阻塞：自身 `bevy ^0.19` + `bevy_ecs_tilemap ^0.19`；其可选物理集成也指向 0.19（`avian2d ^0.7` / `bevy_rapier2d ^0.36`） | <https://docs.rs/crate/bevy_ecs_tiled/latest> |
| `bevy_tweening` | 无（最新 0.16.0，2026-06-28） | 最新版只到 **0.19** | 补间动画 | **不可用** | ⇒ 装饰品/UI 过渡动画自研（Bevy 自带 `bevy_curve` 特性可用，见 `07`） | <https://docs.rs/crate/bevy_tweening/latest> |
| `bevy_easings` | 无（最新 0.19.0，2026-06-24） | 最新版只到 **0.19** | 缓动 | **不可用** | 同上；它拆包依赖 `bevy_app/bevy_ecs/bevy_math/...` 而非 `bevy` 门面，迁移成本更高 | <https://docs.rs/crate/bevy_easings/latest> |
| `iyes_perf_ui` | 无（最新 0.5.0，2025-05-20） | 最后兼容 **0.16** | 性能 HUD | **不可用** | 落后 4 个大版本；用内建 `bevy_dev_tools` / `FrameTimeDiagnosticsPlugin` 自建 | <https://docs.rs/crate/iyes_perf_ui/latest> |
| `bevy_screen_diagnostics` | 无（最新 0.8.1，2025-04-26） | 最后兼容 **0.16** | 屏显诊断 | **不可用** | 落后 4 个大版本 | <https://docs.rs/crate/bevy_screen_diagnostics/latest> |
| `bevy_pixel_camera` | 无（最新 0.13.0，2024-02-19） | 最后兼容 **0.13** | 像素完美相机 | **不可用** | 实质停更；其设计基于早已不存在的 `Camera2dBundle` 体系。⇒ 像素缩放按 `07` 自研（`ScalingMode` + 整数倍缩放） | <https://docs.rs/crate/bevy_pixel_camera/latest> |
| `bevy-inspector-egui` | 无（最新 0.37.0，2026-06-20） | 最新版只到 **0.19** | 反射 / 世界检查器 | **不可用** | **版本死锁**：它要 `bevy_* ^0.19` + `bevy_egui ^0.40` + `egui ^0.34`，而 0.20 链的 `bevy_egui 0.43.1` 要 `egui ^0.36` ⇒ 同一依赖图内无法共存。编辑器面板自研（`08`） | <https://docs.rs/crate/bevy-inspector-egui/latest> |
| `bevy_editor_pls` | 无（最新 0.10.0，2024-10-14） | 最后兼容 **0.14** | 内嵌编辑器面板 | **不可用** | 近两年未更新，实质弃养；不要纳入选型 | <https://docs.rs/crate/bevy_editor_pls/latest> |

### 9.2.1 这张表对项目的直接后果

| 需求 | 0.20 上的可用方案 | 禁止的捷径 |
|---|---|---|
| 瓦片渲染 | 自研（sprite/atlas 批次，见 `07`） | `bevy_ecs_tilemap` |
| 关卡编辑工作流 | 自研 `.level.ron` + egui 编辑器（`03`、`08`） | LDtk / Tiled 导入器 |
| 移动与碰撞 | 自研 AABB + 网格瓦片（`00` §0.1 D3、`06`） | `avian2d` / `bevy_rapier2d` |
| 装饰品解锁 / UI 过渡动画 | 自研（内建 `bevy_curve` / `bevy_animation` 可用） | `bevy_tweening` / `bevy_easings` |
| 性能 HUD、热重载 | 内建 `dev` 集合（`debug` + `bevy_dev_tools` + `render_dev_tools` + `file_watcher`）+ `bevy::diagnostic` | `iyes_perf_ui` / `bevy_screen_diagnostics` |
| 像素完美缩放 | 自研（见 `07`） | `bevy_pixel_camera` |
| 世界检查器 | 自研 egui 面板（`08`） | `bevy-inspector-egui`、`bevy_editor_pls` |
| 联机传输 | 自研 UDP 协议（`std::net`，`05` §5.15） | `bevy_renet` / `lightyear` / `matchbox` 等第三方网络 crate（0.20 兼容性未取证，且会绕过 D15） |

---

## 9.3 可用但**未采用**的依赖

| crate | 版本 | 一句话结论：为何不用 | 若将来要用的触发条件 |
|---|---|---|---|
| `bevy_rapier2d` | 0.37.0 | 它是 0.20 上唯一可用的物理引擎，但本项目要求的是网格瓦片平台跳跃的确定性手感，用纯函数 AABB 内核即可完全掌控（`00` §0.1 **D3**、**D12**），引入刚体求解器会把手感参数的所有权交给外部 crate（与 `06` 的常量归属冲突） | 需要旋转刚体、绳索/关节、复杂多边形碰撞时 |
| `leafwing-input-manager` | 0.22.0 | 输入抽象在本项目只需 4 个动作，用 `ButtonInput<KeyCode>` + 一个 `PlayerInput` 结构即可覆盖，不引入新 crate 与新的输入状态机（`00` §0.1 **D16**） | 需要运行期重绑定按键 / 多设备映射 UI 时 |
| `bevy_asset_loader` | 0.28.1 | 资产数量是"一张图集 + 一个关卡文件 + 字体"的量级，用内建 `AssetServer` + `LoadState` 轮询足够；引入它会把启动态绑到额外的状态机上（`05` 的调度已经够用） | 资产达到数百项、需要分阶段加载进度条时 |
| `bevy_kira_audio` | 0.27.1 | 内建 `bevy_audio`（`audio` 集合 = `bevy_audio` + `vorbis`）已覆盖 BGM/音效；kira 的价值在变调、声道、滤波、淡入淡出，本项目用不到，且它与内建 `audio` **互斥**（引入后必须关掉 bevy 的 `audio` feature，牵动 §9.4 的验证） | 需要动态混音/音频效果链时 |

---

## 9.4 bevy 0.20 的 feature 选择

### 9.4.1 目标写法

```toml
bevy = { version = "0.20.0", default-features = false, features = ["2d", "ui", "audio", "wav", "complex_script_segmentation"] }
```

| 开关 | 为什么 | 标记与证据 |
|---|---|---|
| `default-features = false` + `features = ["2d"]` | 0.20 官方文档（Profiles 小节）给出的正是这个写法；`default` profile = `2d` + `3d` + `ui` + `audio`，关掉默认可以少编译整个 3D 渲染器 | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/>（§Profiles） |
| `ui` | 屏幕空间 UI 用 `bevy_ui` 自带的 `ZIndex`（见 `00` §0.4），且 `assets/sprites/ui/` 需要一个 UI 层（见 `04`）；另外 `bevy_egui 0.43.1` 的默认特性就含 `bevy_ui` → `bevy_ui_render`，game 与 editor 开同一套 bevy feature 可避免两个 crate 解析出不同特性集 | 【已验证】`ui` 是与 `2d` 并列的独立 profile（§Profiles）；【已验证】bevy_egui 默认特性含 `bevy_ui`（features 页） |
| `audio` | 拾取/通关反馈音效与 BGM 用内建 `bevy_audio` 即可，零第三方依赖；`2d` 单独写**不会**带上它（`audio` 是与 `2d` 并列的独立 profile） | 【已验证】`audio` 集合 = `bevy_audio` + `vorbis`；`default` = 2d+3d+ui+audio（§Profiles / §Collections） |
| `complex_script_segmentation` | 中文/日文的**字典式断行**。不开时 parley 走 `LineSegmenter::new_for_non_complex_scripts`，遇到 CJK 每个文本布局都会打印 `ICU4X data error: No segmentation model for complex script: Chinese/Japanese`，且断行退化。开启后 bevy_text → `parley/complex-scripts` → `LineSegmenter::new_dictionary`（ICU4X 编译期数据） | 【已验证】<https://docs.rs/crate/bevy_text/0.20.0/features>（`complex_script_segmentation = ["parley/complex-scripts"]`）；<https://docs.rs/parley/0.11.1/src/parley/analysis/mod.rs.html>（`line_segmenter_impl` 两个 cfg 分支）；<https://docs.rs/icu_segmenter/2.3.0/src/icu_segmenter/complex/mod.rs.html>（`select` 的错误）；本仓实测：开启后中文菜单与 300 帧 HUD 均 0 条该错误 |
| **不**加 `tonemapping_luts` | `Camera2d` 用默认 `Tonemapping`（见 `07`），不需要 LUT；feature 表原文："If everything is pink, you need to enable this feature or change the `Tonemapping` method for your `Camera2d`" | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/>（Feature List 行 `tonemapping_luts`） |
| **不**加 `bevy/serialize` | 本项目的 serde 类型在自己的 crate 里（`level_format` / `game`），不需要 bevy 的反射序列化（`03`、`00` §0.7） | 设计决策 |
| `dev`（**逐个子 feature，仅开发期**） | ★ **不要**直接用 `bevy/dev` 集合：它含 `render_dev_tools` → 启用 `bevy_dev_tools/render` → 拖入 `bevy_pbr`（3D），而 `render_debug` 的初始化系统要求 `MeshPipelineViewLayouts`（3D 运行期资源）⇒ **2D-only 构建启动即 panic**（实测，见 `07` §7.9.2.1）。因此我们自己的 `dev` feature 只列三个子 feature：`bevy/debug` + `bevy/bevy_dev_tools` + `bevy/file_watcher`（`.level.ron` 热重载依赖最后一个，见 `08`） | 【已验证】`bevy_dev_tools` 的 `render` feature 依赖 `bevy_pbr ^0.20.0` <https://docs.rs/crate/bevy_dev_tools/0.20.0/features> ；panic 现场见 `07` §7.9.2.1 |

### 9.4.2 必须承认的展开盲区（**【UNVERIFIED】**）

| # | 未知项 | 为什么未知 | 一旦为否会怎样 |
|---|---|---|---|
| U1 | `2d` profile 的完整 feature 列表 | 0.20 文档只给一句描述："The default 2D Bevy experience. This includes the core Bevy framework, 2D functionality, scenes and picking." —— **不给展开集合** | 需要按 U2/U3 的结论手工补 feature |
| U2 | `2d` 是否已含 `bevy_ui` / `bevy_text` | 描述里未提 UI。**【INFERENCE】**由 `default = 2d + 3d + ui + audio` 推断 `2d` **不含** UI（否则 `ui` 这个 profile 无从定义）；但 `common_api` 明确含 `bevy_text`，而 `2d` 是否包含 `common_api` 未逐字给出（**【INFERENCE】**是） | 编译缺符号（`Text`/`Node`）→ 保留 `"ui"`（本目标写法已包含） |
| U3 | `2d` 是否已含窗口后端（`bevy_winit`、平台特性 `x11`/`wayland`、`default_font`、`bevy_gilrs`） | 这些属于 `default_platform` 集合，文档未说明 `2d` 是否包含该集合 | 应用起不来窗口 / 文字不显示 → 补 `"default_platform"` |

> 已证实的相关事实（可用来缩小盲区）：`common_api` 的固定集合是 `bevy_animation, bevy_camera, bevy_color, bevy_curve, bevy_gizmos, bevy_image, bevy_mesh, bevy_shader, bevy_material, bevy_text, bevy_window, hdr, png`；`2d_api = common_api + bevy_sprite`；`2d_bevy_render = 2d_api + bevy_render + bevy_core_pipeline + bevy_post_process + bevy_sprite_render + bevy_gizmos_render`；`ui_bevy_render = ui_api + bevy_render + bevy_core_pipeline + bevy_ui_render`。证据：<https://docs.rs/bevy/0.20.0/bevy/>（§Collections 表，逐行展开）【已验证】

⇒ 由 `common_api` 含 `png` 可知：**PNG 解码在 2d 链上可用**（像素贴图必需）；由 `common_api` 含 `bevy_text` 可知文本类型的子 crate 会被编译，但**是否注册渲染**取决于 `bevy_ui_render` / `bevy_text` 的插件是否随 profile 打开——这正是 U2 要实测的点。

### 9.4.3 保底方案（按顺序降级）

1. **保底（一定能跑）**：把依赖改回 `bevy = "0.20.0"`（默认 features = 2d + 3d + ui + audio），先让 M0 全链路跑通；代价是多编译 3D 渲染器（编译时间/体积）。
2. **目标**：`default-features = false, features = ["2d", "ui", "audio"]`（§9.4.1）。
3. **若 U2/U3 有一项为否**：不要堆零散 feature 名，而是补**集合名**，例如 `features = ["2d", "ui", "audio", "default_platform", "2d_bevy_render", "ui_bevy_render"]`（这些集合名同样来自 0.20 feature 页，稳定性高于零散 feature）。
4. **判定规则**：只要 §9.8 的 V1–V4 全部通过（能开窗、键鼠可用、`Node`+`Text` 能显示、图集不模糊），就锁定目标写法并删除保底写法；若某一项挂了，按 U1–U3 的对应行补集合，并把实测结论回填 §9.4.2（把对应行改标【已验证】+ 记录 `cargo tree` 输出摘要）。

### 9.4.4 明确禁止

| 禁止项 | 原因 |
|---|---|
| 任何 **git 依赖**（例如 `bevy_ecs_tilemap` 的 `bevy-track`/`main`） | 无固定 `rev` 的 git 依赖会让 CI 与本地解析到不同代码；若确需试验，必须写 `rev = "<sha>"` 且只允许存在于草稿分支，禁止合入 `main` |
| `bevy-inspector-egui` / `bevy_editor_pls` / `iyes_perf_ui` / `bevy_screen_diagnostics` / `bevy_pixel_camera` | §9.2 判定为不可用；引入即解析失败或行为错乱 |
| `avian2d` / `bevy_tweening` / `bevy_easings` / `bevy_ecs_tilemap` / `bevy_ecs_ldtk` / `bevy_ecs_tiled` | §9.2 判定为不可用（0.19 及更早） |
| `bevy_rapier2d` / `leafwing-input-manager` / `bevy_asset_loader` / `bevy_kira_audio` | §9.2 判定为可用但**未采用**；引入前必须先改 `00` §0.1 的对应决策（D3/D15/D16）并同步 `06`/`05`，禁止顺手加入 |
| 直接声明 `egui = "..."` | 会与 `bevy_egui` 的 `egui ^0.36` 形成第二份 egui；只用 `bevy_egui::egui` 重导出 |
| 直接使用 **`bevy/dev` 集合** | 它含 `render_dev_tools` → `bevy_dev_tools/render` → `bevy_pbr`，其 `render_debug` 初始化系统在 2D-only 构建下启动即 panic（§9.4.1 的 `dev` 行、`07` §7.9.2.1）。只允许用 `bevy/debug` + `bevy/bevy_dev_tools` + `bevy/file_watcher` |
| 用 **`-p game` / `-p editor`（单包入口）与 `--workspace`（全仓入口）混着用** | Bevy 的 feature 集由「被选中的包集合」决定：单包入口不含另一个包（因而少了它带来的 bevy feature）⇒ 两套 feature 集。在同一会话里来回切换会**各触发一次 Bevy 全量重编译（实测 ~6 分钟）**。实践：一次会话只用一类入口（`cargo game`/`cargo editor`/`cargo dev-cli`，或 `cargo test-all`/`cargo build-all`）；提交前完整检查用 `cargo test-all`。注意 `--workspace` **不能**与 `--bin` 组合（cargo 报用法错），所以单包入口无法统一成 workspace 形式 |
| 在成员 crate 里写 `[patch]` / `[profile]` 段 | `[patch]` 改变全仓解析结果、绕过 §9.2；`[profile.*]` 只在 workspace 根生效（成员里的同名段被 Cargo 忽略）【INFERENCE】 |

---

## 9.5 完整 `Cargo.toml`

### 9.5.1 workspace 根 `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["crates/level_format", "crates/game", "crates/editor"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
# 版本唯一真相源：本文件。成员 crate 一律用 `workspace = true` 继承。
bevy = { version = "0.20.0", default-features = false, features = ["2d", "ui", "audio", "wav", "complex_script_segmentation"] }
serde = { version = "1.0.229", features = ["derive"] }
ron = "0.12.2"
bevy_egui = "0.43.1"
# 仅 animj-dev 工具使用（占位资产 PNG 编码），不进运行时
png = "0.18.1"

level_format = { path = "crates/level_format" }
game = { path = "crates/game" }

# —— 编译性能：依赖用 O3 编，自己的代码用 O1 编（官方 Quick Start 推荐）——
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

# —— 发布 ——
[profile.release]
codegen-units = 1
lto = "thin"
```

- `[profile.dev]` / `[profile.dev.package."*"]` 的组合是 Bevy 官方 Quick Start "Compile with Performance Optimizations" 明示的配置（代价：首次构建变慢，之后依赖不再重编）。【已验证】<https://bevy.org/learn/quick-start/getting-started/setup/>
- `[profile.release]` 的 `lto`/`codegen-units` 是发布期体积与运行速度的取舍；**不是**上游强制配置。【INFERENCE】
- `resolver = "3"` 与 `edition = "2024"` 配套使用。

### 9.5.2 `crates/level_format/Cargo.toml`

```toml
[package]
name = "level_format"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
ron   = { workspace = true }
```

- 只有两个依赖，**没有** `bevy`（`00` §0.6 硬约束）：这个 crate 承载 `.level.ron` 的数据结构与校验（`03`）、`version` 迁移（`00` §0.7），必须能在没有窗口/GPU 的环境里做单元测试。

### 9.5.3 `crates/game/Cargo.toml`

```toml
[package]
name = "game"
version.workspace = true
edition.workspace = true

[lib]
name = "game"
path = "src/lib.rs"

[[bin]]
name = "animj"
path = "src/main.rs"

[features]
default = []
# 开发期专用：热重载 + 调试工具。发布构建**不带**该 feature。
# ★ 刻意不用 `bevy/dev` 集合（它含 render_dev_tools → bevy_pbr → 2D-only 构建启动 panic）：
#   逐个子 feature 列出，理由见 §9.4.1 与 07 §7.9.2.1。
dev = ["bevy/debug", "bevy/bevy_dev_tools", "bevy/file_watcher"]

[dependencies]
bevy         = { workspace = true }
level_format = { workspace = true }
serde        = { workspace = true }
ron          = { workspace = true }
```

- `default = []`：**故意**不把 `dev` 设为默认，因为 bevy 的 `dev` 集合官方注明不应出现在发布构建里（§9.4.1）。开发时用 `--features dev`（§9.5.5 提供别名）。
- `serde` / `ron` 在这里是给存档用的（`00` §0.1 **D17** 的 `SaveData`）。

### 9.5.4 `crates/editor/Cargo.toml`

```toml
[package]
name = "editor"
version.workspace = true
edition.workspace = true

[[bin]]
name = "animj-editor"
path = "src/bin/animj_editor.rs"

[[bin]]
name = "animj-dev"
path = "src/bin/animj_dev.rs"

[features]
default = []
dev = ["bevy/dev", "game/dev"]

[dependencies]
bevy         = { workspace = true }
bevy_egui    = { workspace = true }
game         = { workspace = true }
level_format = { workspace = true }
ron          = { workspace = true }
png          = { workspace = true }
```

- `bevy_egui` 只出现在这里（`00` §0.6：`game` 不得依赖 `bevy_egui`）；使用其**默认特性**，不做裁剪（默认包含 `render`、`bevy_ui`、`picking`、`manage_clipboard`、`open_url`、`default_fonts`【已验证】）。
- `animj-dev`（CLI 工具，`00` §0.6）与 `animj-editor` 同 crate，因此它也会连带编译 bevy（编译时间成本，运行期不需要 GPU）。

### 9.5.5 `.cargo/config.toml`（在仓库根）

```toml
[alias]
game   = ["run", "-p", "game",   "--bin", "animj",       "--features", "dev"]
editor = ["run", "-p", "editor", "--bin", "animj-editor", "--features", "dev"]
dev-cli = ["run", "-p", "editor", "--bin", "animj-dev"]
check-all = ["check", "--workspace", "--all-targets"]
```

用法：`cargo game` / `cargo editor` / `cargo dev-cli`。别名只是把 `--features dev` 固化，避免"忘了开热重载"。
（可选）若 §9.8 的 V8 证明 `bevy/dynamic_linking` 与 `dev` 共存，可再加一个 `game-fast = ["run", "-p", "game", "--bin", "animj", "--features", "dev,bevy/dynamic_linking"]`；未验证前不写入。

---

## 9.6 资产根路径配置（两个 bin 共用仓库根 `assets/`）

### 9.6.1 Bevy 0.20 的解析规则（**【已验证】**）

1. `AssetPlugin.file_path` 是"相对项目根"的默认资产路径，字段语义见 <https://docs.rs/bevy/0.20.0/bevy/asset/struct.AssetPlugin.html>。
2. 实际文件系统的根路径 = `get_base_path().join(file_path)`，而 `get_base_path()` 的取值顺序是：
   **`BEVY_ASSET_ROOT` 环境变量 → `CARGO_MANIFEST_DIR` 环境变量 → 可执行文件所在目录**。
   证据：<https://docs.rs/bevy_asset/0.20.0/src/bevy_asset/io/file/mod.rs.html#25-35>（`get_base_path`）与同文件 `FileAssetReader::new`（`get_base_path().join(path)`）。
3. `AssetPlugin::default()` 的 `file_path` = `"assets"`，`mode` = `AssetMode::Unprocessed`，`processed_file_path` = `"imported_assets/Default"`。
   证据：<https://docs.rs/bevy_asset/0.20.0/src/bevy_asset/lib.rs.html#334>（`Default` impl）、同页 `#346`/`#349`（两个常量）。
4. `CARGO_MANIFEST_DIR` 在 `cargo run` / `cargo test` 时也存在于**运行**环境，且指向**当前包**的目录。Cargo Book 原文："Cargo exposes these environment variables to your crate when it is compiled. Note that this applies for running binaries with `cargo run` and `cargo test` as well." 证据：<https://doc.rust-lang.org/cargo/reference/environment-variables.html>（§Environment variables Cargo sets for crates）

⇒ **推论**：从仓库根执行 `cargo run -p game`，base path = `crates/game`，默认 `file_path = "assets"` 会去 `crates/game/assets/` 找文件——这与 `00` §0.6「`assets/` 位于仓库根、两个 bin 共用」相冲突，因此**必须显式覆盖 `file_path`**。

### 9.6.2 本项目的写法

在 `game` 里提供唯一的资产路径函数（`editor` 通过依赖 `game` 复用它，避免两份逻辑）：

```rust
// crates/game/src/assets.rs
use std::path::Path;

/// 唯一的资产根路径真相源（见 `09` §9.6）。
///
/// 开发期：Cargo 在 `cargo run` / `cargo test` 时提供 `CARGO_MANIFEST_DIR`，它指向
/// `crates/<crate>`；上跳两级到仓库根后拼 `assets/`。
/// 发布期：该变量不存在，Bevy 的 base path 退化为可执行文件所在目录，因此返回 `"assets"`。
pub fn asset_file_path() -> String {
    match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(manifest_dir) => Path::new(&manifest_dir)
            .join("..")
            .join("..")
            .join("assets")
            .to_string_lossy()
            .into_owned(),
        None => String::from("assets"),
    }
}
```

接入 `App`（`DefaultPlugins.set(...)` 覆写单个插件是官方示例的既有写法）：

```rust
DefaultPlugins
    .set(ImagePlugin::default_nearest())   // 像素采样，渲染侧的取舍见 `07`
    .set(AssetPlugin {
        file_path: game::assets::asset_file_path(),
        ..default()
    })
```

设计约束与理由：

| # | 约束 | 理由 |
|---|---|---|
| A1 | 路径算术**只允许**出现在 `asset_file_path()` 一处 | 与 `00` §0.3「转换函数唯一入口」同构：任何地方再手写 `../../assets` 都会在目录调整时静默失效 |
| A2 | `..` 上跳**两级**成立的前提：`crates/game` 与 `crates/editor` 都位于仓库根下一级（`00` §0.6） | 新增/移动 crate 层级时必须同步改这个函数；M0 用一个断言性检查兜住（§9.8 **V6**） |
| A3 | `manifest_dir` 通常是绝对路径，`Path::join` 会把 Bevy 的 base path 整体替换掉 | 于是无论 `get_base_path()` 解析成什么（CI 里设了 `BEVY_ASSET_ROOT` 也一样），最终都指向仓库根 `assets/` |
| A4 | **不**用编译期 `env!("CARGO_MANIFEST_DIR")` | 它会把开发机的绝对路径烧进 release 二进制，到了别人机器上 `file_path` 指向不存在的目录，比直接用 `"assets"` 更糟；运行时读取才能让同一份代码在开发/发布两种布局下都正确 |
| A5 | **不**用 `BEVY_ASSET_ROOT`，**不**在 `crates/*/assets` 建 junction/符号链接 | 前者要求每个人的 IDE/终端运行配置都设一遍（漏了就报"资源找不到"）；后者在 Windows 上需要管理员或开发者模式，破坏"clone 即跑" |
| A6 | `unapproved_path_mode` 保持默认 | 所有资产都在 `file_path` 之下；该字段的文档明确"approved folders 是 `AssetPlugin::file_path` 与各 `AssetSource` 目录"【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/struct.AssetPlugin.html> |
| A7 | `watch_for_changes_override` / `use_asset_processor_override` 保持 `None` | 热重载交给 `dev` feature（⇒ `file_watcher`），资产处理我们不用（§9.6.3） |

### 9.6.3 开发与发布的差异

| 项 | 开发（`cargo run` / `cargo test`，仓库根为 cwd） | 发布（直接执行 exe / 双击） |
|---|---|---|
| base path | `CARGO_MANIFEST_DIR` = `crates/<crate>` | `current_exe().parent()` = 可执行文件所在目录 |
| 实际 `file_path` | `<manifest>/../../assets`（绝对路径） | `"assets"` ⇒ `<exe 目录>/assets` |
| 发布包布局 | — | `animj.exe`、`animj-editor.exe` 与 `assets/` **同级**（`assets/` 内含 `levels/`、`sprites/`、`fonts/`，见 `00` §0.6） |
| 热重载 | `dev` feature 打开 `file_watcher`：改 `.level.ron` 后 `AssetServer` 侦测变更并重载（`AssetEvent` 是 Message，消费侧用 `MessageReader<AssetEvent<_>>`；见 `10`） | 无 watcher、无 `bevy_dev_tools`（bevy `dev` 集合官方注明不应出现在发布构建） |
| `mode` | `AssetMode::Unprocessed`（默认） | 同左：我们不做资产预处理，`processed_file_path`（默认 `imported_assets/Default`）用不到 |
| `.meta` 文件 | 不需要（不使用 asset processor） | 也可不打进发布包：`AssetMetaCheck::Always` 下"meta 不存在时使用默认 meta"【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/enum.AssetMetaCheck.html> |
| 未来若引入纹理压缩 | — | 需要同时开 `asset_processor` + `compressed_image_saver`，并把 `mode` 改为 `AssetMode::Processed`、把 `imported_assets/` 一并发布；届时本节与 `04` 需同步修改 |

---

## 9.7 升级策略

### 9.7.1 版本联动三角：`bevy` ↔ `bevy_egui` ↔ `egui`

| 组件 | 当前锁定 | 约束来源 | 证据 |
|---|---|---|---|
| `bevy` | **0.20.0** | 产品决策（`00` §0.1 D1） | <https://docs.rs/crate/bevy/latest> |
| `bevy_egui` | **0.43.1** | 唯一钉在 `bevy_* ^0.20.0` 的 egui 集成 | <https://docs.rs/crate/bevy_egui/0.43.1> |
| `egui`（传递） | **`^0.36`**（最新 0.36.2） | `bevy_egui 0.43.1` 的依赖约束；egui 自身最新是 0.37.0，**不得**单独升级 | <https://docs.rs/crate/bevy_egui/0.43.1> ・ <https://docs.rs/crate/egui/latest> |

三条不变量：

1. 升级 `bevy` ⇒ 必须同时升级 `bevy_egui` 到"支持新 bevy 的那一版"，并接受它带来的 `egui` 区间。
2. 升级 `bevy_egui` ⇒ 必须复核它允许的 `egui` 区间是否变化（0.43.1 是 `^0.36`）。
3. **任何情况下都不要单独升级 `egui`**：它不是我们的直接依赖，只通过 `bevy_egui::egui` 使用（§9.1）。

`serde` / `ron` 与 bevy **无耦合**（`level_format` 不依赖 bevy，§9.5.2），可独立升级；但它们影响关卡文件与存档的兼容性，必须按 `00` §0.7 的版本化规则处理（`.level.ron` 的 `version` 与迁移函数在 `03`）。

### 9.7.2 升级检查清单（按序执行，全部通过才允许合并）

| # | 检查 | 命令 / URL | 不通过的处置 |
|---|---|---|---|
| 1 | 读迁移指南，列出影响本项目的破坏性变更 | `https://bevy.org/learn/migration-guides/<old>-to-<new>/`（0.20 的链接：<https://bevy.org/learn/migration-guides/0-19-to-0-20/>） | 停止升级，先评估工作量 |
| 2 | 确认 `bevy_egui` 已发布对应版本，并记下它的 `bevy_*` 与 `egui` 区间 | `https://docs.rs/crate/bevy_egui/latest`（看依赖清单） | 若未发布 ⇒ 本次**只升 bevy 不升 bevy_egui 是不允许的**，整批等上游 |
| 3 | 确认 `egui` 最新版本落在第 2 步的区间内 | `https://docs.rs/crate/egui/latest` | 若已越界（例如 bevy_egui 仍要 `^0.36` 而 egui 已 0.37）⇒ 什么都不做，等 bevy_egui 跟进 |
| 4 | 复查 §9.2：目标版本上是否有新 crate 终于可用（瓦片/补间/调试 HUD） | `https://docs.rs/crate/<crate>/latest` | 有 => 记录到 §9.2，是否需要引入另开决策（不得顺手引入） |
| 5 | 改 `[workspace.dependencies]` 的版本号 | 本文件 §9.5.1 | — |
| 6 | 检查依赖图里**没有**两份 `bevy*` | `cargo tree -d` | 有重复 ⇒ 某个中间依赖没跟上，停止升级并记录 |
| 7 | 更新 lockfile 并提交 | `cargo update` 后提交 `Cargo.lock` | — |
| 8 | 全仓搜过时 API 名 | 对照 `10` 的黑名单逐条搜索 | 逐个改名 |
| 9 | 重跑 §9.8 的 V1–V7 | 见 §9.8 | 按失败项补 feature / 补 URL |
| 10 | 回填本文（§9.1 精确版本、§9.2 矩阵、§9.7.1 锚点）与 `10` 的新事实 | — | 未回填视为升级未完成 |

### 9.7.3 两条禁令

- **禁止**用 `cargo update` 当升级手段：caret 语义（§9.1.1）保证 `cargo update` 不会跨破坏性版本，所以"意外升级"不会发生——升级永远是显式改版本号 + 走完 §9.7.2。
- **禁止**为了绕过某个 crate 的版本冲突而用 `[patch]`、`[replace]` 或 fork：0.20 生态里冲突的根因是上游未跟进（§9.2），patch 会把冲突藏到运行时。

---

## 9.8 M0 验证任务清单（编译通过后回填本文）

> ✅ **M0 已执行完毕，实测结果见 §9.8.1**（2026-10-16 回填）。下表保留任务定义。

> 规则：每项通过后，把结论写回"回填位置"指向的小节，并把对应行的标记升级为【已验证】+ 附 `cargo tree` / 日志输出摘要。**未回填的项视为未完成**。

| ID | 目标 | 命令 | 通过判据 | 回填位置 |
|---|---|---|---|---|
| **V1** | 拿到 `2d`/`ui`/`audio` 的真实展开集合 | 仓库根：`cargo tree -p bevy -e features --prefix none`（必要时加 `-f "{p} {f}"`） | 能列出 `bevy_sprite`（2d）、`bevy_ui` + `bevy_ui_render`（ui）、`bevy_audio` + `vorbis`（audio）等子 crate/feature；盲区 U1–U3 全部有确定答案 | §9.4.2（U1/U2/U3 改为【已验证】） |
| **V2** | 窗口与输入后端确实被打开 | `cargo game`（等价 `cargo run -p game --bin animj --features dev`） | 窗口出现、键鼠有响应、`Esc` 可退出；日志无 "no window backend" 类错误 | §9.4.2（U3） |
| **V3** | UI 与文本可用 | 在 game 的最小场景里 spawn 一个 `Node` + `Text` 与一个 `TextureAtlas` sprite | 文字与图集都能正确显示、字号与图集索引符合 `04`/`07` 的规格 | §9.4.2（U2） |
| **V4** | PNG 与像素采样 | 加载 `assets/sprites/tiles/tileset.png`（路径规格见 `04`） | 贴图放大后不模糊（`ImagePlugin::default_nearest()` 生效，渲染取舍见 `07`） | §9.4.1（PNG 结论） |
| **V5** | 编辑器宿主可用且依赖不冲突 | `cargo editor`（等价 `cargo run -p editor --bin animj-editor --features dev`） | egui 面板出现、可与 2D 画面共存、鼠标输入按 `08` 的策略被正确吸收；`cargo tree -d` 无重复 `bevy*` | §9.5.4、§9.2（`bevy_egui` 行） |
| **V6** | 资产路径三种运行方式都成立 | (a) 仓库根 `cargo game`；(b) 仓库根 `cargo editor`；(c) `cargo build --release -p game` 后把 exe 与 `assets/` 放同一目录再运行 | 三者都能加载同一个 `assets/levels/<id>.level.ron`；用 `RUST_LOG=bevy_asset=debug` 核对日志里的 base path（`bevy_asset` 会打印 "Asset Server using … as its base path."） | §9.6.3 |
| **V7** | 依赖卫生 | `cargo tree -p level_format`；`cargo tree -d` | `level_format` 的依赖树**不含任何 `bevy*`**（`00` §0.6）；全仓无重复版本 | §9.5.2、§9.2 |
| **V8**（可选） | `dynamic_linking` 是否与 `dev` 共存 | `cargo run -p game --bin animj --features dev,bevy/dynamic_linking` | 能编译能开窗；若冲突则**不**把该别名写进 `.cargo/config.toml`，并在此记录冲突现象 | §9.5.5 |
| **V9** | 工具链 | `rustup show` / `cargo build -p game` | 用最新 stable 编译无 MSRV 报错（Bevy README 述其 MSRV 贴近最新 stable）；因此本项目不写 `rust-version` | §9.1 |

### 9.8.1 实测结果（2026-10-16）

环境：Windows 11 26100 / RTX 4070 SUPER / Vulkan / **rustc 1.99.1** / bevy 0.20.0。

| ID | 结果 | 实测证据摘要 |
|---|---|---|
| **V1** | ✅ **通过，且盲区全消** | `cargo tree -p bevy_internal --depth 1` 实际拉入约 50 个 `bevy_*` 子 crate：`bevy_sprite` `bevy_sprite_render` `bevy_ui` `bevy_ui_render` `bevy_ui_widgets` `bevy_text` **`bevy_winit`** `bevy_window` `bevy_audio` `bevy_gilrs` `bevy_gizmos(+_render)` `bevy_picking` `bevy_scene` `bevy_input_focus` …；**`bevy_pbr` 计数 = 0**。⇒ U1/U2/U3 全部为「是」，`features = ["2d","ui","audio","wav"]` 锁定（M7 为中文断行追加 `complex_script_segmentation`），§9.4.3 的保底方案**不需要** |
| **V2** | ✅ 窗口与渲染后端；**输入链路已由进程内测试机器验证** | `Creating new window animj (67v0)` + `AdapterInfo { … backend: Vulkan }`；90 帧后正常退出。**F3 切换**两条测试覆盖：(a) 纯函数 `render::debug::toggled()` 的 `just_pressed` 语义（长按/松开不翻转）；(b) **端到端**：注入 winit 后端同型的 `KeyboardInput`（0.20 里是 `Message`）→ `InputPlugin` 的输入系统 → `ButtonInput<KeyCode>` → 本系统翻转，**不依赖窗口/GPU/OS 按键注入**，且顺带钉住「系统必须排在输入系统之后」这一调度契约。⚠️ **OS 级按键注入在本会话仍不可行**（试过 `AppActivate`（撞上标题同为 `animj` 的 Zed 窗口）/`FindWindow`（`$null` 被 marshaling 成空串）/`SetForegroundWindow`（返回 False）/`PostMessage(WM_KEYDOWN)`（winit 只处理有焦点窗口））；真机人工确认方式：`cargo game` 后按 `F3`/`Esc` |
| **V3** | ✅ 通过（附带一条新约束） | HUD 的 `Node`+`Text` 三行文本渲染成功（非背景像素 x ∈ [8,453]），`TextureAtlas` sprite 渲染成功。⚠️ **非 ASCII 文本会被静默截断**（内置字体仅 ASCII）⇒ 已写入 `04` §9 |
| **V4** | ✅ **客观证明无模糊** | 内层画布截图（当时的 512×512）与窗口截图（1024×1024）逐像素比较：**1 048 576 像素、0 差异** ⇒ 严格 2 倍整数放大 + 最近邻 + 无插值。（方法：`animj --frames 90 --screenshot <prefix>` 后比对两张 PNG。现行视口为 320² / 窗口 640²，见 V13） |
| **V5** | ✅ 通过 | `animj-editor.exe --frames 90 --screenshot …`：`bevy_egui::render: Using bindless_mode_array_size 16`、右侧 egui 面板可见、关卡两个房间都渲染（证明 `RoomCulling(false)` 生效）、无 panic。`cargo tree -d` 中无重复 `bevy*` |
| **V6** | ✅ 通过（并发现一个必须记住的失败模式） | (a) 仓库根：`CARGO_MANIFEST_DIR` → 仓库 `assets/` ✓；(b) 编辑器同一入口 ✓；(c) `target/reltest/`（exe 与 `assets/` 同级）→ 日志 `assets = assets` 且 8/8 自检 PASS ✓。⚠️ **裸跑 `target/debug/animj.exe` 会找不到资产**（base path 落到 exe 目录）——见 `10` E4 |
| **V7** | ✅ 通过（判据按实测收窄） | `cargo tree -p level_format` 只含 `ron`/`serde`/`bitflags`/`once_cell`，**无任何 `bevy*`** ✓。⚠️ 全仓**存在上游重复版本**：`syn 2.0/3.0`、`miniz_oxide 0.8/0.9`、`hashbrown 0.16/0.17`、`itertools 0.14/0.15`、`windows-sys 0.52/0.61`、`regex-automata 0.4`、`winnow`、`toml_*` 等，全部来自 bevy / bevy_egui / 构建工具链，**不可消除**。⇒ 判据改为「我们自己三个 crate 共用同一份 bevy」 |
| **V8** | ⏸ **未验证**（可选） | 未测 `dynamic_linking`：它会改变 cargo feature 集并触发 Bevy 全量重编（实测同类操作 ≈ 5–10 分钟）。因此别名里**不写** `game-fast`；需要热重载迭代时再单独测 |
| **V9** | ✅ 通过（**并纠正了本文原有假设**） | **bevy 0.20.0 要求 `rustc ≥ 1.97.1`**：本机原 1.95.0 在**依赖解析阶段**即被 cargo 拒绝（`bevy@0.20.0 requires rustc 1.97.1`，同时 `wesl*`/`wgsl-*` 同要求）；`rustup update stable` → **1.99.1** 后编译通过。⇒ §9.1 已补 `rustc` 下限行 |
| **V10** | ✅ 通过（M1 回填） | 真实 App 内跳跃：`--demo jump --frames 3000` 每次 `DEMO 峰值上升 = 80.00 px`（解析值 80.0，`06` §6.2.6）；`physics` T1–T21 全绿；脚本直接驱动 `PlayerInput`，不依赖窗口焦点 |
| **V11** | ✅ 通过（M2 回填；2026-10-19 更新） | 相机跟随与地图边界：`cargo game -- --frames 90 --screenshot target/m0` 自检 **11/11 PASS**（含 `camera follows player`），窗口与画布逐像素 **0 差异**。`camera::rig` 单测覆盖：地图内精确跟随、四边钳制（`VIEWPORT_W/2` 与 `extent − VIEWPORT_W/2`）、任意玩家位置下视野不越界、单房间内按半视野钳制。房间判定仍由 `room` 单测覆盖（滞回 7.9 px 不切 / 8.0 px 切、4 方向、幂等）。⚠️ 旧设计（相机落点 = 房间中心 + 0.25 s 过渡）已废弃；`CAMERA_PAN_SECS` / ease-out 状态机随之删除 |
| **V12** | ✅ 通过（M3 回填） | 热重载：进程存活时把房间 0 平台 4 格改 5 格 → `检测到关卡文件变化，重新加载` + `关卡热重载：world_01 → world_01（2 个房间，43 个瓦片）`（原 42）⇒ **保存即生效**；写入非法字符 → `[rooms[0].tiles[0]] V6 rooms[0].tiles[0]: 列 0 的字符 'X' 不在 legend 中` + `热重载被拒绝：1 条阻断性问题，继续使用旧关卡` ⇒ **字段级可定位 + 不破坏会话**。⚠️ 方法论教训：前两轮"验证"无效——窗口失焦时无 vsync（>200 fps），`--frames 300` 在改文件前就结束了（见 `10` E9） |
| **V13** | ✅ 通过（2026-10-19 视口缩小两档） | 内层视野由 512² 缩到 **10×10 瓦片 = 320²**（`VIEWPORT_TILES = 10`），窗口保持 2×、缩为 **640²**、`k = 2` 且无黑边。`cargo game -- --frames 90 --screenshot target/m0` 自检 **11/11 PASS**；画布 320×320、窗口 640×640，逐像素比较 **102 400 / 0 差异**。`camera::rig` 单测更新为「半视野钳制」（`VIEWPORT_W/2 = 160`）与「单房间内按半视野跟随」，仍锁定四边不越界。 |

### 9.8.2 M0 实测改变的两处配置（已回填到 §9.4/§9.5）

1. **`dev` 不能用 `bevy/dev` 集合**：它含 `render_dev_tools` → `bevy_dev_tools/render` → 拖入 `bevy_pbr`，其 `render_debug` 初始化系统在 2D-only 构建下**启动即 panic**。改为逐个子 feature：`bevy/debug` + `bevy/bevy_dev_tools` + `bevy/file_watcher`（§9.4.1、`07` §7.9.2.1）。
2. **`editor` 新增仅工具用的 `png 0.18.1`**（占位资产编码；不进运行时）⇒ §9.1 已登记、`08` §8.10.3 已同步。
3. **`.cargo/config.toml` 的别名全部带 `--features dev`**，并按「单包 / 全仓」两类入口分组：单包（`cargo game`/`editor`/`dev-cli`）用于日常，全仓（`cargo build-all`/`test-all`/`check-all`）用于完整检查。两类入口不能混用（混用会让 Bevy 各重编一次，实测 5m54s）；`--workspace` 与 `--bin` 无法组合，所以不存在「统一成 workspace 形式」的方案 ⇒ §9.4.4 已按实测改写。
