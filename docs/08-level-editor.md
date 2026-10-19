# 08 — 关卡编辑器（自研地图设计工具）

> 本文拥有：**编辑器工具集、快捷键、编辑器状态机、就地试玩流程、`animj-dev` CLI 契约**。
> 其它主题一律引用，不复制定量：
> 坐标 / 房间网格 / 转换函数 / Z 层 / 目录布局 / 命名 → `00-conventions.md`；
> 关卡字段与校验规则 → `03`；资源路径与占位资产规格 → `04`；插件与调度 → `05`；
> 物理手感常量 → `06`；渲染管线与相机常量 → `07`；依赖版本与 feature → `09`；Bevy API 速查与黑名单 → `10`。
> 标记约定见 `00` §0.9：【已验证】/【UNVERIFIED】/【INFERENCE】。

---

## 8.1 选型结论与证据

### 8.1.1 结论

**编辑器 = 同一个 Bevy App 内的 egui 面板（宿主方案①），唯一的 UI 依赖是 `bevy_egui 0.43.1`；关卡数据仍以 `.ron` 文件为交换格式，配合 `file_watcher` 热重载（方案④）。**

```toml
# crates/editor/Cargo.toml（版本号的唯一所有者是 09，此处只固定「选哪个」）
[dependencies]
bevy_egui = "0.43.1"      # 锁定 egui 0.36（由 bevy_egui 决定，不要单独指定 egui）
level_format = { path = "../level_format" }
game = { path = "../game" }
```

`bevy_egui 0.43.1` 默认 feature 集已含 `render` / `picking` / `default_fonts` / `manage_clipboard` / `open_url` / `bevy_ui`，**无需额外开启 feature**【已验证】<https://docs.rs/crate/bevy_egui/0.43.1/features>。
**不要**在 `Cargo.toml` 里直接写 `egui = "0.37"`：`bevy_egui 0.43.1` 约束 `egui ^0.36`，直接上 0.37 会与 `bevy_egui` 冲突【已验证】<https://docs.rs/crate/bevy_egui/0.43.1/features>、<https://docs.rs/crate/bevy_egui/latest>。

### 8.1.2 候选方案对比

| 方案 | 与 Bevy 0.20 兼容性【已验证】 | 就地试玩 | 结论 |
|---|---|---|---|
| ① `bevy_egui 0.43.1` 内嵌 | 依赖 `bevy_app/bevy_ecs/bevy_camera/bevy_window/bevy_winit … ^0.20.0`（2026-10-10 发布） | ✅ 同一 App、同一 winit 事件循环、同一 `World` | **选用** |
| ② 独立 `eframe` 进程 + 共享 `level_format` | `eframe 0.37.0`（2026-10-15）与 bevy 无依赖关系，同代 winit 0.30，**能编译但拿不到就地试玩** | ❌ 需另起引擎进程 / 文件交接 / IPC | 否 |
| ③a `bevy-inspector-egui` | 最新 0.37.0 依赖 `bevy_* ^0.19.0` + `bevy_egui ^0.40` + `egui ^0.34` → **落后一个 Bevy 大版本，且 egui 0.34 vs 0.36 硬冲突** | — | **不可用** |
| ③b `bevy_editor_pls` | 最新 0.10.0（**2024-10-14**）依赖 `bevy ^0.14` / `egui ^0.28` → 落后 6 个大版本，实质停更 | — | **不可用** |
| ④ 纯文本 `.ron` 手编 + `file_watcher` | 机制可用，但无可视化编辑 | — | 作为①的**数据层补充**保留 |

证据 URL：
- `bevy_egui 0.43.1` 依赖表（全部 `bevy_* ^0.20.0`，`egui ^0.36`）：<https://docs.rs/crate/bevy_egui/latest>
- `bevy-inspector-egui 0.37.0` 依赖表（`bevy_* ^0.19.0`、`bevy_egui ^0.40.0`、`egui ^0.34`）：<https://docs.rs/crate/bevy-inspector-egui/latest>
- `bevy_editor_pls 0.10.0` 依赖表（`bevy ^0.14`、`egui ^0.28`，发布日期 2024-10-14）：<https://docs.rs/crate/bevy_editor_pls/latest>
- `eframe 0.37.0`：<https://docs.rs/crate/eframe/latest>

### 8.1.3 三条「不可用 / 不选」的明确理由（写进团队记忆，避免半年后有人重新提）

1. **`bevy-inspector-egui` 在本项目不可用**：它 0.37.0 把整套 bevy 生态钉在 `^0.19.0` 且要求 `bevy_egui ^0.40`，而 Bevy 0.20 链上的 `bevy_egui` 是 0.43 且要 `egui ^0.36`。**同一依赖图内二者无法共存**（`egui 0.34` 与 `egui 0.36` 不可同时满足）。它也不会提供任何本项目需要的瓦片/房间视图（其价值是通用反射面板，本项目已有 `03` 的强类型格式与本文的专用面板）。
2. **`bevy_editor_pls` 在本项目不可用**：最后兼容版本是 bevy 0.14，**2024-10 之后未再更新**，落后 6 个大版本；启用它等于把引擎版本退回 0.14，与 D1（Bevy 0.20.0）直接冲突。
3. **不选独立 `eframe` 进程**：本项目编辑器的**第一目标是「快速搭平台 + 就地试玩」**（D10）。独立进程只能编辑数据、无法把未落盘的编辑缓冲直接推进引擎，必须「存盘 → 另起进程 → 启动完整 `animj` → 等待 → 人工退出 → 回到编辑器」，丢掉即时性；并且要维护两套窗口/GPU 初始化与两套 UI 生命周期。方案①用同一个 `App` 与同一个 `World`，编辑态与试玩态只是状态切换，**数据零拷贝、零序列化往返**。

### 8.1.4 编辑器不做的事（防止范围蔓延）

- **不在编辑器内注册 `07` 的像素完美双层管线**：编辑器用**单个** `EditorCamera` 直接渲染游戏世界（§8.4）。像素完美（离屏纹理 + 整数倍放大 + 黑边）的验收在发布 bin `animj` 上做；需要看像素管线效果时用 `animj` 打开同一个关卡文件（加载入口见 `05`）。但 `SnapPlugin`（`Aabb→Transform` 投影 + 像素吸附）**必须**注册——否则试玩时角色贴图停在原地（§8.2.2）。
- **不做多关卡工程管理 / 关卡选择 UI**（`01` §里程碑：先做透单关卡）。
- **不做撤销栈以外的版本管理**：不用 git 集成交互，只写工作区文件。

---

## 8.2 进程与 crate 架构

### 8.2.1 三个可执行体（目录布局见 `00` §0.6）

| bin | crate | 组成 | 是否进发布 |
|---|---|---|---|
| `animj` | `crates/game` | `DefaultPlugins`（按 `07` 配置）+ `game` 的全部玩法插件 | ✅ 唯一发布物 |
| `animj-editor` | `crates/editor` | `DefaultPlugins` + `game` 的玩法插件 + `bevy_egui` + 编辑器插件 | ❌ dev only |
| `animj-dev` | `crates/editor` | **只依赖 `level_format` + `std`**，不构造 `App` | ❌ CI only |

### 8.2.2 `animj-editor` 如何复用 `game` 构建 App

- `crates/game` MUST 暴露一个**只注册玩法插件**的 App 构建入口（最终函数名以 `05` 为准；本文按下述形态使用）：

```rust
// crates/game/src/lib.rs —— 由 05 定义并拥有
/// 注册全部玩法逻辑插件。**不含** DefaultPlugins / 窗口 / 资产源 / 编辑器任何东西。
/// MUST NOT 引用 EditorMode、bevy_egui 或任何编辑器类型。
pub fn add_game_plugins(app: &mut App);
```

- 两个 bin 的差异**只允许**出现在「外层壳」：

```rust
// crates/editor/src/bin/animj_editor.rs（示意，形状以 05/09 为准）
fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())   // 与 07 一致，不重写常量
            .set(WindowPlugin { primary_window: Some(editor_window()), ..default() }),
    )
    .add_plugins(EguiPlugin::default())            // 【已验证】
    .add_plugins(EditorPlugin);
    game::add_game_plugins(&mut app);              // ← 复用，不复制
    app.add_plugins(game::render::snap::SnapPlugin); // AABB→Transform 投影（试玩必需）
    app.run();
}
```

- 编辑器**不**注册 `PixelRenderPlugin`（双层相机/离屏纹理，`08` §8.1.4），但**必须**注册
  `SnapPlugin`：试玩时角色的 `Aabb` 要投影到 `Transform`（`07` §7.6.3），否则贴图停在原地不动。

- **硬约束**：`crates/game` MUST NOT 依赖 `bevy_egui`（`00` §0.6）也不得依赖 `crates/editor`。依赖方向永远 `editor → game → level_format`，且 `level_format` 不依赖 bevy。

### 8.2.3 「编辑器代码不得进入发布 bin」的强制手段

| 层级 | 约束 | 检查方式（CI，见 `09`） |
|---|---|---|
| crate 图 | `game` 不依赖 `editor`/`bevy_egui` | `cargo tree -p game -e normal \| findstr /i egui` MUST 无输出 |
| 构建 | 发布只构建 `animj` | 发布命令固定为 `cargo build --release -p game --bin animj`（**不要**用全 workspace 构建） |
| 源码 | `crates/game/src/**` 与 `crates/level_format/src/**` 中不得出现 `bevy_egui` / `egui` 字样 | CI 文本检查 |
| 源码 | `crates/editor/src/bin/animj_dev.rs`（`animj-dev` 的实现）不得出现 `use bevy` | CI 文本检查（保证 CLI 能在无 GPU 环境跑，§8.10） |
| 类型 | `game` 的公开 API MUST NOT 出现编辑器专有类型（`EditorMode`、`EditorSession`…） | 评审项 |

---

## 8.3 状态模型：`EditorMode` 与 `GameState` 的关系

### 8.3.1 两个状态机，职责不重叠

```rust
/// 编辑器独有（定义在 crates/editor）。编辑态 / 试玩态。
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorMode {
    #[default]
    Editing,
    Playing,
}
```

- `EditorMode` **只属于编辑器**，用 `app.init_state::<EditorMode>()` 注册（`init_state` / `OnEnter` / `OnExit` / `in_state` 均为 0.20 现行 API【已验证】<https://docs.rs/bevy/0.20.0/bevy/state/index.html>）。
- **`game` 不知道 `EditorMode` 的存在**。`game` 只认它自己的 `GameState`（变体与转换由 `05` 拥有）。
- 关系一句话：**`EditorMode` 负责「谁来驱动世界」，`GameState` 负责「玩法走到哪一步」**。

### 8.3.2 转换规则

| 事件 | 编辑器动作 | `GameState` 的结果 |
|---|---|---|
| 启动 | `EditorMode::Editing`（默认） | `GameState::Paused`（见 `05` §5.5 的门控规则） |
| `Ctrl+S` 保存 | 见 §8.7 | 不变 |
| 进入试玩（`F5`） | 见 §8.6 | `GameState::Playing` |
| 退出试玩（`F5`/`F6`） | 见 §8.6 | `GameState::Paused` |

**关键机制（零侵入）**：编辑态下 `GameState` 停在 `Paused`，因此 `game` 里所有以 `in_state(GameState::Playing)` 门控的系统**天然停摆**——编辑器**不需要**给任何 `game` 系统加 `run_if`，也**MUST NOT** 这么做（那会让 `game` 反向知道编辑器存在）。
【INFERENCE】依据：`in_state` 是标准 run condition【已验证】<https://docs.rs/bevy/0.20.0/bevy/state/index.html>；「当前状态不是 `Playing` ⇒ 以 `in_state(GameState::Playing)` 门控的系统不运行」是其定义。

### 8.3.3 系统门控矩阵

| 系统组 | schedule | run condition |
|---|---|---|
| 编辑器 UI 面板（egui） | `EguiPrimaryContextPass` | `in_state(EditorMode::Editing)`（试玩态只保留一条「停止试玩」栏，见 §8.6） |
| 编辑器世界输入 / 快捷键 | `EguiPrimaryContextPass`（**排在面板之后**） | `in_state(EditorMode::Editing)` **且** 同一帧的 `ctx.egui_wants_pointer_input()` / `egui_wants_keyboard_input()` 为假。放在 `Update` 会读到上一帧布局 ⇒ 点在按钮上的那次点击会漏到画布（实机：按钮要点两次） |
| 编辑器世界输入（工具） | `Update` | `in_state(EditorMode::Editing)` **且** `not(egui_wants_any_pointer_input)`（§8.4.4） |
| 编辑器快捷键 | `Update` | `not(egui_wants_any_keyboard_input)`（§8.9） |
| 编辑器 Gizmos 叠加 | `Update` | `in_state(EditorMode::Editing)` |
| 热重载消费 | `Update` | 无（两种模式都要监听，§8.7.4） |
| `game` 的全部系统 | 由 `05` 决定 | 由 `05` 决定，**编辑器不改** |

> Gizmos 必须在 `Last` **之前**提交才会被绘制【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Gizmos.html>（"Gizmos should be spawned before the `Last` schedule"），因此编辑器绘制系统放在 `Update`。

---

## 8.4 相机与输入

### 8.4.1 编辑态只有一台相机

```rust
#[derive(Component)]
struct EditorCamera;

// 启动时
commands.spawn((
    Camera2d,                                    // 0.20 是 unit component，Camera2dBundle 已不存在【已验证】
    EditorCamera,
    Msaa::Off,                                   // 见下
    PrimaryEguiContext,                          // egui 宿主，显式指定，不靠自动挑选
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::WindowSize,
        scale: 1.0,                              // 1 世界单位 = 1 逻辑像素【已验证】
        ..OrthographicProjection::default_2d()
    }),
));
```

- `Msaa::Off` **必须显式插**：0.20 的 `RenderPlugin` 把 `<Camera, Msaa>` 注册为 required component，`Msaa::default() == Sample4` ⇒ 每台相机默认带 4×MSAA，像素风必须关掉【已验证】<https://docs.rs/bevy/0.20.0/bevy/render/view/enum.Msaa.html>。
- `Tonemapping` / `DebandDither` / `Hdr`：**编辑器不设置**，保持 0.20 的 2D 默认（`Tonemapping::Linear`）；不要给相机加 `Hdr`【已验证】<https://bevy.org/learn/migration-guides/0-19-to-0-20/>。
- `PrimaryEguiContext` **必须显式加在 `EditorCamera` 上**：`bevy_egui` 的自动装配是「找第一台被发现的相机」，编辑器里这个不确定性不可接受【已验证】<https://docs.rs/bevy_egui/0.43.1/bevy_egui/struct.PrimaryEguiContext.html>（该页示例即 `commands.spawn((Camera3d::default(), PrimaryEguiContext))`）。想彻底关掉自动装配可设 `EguiGlobalSettings::auto_create_primary_context = false`【已验证】同页。
- **不注册 `07` 的离屏管线相机**（§8.1.4）。`07` 拥有的一切相机与缩放常量在编辑器里**不出现**。

### 8.4.2 缩放与平移

| 项 | 规则 |
|---|---|
| 缩放载体 | 改 `OrthographicProjection::scale`，**不改 `ScalingMode`、不改 `Transform::z`**【已验证】"For simplicity, zooming should be done by changing `OrthographicProjection::scale`" <https://docs.rs/bevy/0.20.0/bevy/camera/struct.OrthographicProjection.html> |
| 缩放档位 | `EDITOR_ZOOM_STEPS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0]`（**放大倍数** `magnify`）；`scale = 1.0 / magnify` |
| 初始档位 | `magnify = 1.0`（`scale = 1.0`）：一屏 ≈ 一个房间，便于编辑整间房（游戏内视野更小，见 `07` §7.1.1） |
| 光标锚点缩放 | `Shift+滚轮` 前进一档 / `Shift+滚轮` 后退一档（档位之间切换，不做连续缩放） |
| 平移 | 中键拖动，或 `Space`+左键拖动；每帧 `translation += -delta_logical_px * scale` |
| 平移取整 | `translation.xy` MUST 量化到**当前 `scale` 的整数倍**（= 屏幕像素对齐）：`(t / scale).round() * scale`。早期按 0.5 量化在 1× 下仍落在半像素上，Gizmos（网格/高亮边框）逐帧抖动，只有放大到 2× 才消失（实机现象，已修）。缩放/平移/`Home`/`Tab`/点击定位四条路径都要量化 |
| 视角复位 | `Home` → 相机移到当前房间中心（中心公式见 `00` §0.3）、`magnify = 1.0`、平移取整 |
| 切换房间 | **右栏「房间」点击式房间选择器**（`cols × rows` 的按钮格）是**权威入口**，切换**只改编辑目标、不移动相机**（放置瓦片时视角不被拽走）；需要时点同行的「对焦」按钮，试玩态仍跟随玩家。键盘 `Tab` / `Shift+Tab` / `[` / `]` 为快捷方式；⚠️ `Tab` 会被 egui 焦点遍历吞掉，不能作为唯一入口（实机 bug："网格永远只在房间 1"） |

**光标锚点缩放的精确算法**（`viewport_origin` 默认为 `(0.5, 0.5)` ⇒ 相机位置即视口中心【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.OrthographicProjection.html>）：

```
// 记 cursor 为窗口逻辑像素坐标，win 为窗口逻辑尺寸
before_world = camera.viewport_to_world_2d(&cam_gt, cursor)?;     // 缩放前光标世界点
new_scale    = 1.0 / next_magnify;
// 【INFERENCE】由「viewport_origin = 中心 + scale 为世界单位/逻辑像素比」推出：
new_translation = before_world - (cursor - win / 2.0) * new_scale;
```

- 上面的解析式是 **【INFERENCE】**（由已验证的两条语义推出，非文档直述）。因此**必须**加一条自检：缩放发生后的**下一帧**断言 `camera.viewport_to_world_2d(&cam_gt, cursor)` 与 `before_world` 的距离 < `1.0` 世界像素；`debug_assert!` + 一次人工实测即可固化。
- **禁止**在同一系统里「先写投影/变换、再调用 `viewport_to_world_2d` 期望立刻反映」：`GlobalTransform` 由 Bevy 的变换传播在固定 schedule 中写入（【已验证】`GlobalTransform` 的世界同步在 `PostUpdate`，见 `10`），同帧读到的是上一帧的值。

### 8.4.3 光标 → 房间 / 瓦片

```rust
// 唯一允许的换算路径：Camera 的视口换算 + 00 §0.3 的 world_to_room
let cursor: Vec2 = window.cursor_position()?;                     // 逻辑像素【已验证】
let (camera, cam_gt) = editor_camera.into_inner();
let world: Vec2 = camera.viewport_to_world_2d(cam_gt, cursor)?;   // 【已验证】签名
let (col, row, local) = game::world::coords::world_to_room(world, cols, rows);  // 00 §0.3 唯一入口
let file_row = (local.y / TILE_PX).floor() as i32;   // 房间局部 +Y 向下（00 §0.3）
let tx       = (local.x / TILE_PX).floor() as i32;
let ty       = ROOM_TILES as i32 - 1 - file_row;     // 00 §0.3
```

- 精确签名【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Camera.html>：
  `Camera::viewport_to_world_2d(&self, camera_transform: &GlobalTransform, viewport_position: Vec2) -> Result<Vec2, ViewportConversionError>`
- `Window::cursor_position(&self) -> Option<Vec2>`（逻辑像素）【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.Window.html>。
- **禁止**在编辑器里手写 `00` §0.3 的两个换算公式，只能调 `world_to_room` / `room_local_to_world`。
- 瓦片读写 MUST 通过 `level_format` 的（行主序）访问器；若 `03` 未提供访问器，则在编辑器内封一个且**仅此一个** `fn tile_index(file_row: usize, tx: usize) -> usize`。**禁止**分散的索引算术。

### 8.4.4 egui 吞输入：竞争与抢占策略

**机制事实（全部【已验证】）**

| 事实 | 证据 |
|---|---|
| `EguiContexts::ctx_mut() -> Result<&mut egui::Context, QuerySingleError>`；UI 系统写成返回 `Result` 的形式 | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/struct.EguiContexts.html>、crate 根示例 <https://docs.rs/bevy_egui/0.43.1/bevy_egui/> |
| 所有编辑器 UI 系统加在 `EguiPrimaryContextPass` 这个 `ScheduleLabel` 上 | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/struct.EguiPrimaryContextPass.html> |
| `EguiWantsInput` 资源提供 `wants_pointer_input()` / `is_using_pointer()` / `is_pointer_over_area()` / `wants_keyboard_input()` / `is_context_menu_open()` / `wants_any_pointer_input()` / `wants_any_keyboard_input()` / `wants_any_input()` | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/input/struct.EguiWantsInput.html> |
| 现成 run condition：`egui_wants_any_pointer_input` / `egui_wants_any_keyboard_input` / `egui_wants_any_input`（签名形如 `fn(Res<EguiWantsInput>) -> bool`） | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/input/index.html> |
| `absorb_bevy_input_system`：当 egui 想要输入时，**清空** `ButtonInput<MouseButton>` / `ButtonInput<KeyCode>` 并清空 `KeyboardInput` / `MouseWheel` / `MouseButtonInput` 消息缓冲 | 同上（函数列表） |
| 输入系统的开关默认全开（`EguiInputSystemSettings`："All the systems are enabled by default"） | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/struct.EguiInputSystemSettings.html> |
| egui 的输入翻译系统跑在 `PreUpdate`（`EguiPreUpdateSet`） | <https://docs.rs/bevy_egui/0.43.1/bevy_egui/> |

**因此策略固定为下面 5 条：**

1. **世界输入（鼠标工具）MUST 加 `not(egui_wants_any_pointer_input)`**。它是官方提供的 run condition，比自己查 `is_pointer_over_area()` 更保守（连「egui 正在用指针拖滑块」都算）。
2. **键盘快捷键 MUST 加 `not(egui_wants_any_keyboard_input)`**。这条同时解决了「焦点在 egui 文本框里时按 `1` 不该切工具」的问题——`wants_keyboard_input()` 的定义就是「egui 正在监听文本输入」。
3. **不依赖 `absorb_bevy_input_system` 的清理结果**：它是「egui 想要输入 ⇒ 把 Bevy 侧输入清空」，属于最后一道保险；编辑器的正确性 MUST 由第 1、2 条的 `run_if` 保证。**禁止**为了让工具「感觉更灵敏」而关掉输入系统（`EguiInputSystemSettings`）。
4. **拖拽所有权一旦取得就不再被 egui 抢走**（否则中途划过面板会丢操作）：

```rust
#[derive(Resource, Default)]
struct EditorDrag {
    active: Option<DragOp>,   // 工具、起始单元格、起始世界点、已应用的操作集合
    button: MouseButton,
}
```
   - 当 `active.is_none()` 时，只有「指针不在 egui 区域上」才会开始新拖拽。
   - `active.is_some()` 时，**忽略** `egui_wants_any_pointer_input`，继续本次拖拽，直到按键释放（`just_released`）→ 提交为**一个** undo 步骤（§8.5.2）。
   - 指针离开窗口（`cursor_position() == None`）⇒ 取消拖拽并回滚到拖拽起点快照。
5. **试玩态一律不响应编辑器世界输入**：`in_state(EditorMode::Editing)` 已在门控矩阵里；`F5` 之外的编辑器快捷键在试玩态**全部 no-op**（`Ctrl+S` 也 no-op，见 §8.6.4）。

**优先级总结表**（同一帧内谁赢）：

| 情形 | 赢家 | 说明 |
|---|---|---|
| 指针在 egui 面板上，无进行中的拖拽 | egui | 面板可点，世界工具不触发 |
| 有进行中的拖拽，指针移出世界区 | 编辑器拖拽 | 保持到鼠标释放 |
| 有 egui 文本框获得焦点 | egui | 快捷键失效；`Esc` 先由 egui 处理（失焦） |
| egui 弹出菜单/下拉打开 | egui | `egui_wants_any_pointer_input()` 覆盖该情形 |
| 试玩态 | `game` 的输入系统 | 编辑器工具与快捷键停摆 |

---

## 8.5 工具集

### 8.5.1 工具一览

| 键 | 工具 | 触发 | 作用对象 | 结束条件 |
|---|---|---|---|---|
| `1` | `TileBrush` 瓦片笔刷 | 左键按下并拖动 | 当前房间的瓦片 | 松开左键 ⇒ 提交 1 步 |
| `2` | `Eraser` 擦除 | 左键按下并拖动 | 瓦片 → 置为 `Empty`（`03` 的瓦片种类枚举） | 松开左键 |
| `3` | `RectFill` 矩形填充 | 左键拖出矩形 | 矩形内瓦片全部置为当前笔刷种类 | 松开左键；`Esc` 取消 |
| `4` | `Picker` 吸管 | 单击 | 读取该瓦片种类并设为当前笔刷种类 | 单击即结束（不产生 undo 步骤） |
| `5` | `PlaceEntity` 实体放置 | 单击 | 房间的实体列表（`03` 的实体条目） | 单击即结束（1 步） |
| `6` | `MoveEntity` 实体拖动 | 在实体上按下并拖动 | 选中实体的位置 | 松开左键 |


- `Alt` + 左键：**任意工具下临时吸管**（松开 `Alt` 回到原工具）。这是唯一允许的工具临时覆盖。
- `Ctrl` 按住：**关闭吸附**（对 `5`/`6` 有效，位置对齐到整世界像素）。默认吸附规则见 §8.5.4。
- `Shift` 按住：`3` 矩形填充约束为正方形；`1` 保持直线（只沿起点所在行列扩展，用于画细长平台）。
- 工具切换 MUST **只改变「下一个手势用哪个工具」**，不得中途改变正在进行的拖拽。

### 8.5.2 编辑缓冲与撤销（工具操作的统一状态）

```rust
#[derive(Resource, Default)]
struct EditorSession {
    path: Option<PathBuf>,          // 当前关卡文件
    doc: Level,                     // ★ 编辑数据的唯一真相
    undo: Vec<Level>,               // 全量快照栈，上限 UNDO_LIMIT
    redo: Vec<Level>,
    dirty: bool,                    // doc 相对 saved_text 是否有改动
    saved_text: Option<String>,     // 最后一次写盘/加载的 RON 文本（热重载回环抑制，§8.7.4）
    file_issues: Vec<LevelIssue>,   // 校验结果（§8.8）
    current_room: (u32, u32),
    selection: Selection,           // 房间集合 + 每个房间的瓦片矩形
    tool: Tool,
    brush: TileKind,
    pending_external: Option<Level>, // 待用户确认的外部变更
}
```

- **`doc` 是唯一真相**；世界里的编辑态实体只是 `doc` 的**投影**。任何工具操作 = 「改 `doc` → 把受影响的投影标脏 → 下一帧重建」。
- 撤销/重做用**全量快照**（一个关卡的数据量级 = 房间数 × 房间瓦片数 字节，远小于一张贴图；简单、绝对正确、无「命令重放」的不一致风险）。`UNDO_LIMIT: usize = 128`（常数由本文拥有）。
- **合并规则**：一次拖拽 = 一个 undo 步骤（在拖拽开始时 push 快照，拖拽过程中直接改 `doc`）；面板里连续拖动的数值框（如 `Collect(n)` 的 n）= 每次落焦一个步骤。
- 工具被 egui 抢占而中断 ⇒ 回滚到拖拽起点快照，**不**产生 undo 步骤。

### 8.5.3 各工具的具体行为

**`TileBrush` / `Eraser`**
- 拖动时对**穿过**的每个瓦片生效（用 Bresenham/超采样，避免快速拖动漏格）：取相邻两次光标位置之间的线段，按 `TILE_PX / 4` 步长采样并去重。
- 只写「当前房间」的瓦片；**在别的房间里起手**会先把当前房间切过去（否则表现为"只能编辑第一个房间"，实机 bug），**拖拽中途**跨房间则停止写入但拖拽保持（松开后结束）。
- `Shift` 限制为直线：以按下时所在格为锚，只沿 x 或 y（偏向更大的一轴）扩展。
- **每写入一格都必须推进 `doc_revision`**（不只是 `begin_edit` 那一帧）：投影只在修订号变化时重建，否则长按拖动时数据已改但画面不刷新，必须再点一次才看见（实机 bug）。

**`RectFill`**
- 矩形由「按下格」与「当前格」确定，先 `min`/`max` 归一化再写入。
- `Shift` 约束正方形（边长取两轴较小者）。
- 预览：拖拽中用瓦片高亮；矩形边界 MUST 取整到世界像素（否则逐帧抖动）。

**`Picker`**
- 读取 (a) 瓦片种类、(b) 若点在实体上则读取该实体类型（同时把实体列表选中项切过去），(c) 若点在空瓦片且无实体则把笔刷设为 `Empty`。
- 不产生 undo 步骤、不标 `dirty`。

**`PlaceEntity`**
- 面板列表来自 `03` 的实体类型枚举（当前为 `fruit` / `exit` / `checkpoint` / `player_spawn`）；放置前 MUST 调 `03` 的实体构造器/校验（例如 AABB 尺寸、必需字段有默认值），**编辑器不得自己拼一个「半合法」的实体**。
- 默认吸附到瓦片中心；`Ctrl` 关闭吸附。
- 放置即选中新实体，并在面板里展开其属性。

**`MoveEntity`**
- 命中测试用**逻辑 AABB**（`00` §0.3「逻辑真相永远是 `Aabb`」），把该实体的 AABB 从世界像素换算到房间局部后与光标矩形求交。
- 拖动时实体位置**吸附到瓦片中心**（`Ctrl` 关闭），实时更新 `doc` 里该实体的 `at` 字段（房间局部坐标，`00` §0.3 的实体位置约定）。
- 位置真的变化时必须推进 `doc_revision`（拖动不经过 `begin_edit`），否则投影不重绘、看起来"拖不动"（实机 bug）。
- 只在同一个房间内可拖动（跨房间拖拽 ⇒ 取消并回滚）。
- 实体按房间管理：右栏只列出**当前房间**的实体（列表随房间选择器切换），放置/删除都作用于当前房间。

### 8.5.4 吸附规则（唯一一处）

| 工具 | 默认 | `Ctrl` 按住 |
|---|---|---|
| `TileBrush` / `Eraser` / `RectFill` | 吸附到瓦片格（始终） | 不适用（瓦片工具永远格对齐） |
| `PlaceEntity` / `MoveEntity` | 吸附到瓦片**中心** | 对齐到整世界像素（`round()`） |

> 编辑器**不**引入「像素级吸附开关」之类的第二套设置；`Ctrl` 是唯一开关。这保证任何时刻画面里都只有「格对齐」和「整像素」两种位置。

### 8.5.5 Grid 与叠加（**屏幕空间 egui Painter**）

> **实机修正（2026-10-17）**：早期用 2D `Gizmos` 绘制网格/边框。实机反复出现**逐帧闪烁**（`G` 关掉网格即停，放大到最大才缓解）；
> 依次排除了：相机半像素、整像素边界、`grid_2d` 内部几何、逐房间全量绘制。最终改为 **egui `Painter` 屏幕空间绘制**：
> 世界坐标经 `Camera::world_to_viewport` 投影后**四舍五入到整数屏幕坐标**再画线 ⇒ 几何不随帧变化，绘制路径上不可能产生亚像素抖动。

| 项 | 规则 |
|---|---|
| 绘制位置 | `EguiPrimaryContextPass` 的 `paint_overlay`，`LayerId::background()`（位于各面板之下、世界之上） |
| 坐标 | **不要用 `GlobalTransform`**（变换传播在 `PostUpdate`，egui 通行读到上一帧矩阵 ⇒ 切房间后网格停在旧位置）。用 `Transform` + `scale` 解析换算：`screen = size/2 + (world - cam)/scale`（y 取反）后 `round()` |
| 自愈 | `current_room` 变化 ⇒ 相机必须跟到新房间中心（`keep_camera_on_current_room`，用 `Local` 记住 (mode, room)）；试玩态跟 **game 的** `CurrentRoom` |
| 可见性 | 只画与「屏幕四角反算出的世界矩形」相交的房间（`08` §8.5.5 的"只画可见房间"） |
| 开关 | `G` 网格 / `Shift+G` 房间边框；当前房间高亮常开 |
| 线宽 | 网格 1 px、边框/高亮 2 px（屏幕像素） |
| 禁止 | 为叠加层新增 sprite（`00` §0.4 的 Z 表），以及在 2D 世界空间画 1 px 线（闪烁） |

颜色常量（`view.rs`，不透明度 1.0，靠描边而非半透明填充看见下方）：

| 常量 | 值 | 用途 |
|---|---|---|
| `GRID_COLOR` | `srgb_u8(58, 58, 66)` | 瓦片网格 |
| `ROOM_BORDER_COLOR` | `srgb_u8(120, 120, 140)` | 房间边界 |
| `HOVER_COLOR` | `srgb_u8(255, 240, 130)` | 当前房间 |

- 房间中心/世界 AABB 一律用 `00` §0.3 的 `room_local_to_world` 与中心公式，**禁止**手写算术。
- **投影增量重建**：所有房间在首次加载时建好；**切房间不重建**，只有 `doc_revision` 变化（当前房间被编辑）才重建当前房间。顶栏常驻 `fps` 与 `proj`（投影房间数）便于观察。
- 验证：编辑器静止时取两帧截图在世界区逐像素比对必须 **0 差异**。

### 8.5.6 egui 面板布局（编辑器 UI 的固定骨架）

| 位置 | 内容 |
|---|---|
| 顶栏 | 文件路径、`*` 脏标记、校验计数（`N errors / M warnings`）、当前房间 `(col, row)`、当前工具、`magnify` |
| 左栏 | **工具选择**（§8.5.1 的 6 个，中文标签）+ **带 32×32 缩略图的瓦片选择**：按当前栏宽**显式计算列数**（`floor((可用宽 + 间距) / (单项最小宽 + 间距))`，单项最小宽 72 px），列数随窗口/栏宽变化；选中项黄框，末项「擦除」参与同一行列布局。缩略图按 `cell` 的 UV 裁切，**不缩放整张图集** |
| 右栏 | **只做地图与选中房间的编辑**：房间选择器、房间属性（`reward`、`goals`）、**当前房间的实体列表（每行可选中/删除）**、实体笔刷、去重；编辑态投影里 `player_spawn` 必须可见（青色 32×32 标记） |
| 底栏 | 地块标签页的「新建地块」**自动编号**（`1..9` → `a..z` → `A..Z`，跳过调色板已用字符，可手动改），名称默认 `瓦片<编号>`；保存/试玩/撤销/重做 + 路径、**「地块」/「动画」/「背景」三个标签页**（地块调色板、角色动画、背景视差图层）、外部改动确认。标签页下方**横向分区**：左 = 当前工具（地块/动画面板内部再分左右两栏），右 = 校验问题列表（固定宽、独立滚动、点击定位到房间/格），不再把错误列表堆在工具内容下方。底栏顶部有**自绘竖直分隔条**（`BottomHeight` 资源，`exact_size` 固定高度），向上拖即展开、永远吸附窗口底边；内置 `Panel::resizable` 在自定义根 Ui 下会让面板脱离底边，**禁止使用** |
| 浮层 | 试玩态只保留一个固定浮层：`[停止试玩 F5]` + 「试玩中：编辑缓冲已冻结」提示 |

- 所有面板只读 `EditorSession`，所有修改经**同一个** `EditorCommand` 提交函数（统一 push undo 快照），**禁止**在 UI 代码里直接改 `doc` 的字段。
- 面板外框尺寸（左右栏宽度等）不写进规范，属实现细节；但**禁止**面板遮挡整个窗口——世界区必须始终可见（拖拽才能工作）。
- **点击归属**：面板系统必须排在世界输入之前（`EditorUiSet::Panels.before(WorldInput)`），否则 `egui_wants_pointer_input()` 读到上一帧布局，点按钮会漏到画布。
- **自定义贴图必须同步加载**：编辑器进程里运行期 `AssetServer::load::<Image>` 的新图像会永远停在 `Loading`（启动期请求的则正常，引擎侧原因未定位）。`game::render::tiles::load_image` 直接读盘 + `Image::from_buffer` 插入 `Assets<Image>`；编辑器 UI 与关卡投影都用它。改完贴图路径必须 bump `doc_revision`，否则投影不重建。
- **关卡校验失败也要能编辑**：`LoadedLevel` 只在校验通过时出现；编辑器等 30 帧后直接读 RON 打开，并把校验问题放进底栏列表，否则会静默停在默认文档上（实机 bug：改贴图路径作用于默认文档）。
- **调色板物化**：`Level::palette()` 在 `palette` 为空时才回退内置；**任何新增地块前必须先 `doc.palette = doc.palette()` 物化内置 4 种**，否则内置瓦片会从选择器里整体消失（实机 bug）。
- **中文字体**：egui 内置字体无 CJK 字形（中文显示为 `□`）。启动时把系统中文 TTF（`simhei.ttf` → `Deng.ttf` → `simkai.ttf` / `simfang.ttf`）注册为 Proportional/Monospace 首选；**只用 `.ttf`**，`.ttc` 字体集合 `ab_glyph` 不支持。找不到时打印 error 并保持方块（不 panic）。

---

### 8.5.7 内容工具（三组）

#### A. 动画编辑（`assets/anim/player.anim.ron`，见 `04` §5.5）

| 区域 | 内容 |
|---|---|
| 布局 | 顶行 = 标题 + 脏标记 + 保存/重载；下方**左右两栏**：左 = 当前状态贴图路径/列数 + 实时预览 + 预览状态选择，右 = 从贴图选帧 + 状态/帧列表。避免把控件与按钮纵向堆到底 |
| 状态列表 | 4 个状态各一行：名称、帧序列、帧时长、循环开关 |
| 帧序列 | 以**图集网格**展示所有格（`columns × rows`），点选/拖拽增删帧；当前帧高亮；可直接输入索引 |
| 预览 | ✅ 已实现：`SheetTexture` 把角色图集一次性上传为 egui 纹理（`ColorImage` + `TextureOptions::NEAREST`），面板按当前状态/时长/循环**实时播放**并显示帧号与槽位；可点状态名切换预览。单格裁切必须关闭 `maintain_aspect_ratio`（否则非方形图集会按整图宽高比把预览压扁） |
| 保存 | 面板内 `Save animation`：序列化 → 校验 → `.tmp` → rename；校验失败（空帧序列、越界索引、时长越界）**拒绝写盘**。已实现并有单测（含"校验失败时文件不存在"） |
| 生效 | 保存后走 `AssetEvent` 热重载（`game::anim_data` 采纳），游戏立即用新数据；编辑器面板有 `Reload from disk`。启动时若资产尚未异步加载完成，编辑器**直接读盘**建立工作副本（缺失/非法时才回退内置默认）；资产加载完成后若用户没有未保存改动，编辑器自动同步到实际生效的数据，避免停留在默认 `cat_sheet.png` |

#### B. 新增地块（`03` §3.9 的 v2 palette）

| 区域 | 内容 |
|---|---|
| 布局 | **左右两栏**：左 = 贴图路径/列数 + 现有地块列表（每个地块一张卡片；按栏宽显式计算列数，单卡最小宽 200 px，卡片等宽横向排列），右 = 选中地块的行为/贴图格/选格图 + 新建地块表单；图集按栏宽自适应缩放 |
| 贴图 | 路径输入（相对 `assets/`）+ `columns` 数字框（写入 `Level.tileset` / `tileset_columns`）；图集以 `TileTexture` 上传为 egui 纹理 |
| 地块列表 | 每个地块一张卡片：缩略图、字符/名称、行为、贴图格；按栏宽自适应列数横向排列。选中后可改行为/贴图格，`del` 删除（已实现；每次修改一个 undo 步骤） |
| 新建地块 | 字符 + 名称 + 行为 + 贴图格；字符重复 / `.` / 空名由校验拦截（已实现） |
| 选格 | 32×32 网格显示**贴图缩略图**（`columns × 4`），点图设格、当前格黄框高亮；无纹理时回退数字按钮 |
| 画布 | 笔刷面板直接列出**当前关卡 palette** 的全部地块（已实现：笔刷是**字符**，可画自定义地块；`Alt` 吸管按字符取色） |
| 保存 | 仍是一个 undo 步骤；校验（V6/V19）失败时按 `08` §8.8.3 处理（Error 允许保存，Fatal 拒绝） |

#### C. 背景视差（`03` §3.10、`07` §7.13）

| 区域 | 内容 |
|---|---|
| 图层列表 | 最多 3 层（`MAX_BACKGROUND_LAYERS`）；每层：贴图路径（文本 + `asset_picker` 浏览 `assets/` 下的 PNG）、水平/垂直滚动百分比 `0..=100`、删除 |
| 添加 | 「添加图层」在末尾追加，默认 `scroll_x = 30`、`scroll_y = 0`；空路径由 V20 标红，不阻断编辑 |
| 顺序 | 列表自上而下 = 由远到近；`layers[0]` 渲染在 `z::PARALLAX`，索引越大越靠近镜头 |
| 预览 | **实时**：`sync_background_preview` 把 `doc.background` 写进 `game::render::parallax::BackgroundOverride`，画布用 `EditorCamera` 渲染背景层；不保存、不试玩也能看到 |
| 撤销 | 所有修改走 `session.begin_edit()`，一次拖动/输入 = 一个 undo 步骤 |
| 生效 | 保存后写入 `background`；试玩通过 `enter_play` 注入的 `LoadedLevel` 走同一渲染路径 |

三份数据的编辑都遵循既有纪律：**单个序列化入口**、**一个拖拽一个 undo 步骤**、**保存前必须校验**、**热重载需显式确认**。

## 8.6 就地试玩流程

### 8.6.1 进入试玩（`F5`，`EditorMode::Editing → Playing`）

顺序**固定**，由 `OnExit(EditorMode::Editing)` + 一个 `enter_play` 系统实现：

| # | 动作 | 说明 |
|---|---|---|
| 1 | 结束进行中的拖拽 | 提交（若已生效）或回滚；`EditorDrag.active = None` |
| 2 | **校验** | 调 `level_format` 校验（§8.8）；存在 `error` ⇒ **中止**进入试玩、把错误列表滚到顶部、状态栏标红 |
| 3 | 把 `doc` 交给 `game` | 把 `doc` 写进 `Assets<Level>` 的当前 handle（`Assets::get_mut` / `insert`），使试玩走**与发布完全相同**的关卡消费路径；**不写盘** |
| 4 | 销毁编辑态实体 | 所有带 `EditorEntity` 标记的实体（瓦片投影、实体投影、轴标）批量 despawn |
| 5 | 把编辑器 UI 切到最小浮层 | 左/右/底栏隐藏，只留「停止试玩」 |
| 6 | 置 `NextState<GameState>` 为 `GameState::Playing` | 由 `game` 的 `OnEnter(GameState::Playing)` 构建世界（`05` 拥有该流程） |
| 7 | 玩家控制 | 由 `game` 的输入系统接管，**编辑器不做任何输入映射**（输入抽象见 `00` D16） |

### 8.6.2 退出试玩（`F5` 或 `F6`，`Playing → Editing`）

| # | 动作 | 说明 |
|---|---|---|
| 1 | 置 `NextState<GameState>` 为 `GameState::Paused` | 由 `game` 的 `OnExit(GameState::Playing)` 清理运行时实体（**编辑器不重复清理**） |
| 2 | `OnEnter(EditorMode::Editing)`：从 `doc` 重建编辑态实体 | 全量重建；相机 `Home` 到**试玩前所在房间** |
| 3 | 校验再跑一次 | 试玩不改变 `doc`，所以结果不变，但保证面板数字与实体一致 |
| 4 | 恢复完整面板 | 左/右/底栏重新显示 |

### 8.6.3 可逆性约束（硬规则，逐条可评审）

| # | 约束 | 落地方式 |
|---|---|---|
| R1 | 试玩期间 `EditorSession.doc` **MUST NOT** 被写入 | 试玩相关系统的参数一律 `Res<EditorSession>`（只读），不用 `ResMut`；编译期即可挡住 |
| R2 | 试玩产生的运行时状态（房间进度、奖励、玩家位置、`GameState` 变化）**MUST NOT** 回写 `doc` | `game` 无权写 `doc`（它根本不知道 `EditorSession` 存在），编辑器侧也没有回写路径 |
| R3 | 试玩结束后编辑缓冲与试玩前**逐字节相同** | 退出时 `debug_assert_eq!(serialize(doc), serialize(preplay_doc))`（`preplay_doc` 在进入试玩时按 `Level: PartialEq` 存一份克隆，只在 debug 保留） |
| R4 | undo 栈、redo 栈、`saved_text`、`dirty` 不受试玩影响 | 试玩路径只读这些字段 |
| R5 | 进出试玩**MUST NOT** 触发写盘 | 保存只有两个入口：`Ctrl+S` 与退出应用确认框（§8.7.1） |
| R6 | 编辑态实体 MUST 能**无损重建** | 所有编辑态实体由一个 `rebuild_editor_projection(commands, &doc, …)` 系统生成；禁止任何「只在第一次 spawn 时设置」的状态 |
| R7 | 相机/缩放在试玩后保留 | `EditorCamera` 与 `EditorSession` 都不随状态切换 despawn |

### 8.6.4 试玩态下的其它规则

- **`Ctrl+S` 在试玩态 no-op**（并弹一条提示）：防止把「运行中的世界快照」误当作关卡数据写盘。
- `Esc` 在试玩态 no-op（不退出试玩），避免玩家操作时误退；退出只认 `F5`/`F6` 或按钮。
- 试玩态崩溃/卡死 ⇒ 直接关窗口重开编辑器；因为 R5，磁盘上的关卡一定还是上次保存的版本（这也是「禁止自动保存」的理由）。

---

## 8.7 保存 / 加载 / 热重载

### 8.7.1 保存（写回 `.level.ron`）

```rust
fn save(session: &mut EditorSession, assets: &mut Assets<Level>) -> Result<(), SaveError> {
    let path = session.path.clone().ok_or(SaveError::NoPath)?;
    // 1) 序列化：MUST 走 level_format 提供的唯一序列化函数（与加载同一条路径，03 拥有其排版规则）
    let text = level_format::write(&session.doc)?;
    // 2) 校验：致命错误 ⇒ 拒绝写盘（见 §8.8.3）
    let issues = level_format::validate(&session.doc);
    if issues.iter().any(|i| i.severity == Severity::Fatal) { return Err(SaveError::Fatal(issues)); }
    // 3) 原子写：同目录 .tmp 后缀 + rename 替换
    let tmp = with_extra_extension(&path, "tmp");       // world_01.level.ron.tmp
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &path)?;                      // 覆盖已存在文件
    // 4) 记账：这是「自己写的」，用于回环抑制（§8.7.4）
    session.saved_text = Some(text);
    session.dirty = false;
    // 5) 同步内存资产，使下一次试玩立刻用新数据（不必等 watcher）
    if let Some(level) = assets.get_mut(&session.handle) { *level = session.doc.clone(); }
    Ok(())
}
```

| 规则 | 内容 | 理由 |
|---|---|---|
| 唯一序列化路径 | 编辑器 MUST NOT 自己拼 RON 字符串，一律调 `level_format` 的序列化函数（`03` 拥有字段顺序与排版） | 保证「加载→保存」是恒等变换，不制造无意义 diff |
| 确定性 | 同一 `Level` 值序列化两次 MUST 字节相同（`HashMap` 一律不用；所有集合用 `Vec` 且顺序稳定） | diff 友好 + 指纹比较（§8.7.4）依赖它 |
| 换行 | 输出统一 **LF**，文件以 LF 结尾；写盘前把 `\r\n` 归一到 `\n` | Windows/Linux 混合开发下 diff 稳定 |
| 原子写 | 先写 `*.ron.tmp` 再 `std::fs::rename`；**临时文件必须在同一目录**（同卷） | `rename` 替换是原子的【已验证】Rust std：<https://doc.rust-lang.org/std/fs/fn.rename.html>（Windows 上等价于 `MoveFileExW` + `MOVEFILE_REPLACE_EXISTING`）；避免 watcher 读到半个文件 |
| 临时文件名 | 扩展名 MUST NOT 是 `ron`（用 `.ron.tmp`） | `AssetServer` 只对注册过的扩展名调 loader（`03`/`05` 注册 `ron`）；`.tmp` 不会被解析成关卡。watcher 可能为未知扩展名打一条日志，属预期 |
| 保存入口 | 只有两个：`Ctrl+S`（当前路径）与 `Ctrl+Shift+S`（另存为）/退出确认框 | 见 R5：禁止自动保存、禁止试玩态保存 |
| 无路径时 | `Ctrl+S` = 弹路径输入框（默认落在 `00` §0.5 的 `assets/levels/<level_id>.level.ron`） | 编辑器不引入原生文件对话框依赖（`rfd` 不在依赖白名单内，见 `00` D15 / `09`） |

### 8.7.2 加载

- 编辑器**不自己读文件解析**：通过 `AssetServer::load::<Level>("levels/world_01.level.ron")` 拿到 `Handle<Level>`，等 `AssetServer::is_loaded_with_dependencies` / `LoadState::Loaded` 后再 `Assets<Level>::get(handle)` 取数据【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/index.html>。
- Loader 的扩展名注册与解析（含 `version` 硬失败，`00` §0.7）由 `level_format` + `05` 拥有；编辑器只消费。
- 加载完成 ⇒ `doc = level.clone()`、`saved_text = Some(level_format::write(level))`、清空 undo/redo、`dirty = false`、`Home` 到 `(0,0)`。
- 失败（文件不存在 / RON 语法错 / 版本未知）⇒ 面板显示错误原文（`LoadState::Failed(Arc<AssetLoadError>)` 的字符串），**不**进入编辑态，提供「新建空关卡」和「重试」两个按钮【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/enum.LoadState.html>。

### 8.7.3 `file_watcher` 与热重载的接线

| 项 | 规则 |
|---|---|
| Cargo feature | **`animj-editor` 必须开启** `file_watcher`（属于 bevy 的 `dev` 集合：`debug, bevy_dev_tools, render_dev_tools, file_watcher`）；**发布 bin `animj` MUST NOT** 开启它。feature 的具体写法由 `09` 拥有【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/index.html> |
| 运行时开关 | 编辑器把 `AssetPlugin { watch_for_changes_override: Some(true), ..default() }` 显式置 `true`，避免「依赖某个 feature 才生效」的隐式行为【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/struct.AssetPlugin.html> |
| 资产处理器 | **不要**为 `.ron` 启用 `asset_processor`（文本资产不需要；它默认关闭）【已验证】<https://docs.rs/bevy/0.20.0/bevy/asset/index.html> |
| 消费侧 | `AssetEvent` 在 0.20 里是 **`Message`**，所以用 **`MessageReader<AssetEvent<Level>>`**（不是 `EventReader`） |

### 8.7.4 消费 `AssetEvent::<Level>` + 自写自读的循环抑制

```rust
fn on_level_asset_changed(
    mut events: MessageReader<AssetEvent<Level>>,   // ★ 0.20：AssetEvent 是 Message
    assets: Res<Assets<Level>>,
    mut session: ResMut<EditorSession>,
) {
    for ev in events.read() {
        let AssetEvent::Modified { id } = *ev else { continue };   // AssetEvent: Copy【已验证】
        if session.handle.id() != id { continue; }
        let Some(level) = assets.get(id) else { continue };
        // —— 回环抑制：文本指纹比对 ——
        if let Ok(text) = level_format::write(level) {
            if session.saved_text.as_deref() == Some(text.as_str()) {
                continue;             // 这就是编辑器自己刚写的内容 ⇒ 丢弃
            }
        }
        // 真正的外部变更
        session.pending_external = Some(level.clone());
    }
    // ★ 硬规则：本系统内 MUST NOT 调用任何保存函数（否则 write→event→write 死循环）
}
```

**证据（全部【已验证】）**

| 断言 | 证据 |
|---|---|
| `AssetEvent<A>` 实现 `Message`（不是 `Event`），读取用 `MessageReader<AssetEvent<T>>` | <https://docs.rs/bevy/0.20.0/bevy/asset/enum.AssetEvent.html>（该页 "impl Message for AssetEvent"、示例 `mut events: MessageReader<AssetEvent<LoadedFolder>>`） |
| 0.17 起「拉取式 = `Message` / 推送式 = `Event`」，`EventWriter`/`EventReader` 在 0.20 已不存在 | <https://docs.rs/bevy/0.20.0/bevy/ecs/message/index.html>、`10` |
| 热重载机制：文件变更 → `AssetServer` 重载 → 修改 `Assets<T>` 中数据 → 发 `AssetEvent::Modified { id }` | <https://docs.rs/bevy/0.20.0/bevy/asset/index.html>（§Hot reloading） |
| 变体：`Added{id}` / `Modified{id}` / `Removed{id}` / `Unused{id}` / `LoadedWithDependencies{id}`；`AssetEvent: Copy` | <https://docs.rs/bevy/0.20.0/bevy/asset/enum.AssetEvent.html> |

**抑制策略（4 条，缺一不可）**

1. **原子写 + 非 `ron` 扩展名的临时文件**：写入过程不会产生「半个关卡」的中间态被解析（§8.7.1）。
2. **文本指纹比对**：`serialize(asset) == saved_text` ⇒ 判为自己刚写的内容，直接丢弃。这是**主要**手段（不依赖时间戳、不依赖去抖窗口；写盘后 `Assets` 的同步更新也会命中同一条规则）。
3. **任何 `AssetEvent` 消费系统内禁止保存**：从代码结构上杜绝 `write → Modified → write` 死循环。
4. **外部变更不静默覆盖编辑缓冲**：命中「真外部变更」时只写 `session.pending_external`，由 UI 显式询问：

| 场景 | 行为 |
|---|---|
| `doc` 干净（`dirty == false`） | 顶栏弹「磁盘上的关卡已变化 → [重新载入]」，一键应用：`doc = external`、清 undo/redo、更新 `saved_text` |
| `doc` 脏（`dirty == true`） | 弹确认框，两个选项：**保留我的编辑**（丢弃外部变更，把该 handle 重新指向当前 `doc` 文本以复位指纹）/ **改用磁盘版本**（丢弃编辑，需二次确认）。**禁止**自动合并 |
| 试玩中收到外部变更 | 只排队，退出试玩后再询问 |
| `AssetEvent::Removed` / `Unused`（文件被删） | 状态栏标黄「关卡文件已从磁盘消失」，禁用 `Ctrl+S` 的覆盖写，只允许另存为 |

> **贴图不走本节的确认流程**：瓦片/角色图集被外部重画时立即重建显示（机制见 `04` §5.6），
> 因为它不影响 `doc` 数据，没有「保留我的编辑 vs 用磁盘版本」的冲突。

---

## 8.8 校验反馈

### 8.8.1 校验时机（三处，不给第四处）

| 时机 | 触发 | 目的 |
|---|---|---|
| 编辑中 | `Update` 中 `if session.is_changed()` 跑一次（Bevy 的资源变更检测；不做去抖/定时器）**且** 距上次校验 ≥ `VALIDATE_MIN_INTERVAL = 100ms`（避免拖拽时每帧全校验） | 实时反馈 |
| 进入试玩前 | §8.6.1 第 2 步，**必须**跑 | 阻断坏关卡 |
| 保存前 | §8.7.1 第 2 步，**必须**跑 | 阻断致命错误写盘 |

`is_changed()` 是标准资源变更检测【INFERENCE】（`Res::is_changed` 的基础 API，速查见 `10`）。

### 8.8.2 错误如何展示到 UI

- 错误的**唯一来源**是 `level_format` 的校验器（`03` 拥有规则与消息文案）。编辑器**MUST NOT** 自己发明校验规则、也 MUST NOT 在 UI 层复刻规则（否则两处漂移）。编辑器按下述契约消费：

```
// 契约（最终签名由 03 拥有）：一次调用拿到全部问题，每条问题带定位信息
level_format::validate(&Level) -> Vec<LevelIssue>
LevelIssue { severity: Severity /* Fatal | Error | Warning */,
             path: String /* 字段路径，如 rooms[3].tiles[12] / rooms[7].entities[2].at */,
             message: String }
```

- 展示分层：

| 位置 | 内容 |
|---|---|
| 顶栏 | `N errors / M warnings` 计数徽标（有 error ⇒ 红色，只有 warning ⇒ 黄色） |
| 底栏错误列表 | 每条一行：`severity` + 人类可读位置（`room(3,1) tile(7,12)`）+ 消息 |
| 点击错误 | 相机定位：`rooms[N]` → 切房间；若消息带「列 C」且路径为 `rooms[N].tiles[R]`，进一步**精确对焦到该格中心**（`focus_at` 优先于房间中心）。**错误常驻描边**：每条非 Warning 的瓦片级错误在其格子上画 `ERROR_COLOR` 红框，错误从列表消失即消失 |
| 行内标记 | 房间属性面板的出错字段旁显示红色 `⚠`，悬停显示消息 |
| 控制台 | 校验结果变化的摘要打一条 `warn!`（含路径与数量），便于 CI 抓取 |

- **禁止**：把校验错误画成 sprite 或新开相机/覆盖层（§8.5.5 的 Z 层约束）。

### 8.8.3 错误时是否允许保存 —— 结论

> **结论：允许保存（warn 级与 error 级都允许），只有 `Fatal` 级拒绝写盘。**

理由与细则：

| 级别 | 保存 | 试玩 | 理由 |
|---|---|---|---|
| `Warning` | ✅ | ✅ | 不影响可玩性（如「房间没有 reward」这类完备性提示） |
| `Error` | ✅（写盘后状态栏标红 + 日志 `error!`） | ❌ 阻断 | **必须能存半成品**：编辑器崩溃/被强杀时不能丢工作；同时坏关卡进不了主干靠 CI（`animj-dev validate-levels` 把 error 视为失败） |
| `Fatal` | ❌ 拒绝写盘，打印诊断并要求修好后重试 | ❌ 阻断 | `Fatal` 的判定面被刻意收窄到两类：(a) `version` 不是 `00` §0.7 的当前值（`03` 的 V1）；(b) 房间瓦片维度被破坏（`tiles.len() != ROOM_TILES`，或某行字符数 ≠ `ROOM_TILES`，即 `03` 的 V5）。**这两类在编辑器里由构造保证不可能出现**（编辑器的每一次瓦片写入都走 §8.4.3 的单一索引入口，新建关卡也按 `cols/rows` 一次性分配）——一旦出现即视为**编辑器 bug**，此时拒绝写盘是为了不把已损坏的数据覆盖到磁盘上 |

- **不做**「保存前弹一个校验失败确认框」这种无实质作用的中断——它只会训练出「无脑点确定」的肌肉记忆。计数徽标 + CI 才是真闸门。

---

## 8.9 快捷键表（完整）

> 全表在 `not(egui_wants_any_keyboard_input)` 下生效；标 `[试玩]` 的在 `EditorMode::Playing` 下生效。
> **唯一例外**：`F5`/`F6` 的退出分支不加 egui 焦点门控——试玩态没有任何 egui 文本输入，退出是安全热键，
> 不能被焦点状态吞掉（否则编辑器会卡在试玩态）。试玩态另有固定浮层的「停止试玩」按钮（§8.5.6），
> 提供不依赖键盘的退出路径。

### 工具切换

| 键 | 动作 |
|---|---|
| `1` | `TileBrush` 瓦片笔刷 |
| `2` | `Eraser` 擦除 |
| `3` | `RectFill` 矩形填充 |
| `4` | `Picker` 吸管 |
| `5` | `PlaceEntity` 实体放置 |
| `6` | `MoveEntity` 实体拖动 |
| `Alt`（按住）+ 左键 | 临时吸管（松开恢复原工具） |
| `Esc` | 取消进行中的拖拽（回滚该次操作） |

### 编辑操作

| 键 | 动作 |
|---|---|
| `Ctrl+Z` | 撤销（一个拖拽 = 一步） |
| `Ctrl+Shift+Z` / `Ctrl+Y` | 重做 |
| `Ctrl`（按住） | 关闭吸附（实体工具对齐到整世界像素） |
| `Shift`（按住） | `3` 约束正方形；`1` 约束直线 |

### 文件与试玩

| 键 | 动作 |
|---|---|
| `Ctrl+S` | 保存当前关卡（`[试玩]` 下 no-op 并提示） |
| `Ctrl+Shift+S` | 另存为（弹出路径输入） |
| `Ctrl+R` | 从磁盘重新载入当前关卡（**需二次确认**，丢弃编辑缓冲与 undo 栈） |
| `F5` | 切换试玩（编辑 → 试玩 / 试玩 → 编辑） |
| `F6` | `[试玩]` 停止试玩（与 `F5` 等价，语义更明确） |

### 相机与视图

| 键 / 操作 | 动作 |
|---|---|
| 中键拖动 | 平移（`translation` 量化到 0.5 的倍数） |
| `Space` + 左键拖动 | 平移（备用，方便无中键的鼠标/触控板） |
| 滚轮上 / 下 | 缩放放大 / 缩小一档（档位表 §8.4.2），以光标世界点为锚 |
| `Home` | 视角回到当前房间中心 + `magnify = 1.0` |
| `Tab` / `Shift+Tab` | 当前房间 +1 / -1（行主序，`00` §0.3） |
| `G` | 瓦片网格叠加开/关 |
| `Shift+G` | 房间边界叠加开/关 |
| `B` | 实体 AABB 叠加开/关 |
| `F1` | 帮助浮层（显示本表）开/关 |

**约定**：`Ctrl`/`Shift`/`Alt` 的角色在三处（吸附、约束、临时吸管）是**固定**的，不允许某个工具私自改它们的语义。
**冲突处理**：若某键同时被 egui 与编辑器关注（例如 `Ctrl+S` 在 egui 有全局快捷键时），以编辑器为准——UI 代码 MUST NOT 给 egui 注册全局快捷键，避免双触发。

---

## 8.10 CLI：`animj-dev`

### 8.10.1 通用约定

| 项 | 规则 |
|---|---|
| 目标 | 只在 CI 与本地批处理里跑；**MUST NOT** 创建窗口、GPU 上下文或 `App` |
| 依赖 | **只允许** `level_format` + `std` + `png`（后者仅用于占位资产编码，见 §8.10.3 G2）⇒ `crates/editor/src/bin/animj_dev.rs` 里不得出现 `use bevy`（CI 文本检查，§8.2.3）。它可以在无显示器的 CI 容器里运行 |
| 解析参数 | 手写 `std::env::args`（**不引入 `clap`**：`00` D15 的依赖白名单里没有它，版本与 feature 由 `09` 拥有） |
| 输出 | 人类可读行输出到 **stdout**；错误/诊断到 **stderr**；`--json` 时 stdout 只输出**一个** JSON 文档（无其它杂音），便于 CI 解析 |
| 路径 | 默认扫描 `assets/levels/**/*.level.ron`（相对仓库根）；也可显式给出文件或目录（目录递归） |
| 稳定性 | 同输入同输出（无时间戳、无随机、无绝对路径）；JSON 字段顺序固定 |

### 8.10.2 `validate-levels`

```
animj-dev validate-levels [<path>...] [--json] [--deny-warnings] [--format-version <n>]
```

| 参数 | 默认 | 语义 |
|---|---|---|
| `<path>...` | `assets/levels/` | 文件或目录；目录递归匹配 `*.level.ron`。**零个匹配文件 ⇒ 视为失败**（退出码 1） |
| `--json` | off | 输出机器可读汇总（见下） |
| `--deny-warnings` | off | 把 `Warning` 也计为失败（发布前可用） |
| `--format-version <n>` | 当前 `00` §0.7 的 `Level.version` | 额外断言每个文件的 `version` 等于给定值（版本升级期的批量体检） |

每个文件都会走完整的「读文件 → RON 解析 → `version` 检查 → 维度不变式 → `level_format::validate`」，即**编辑器/运行时用的同一条校验路径**（不复制规则）。

行输出（失败示例）：

```
assets/levels/world_01.level.ron: OK
assets/levels/wip.level.ron: FAIL
  ERROR V17 rooms[0].entities[0]: 实体 AABB 与 Solid 瓦片重叠
  WARNING V18 rooms[1].tiles[14]: 接缝处一侧 Solid、另一侧 Air
2 files checked: 1 ok, 1 failed (1 errors, 1 warnings)
```

- 严重度前缀固定为 `ERROR` / `WARNING` / `FATAL`（`03` §3.7 的三态），**后面直接跟 `LevelIssue.message`**。
  由于 `message` 自身已经带 `Vxx` 编号与字段路径（`03` §3.7 的硬要求），CLI 侧**不得**再拼一遍 `room(x,y) tile(x,y)` 之类的位置串——那是重复信息，且会与 `message` 的路径口径打架。
- `--json` 里的 `room` 字段由 `path` 反解（`rooms[i]` → `(i % cols, i / cols)`）；v1 的 path 只带文件行、不带列，因此 `tile` 恒为 `null`。

`--json` 输出形状（字段名固定）：

```json
{
  "files": [
    { "path": "levels/world_01.level.ron", "ok": false,
      "issues": [ { "severity": "error", "room": [3,1], "tile": [7,12], "message": "…" } ] }
  ],
  "summary": { "files": 2, "ok": 1, "failed": 1, "errors": 3, "warnings": 5 }
}
```

### 8.10.3 `gen-placeholder-assets`

```
animj-dev gen-placeholder-assets [--out <dir>] [--force] [--check]
```

| 参数 | 默认 | 语义 |
|---|---|---|
| `--out <dir>` | 仓库 `assets/` 根 | 输出根目录。路径与文件名的**权威定义在 `04`**（本文只约定机制，不定义资源路径） |
| `--force` | off | 覆盖已存在的不同内容文件 |
| `--check` | off | **不写盘**：把生成结果与现有文件逐字节比较，有任何差异 ⇒ 失败（CI 防漂移） |

实现规则：

| # | 规则 |
|---|---|
| G1 | 占位图内容 = `04` 规定的程序化像素图案（尺寸/配色/命名由 `04` 拥有；本文不复制任何数值）。生成函数 MUST 是**纯函数**（相同参数相同字节） |
| G2 | 输出格式 = PNG（`00` §0.5 规定 sprite 为 `.png`）。用 **`png 0.18.1`**（`09` §9.1 登记的唯一**工具**依赖）编码，并**强制 `set_compression(NoCompression)`**：不做压缩 ⇒ 输出与压缩实现无关，跨进程/跨机器逐字节可复现（`--check` 依赖这一点）。编码后必须**立即解码回读并逐字节比对**，不自洽 ⇒ 退出码 4 |
| G3 | 色深固定 8-bit RGBA、无隔行、无调色板、无辅助 chunk（时间戳类 chunk 一律不写，保证确定性） |
| G4 | 不覆盖已存在的文件，除非 `--force`；被跳过的文件在 stdout 逐条说明 |
| G5 | 若输出目录不存在则创建；`--out` 指向仓库外时不报错（方便本地试用） |

> 「用 `png` 而不是自研编码器」是**有意**的取舍：自研（PNG 签名 + IHDR + stored 块的 IDAT + IEND + CRC32）约百行、同样字节确定，但属于自造轮子；`png` + `NoCompression` 同时拿到「零维护」与「字节确定」。代价是 `editor` crate 多一个**仅工具用**的依赖（`09` §9.1 已登记，不进运行时）。
> **回退方案已明确**：若将来要砍掉该依赖，把 `raster::encode_png` 换成自研编码器即可（接口不变：`&Raster -> Result<Vec<u8>, String>`，且必须保留「编码后回读比对」这一步）。

### 8.10.4 退出码约定（供 CI 使用）

| 码 | 名称 | 触发条件 | CI 处理 |
|---|---|---|---|
| `0` | `Ok` | 全部检查通过（`--deny-warnings` 下无 warning） | 继续 |
| `1` | `CheckFailed` | 发现了 `error`（或 `--deny-warnings` 下的 `warning`）；`gen-placeholder-assets --check` 发现漂移；目录下零个匹配文件 | **失败**：关卡/资产不过关，输出已打印到 stdout，不允许绕过 |
| `2` | `UsageError` | 未知子命令、未知参数、`--format-version` 值非法、`--out` 缺失参数值 | 失败：CI 配置写错了，修 CI |
| `3` | `IoOrParseError` | 文件读不到、不是合法文件头、RON 语法错误、结构无法反序列化、写盘失败 | 失败：数据或环境污染（不是内容校验问题），输出含精确路径与原因 |
| `4` | `InternalError` | 不变式被破坏（如序列化成功但重新解析失败 ⇒ 编解码不自洽） | 失败并**必须**提 issue：这是工具本身有 bug |

- `1` 与 `3` 的区别是硬要求：**「关卡内容不合格」与「读不到/读不懂文件」必须能被 CI 分开统计**（前者怪作者，后者怪环境）。
- `--help` / `--version` ⇒ 退出码 `0`。
- 退出码常量在 `crates/editor/src/bin/animj_dev.rs` 的内联 `mod exit` 里定义为 `enum ExitCode { Ok = 0, CheckFailed = 1, UsageError = 2, IoOrParseError = 3, InternalError = 4 }`，**任何路径都通过 `std::process::exit(code as i32)` 唯一出口**（禁止多处散落字面量）。它内联在该 bin 里而**不是**独立文件，是因为 `editor` crate 没有 lib target；这样也满足 §8.10.1 的「CLI 只依赖 `level_format` + `std` + `png`」。

### 8.10.5 CI 用法（`09` 的流水线里抄这两行）

```bash
cargo dev-cli -- validate-levels --deny-warnings      # 关卡体检
cargo dev-cli -- gen-placeholder-assets --check       # 占位资产防漂移
```
（`cargo dev-cli` 是 `.cargo/config.toml` 里的别名 = `run -p editor --bin animj-dev --features dev`。**必须带 `--features dev`**：否则解析出的 bevy feature 集与其它入口不同，会触发 Bevy 全量重编译，见 `09` §9.4.4。）

---

## 8.10.5 M5 实现状态（与本文的偏差，逐条如实列出）

**已实现**：§8.3 状态模型（`EditorMode` + `GameState::Paused` 零侵入）、§8.4 相机（唯一相机 / `Msaa::Off` /
`PrimaryEguiContext` / 档位缩放 + 光标锚定 / 中键平移 + 0.5 量化 / `Home` / `Tab` 切房间 / 光标→瓦片单一换算）、
§8.5 工具 1–6（笔刷、擦除、矩形填充、吸管、放实体、移动实体；`Ctrl` 关吸附；拖拽所有权；`Esc` 取消回滚；
笔刷/橡皮拖动时逐格推进 `doc_revision`，实体拖动时位置变化也推进修订号，保证投影实时重绘）、
§8.5.2 全量快照撤销/重做（`UNDO_LIMIT = 128`）、§8.5.3 **工具 6 `MoveEntity`**（逻辑 AABB 命中、瓦片中心吸附、`Ctrl` 关吸附、跨房间取消并回滚）、§8.5.5 Gizmos 网格/边界（两个自定义组）、
§8.5.6 四栏面板（顶/左/右/底，含 goals 与 reward 编辑）、§8.6 就地试玩（校验闸门 → 注入 → 退出，
R1–R3 由类型系统 + `--demo-play` 断言）、§8.7.1 保存（序列化 → 校验 → `.ron.tmp` → `rename`，`saved_text` 记账）、
§8.8 校验展示（计数徽标 + 列表；唯一来源是 `level_format`）。

**§8.10.5 原顺延项现已全部实现**：工具 6（实体拖动）、编辑态热重载消费（检测 → 显式 Apply/Ignore → 文本级回环抑制）、
错误列表点击定位（解析 `rooms[N]` → 切房间 + 相机对焦）、`Ctrl+S` 走路径输入框（另存为）、缩放锚点的下一帧自检（`debug_assert`）、
`Shift` 约束（矩形正方形 / 笔刷直线）。

**明确移除**：区域框选（`Marquee`）及其派生的房间集合批量编辑与瓦片复制/粘贴/填充快捷键——
实现成本与收益不匹配；实体管理改为按当前房间列出（`08` §8.5.3 / §8.5.6）。

## 8.11 实现检查清单（评审用，逐条可勾）

| # | 项 |
|---|---|
| 1 | `crates/editor` 是唯一引用 `bevy_egui` 的地方；`cargo tree -p animj -e normal` 里没有 `egui` |
| 2 | `game` 的公开 API 里没有 `EditorMode` / `EditorSession` 等编辑器类型 |
| 3 | 编辑器里没有出现 `00` §0.3 的坐标算术（只调 `world_to_room` / `room_local_to_world`） |
| 4 | 编辑器里没有出现第二个瓦片索引算式（只有 §8.4.3 的那一个访问器） |
| 5 | `PrimaryEguiContext` 显式挂在 `EditorCamera` 上；`Msaa::Off` 已插 |
| 6 | 世界输入系统带 `not(egui_wants_any_pointer_input)`；快捷键系统带 `not(egui_wants_any_keyboard_input)` |
| 7 | 试玩相关系统只持有 `Res<EditorSession>`（R1 由类型系统保证） |
| 8 | 热重载消费系统用 `MessageReader<AssetEvent<Level>>`，且其中没有调用保存 |
| 9 | 保存走「序列化 → 校验 → `.ron.tmp` → `rename`」四步，且 `saved_text` 已记账 |
| 10 | 校验结果只来自 `level_format`；编辑器没有自建规则 |
| 11 | 叠加全部用 Gizmos；编辑器没有为叠加新增 sprite |
| 12 | `animj-dev` 的两个子命令都不构造 `App`，`crates/editor/src/bin/animj_dev.rs` 里没有 `use bevy` |
| 13 | 退出码只有 §8.10.4 那五个，且从单一出口返回 |
| 14 | 编辑器不注册 `07` 的离屏像素管线（用 `animj` 验证像素效果） |

### 8.5.8 资源路径选择器

地块贴图与动画图集路径旁的「浏览…」打开**内嵌目录列表**（`AssetPickerState`）：以 `assets/` 为根，逐级进入子目录，只列目录与 `.png`，点文件即写入相对 `assets/` 的路径。不使用系统文件对话框（不引入 `rfd` 依赖），路径约定与同步加载器完全一致。

### 8.5.9 动画图集切换与帧清空

- `ensure_sheet_texture` 只做「首次上传」；**切换图集路径后必须把 `sheet_texture.0` 置空**再调用，否则预览/选框会一直显示旧图（实机 bug）。`prepare_textures` 用 `Local<Option<Handle<Image>>>` 比较句柄来做这件事。
- 每个状态行提供「清空帧」：清空后该状态帧序列为空，保存会被校验拒绝（提示帧序列为空），直到重新添加帧。

### 8.5.10 动画保存后的生效路径

`game::anim_data::watch_anim_file` 轮询 `assets/anim/player.anim.ron` 的 mtime，变化即重新解析并 `ActiveAnim.revision += 1`；`PlayerSheet` 的每状态贴图/布局按 `revision` 重建。因此编辑器「保存动画」后**无需重启**，编辑器试玩与发布 bin 都会立即用新数据（解析失败保留旧数据并打印错误）。
