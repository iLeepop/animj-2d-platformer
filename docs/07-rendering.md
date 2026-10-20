# 07 — 渲染（像素完美管线）

> **本文拥有**：所有渲染常量 —— 内层视口尺寸、窗口默认尺寸、相机 `order`、渲染层编号、采样/MSAA/色调映射的固定取值、
> 像素吸附规则与调度、瓦片渲染策略、调试可视化、HUD/文字的选型与写法常量、`Z_CANVAS`。
> **本文不拥有**：Z 层数值（见 `00` §0.4，本文只声明常量名，**不复制数值**）、`TILE_PX`/`ROOM_PX`/坐标转换（`00` §0.3）、
> 物理与手感（`06`）、模块与调度装配（`05`）、房间拓扑与相机跟随逻辑（`02`）、图集物理布局与资产规格（`04`）、
> 依赖版本与 cargo feature 开关（`09`）、Bevy API 速查与过期名称黑名单（`10`）。
>
> 标记约定见 `00` §0.9：**【已验证】** = 有 0.20 文档/官方 example 直接支撑并附 URL；**【UNVERIFIED】** = 无 0.20 证据；
> **【INFERENCE】** = 由已验证事实推断。所有 Bevy 断言禁止凭记忆书写，写码前对照 `10` 的黑名单。

---

## 7.1 渲染常量表

实现位置：`crates/game/src/config/render.rs`（常量分组规则见 `00` §0.5）。**本文定义的常量不得在其它文档重复定义。**

### 7.1.1 视口与窗口

| 常量 | 值 | 说明 |
|---|---|---|
| `VIEWPORT_TILES` | `10` | 内层视野覆盖的瓦片数。必须小于 `ROOM_TILES`（一个房间 16×16 瓦片），玩家看不到整间房 |
| `VIEWPORT_W` | `VIEWPORT_TILES × TILE_PX` | 内层离屏渲染目标宽（像素）。**数值见 `00` §0.3 的 `TILE_PX`**，本文不复制 |
| `VIEWPORT_H` | `VIEWPORT_TILES × TILE_PX` | 内层离屏渲染目标高。同上 |
| `WINDOW_W` | `2 × VIEWPORT_W` | 窗口初始宽（物理像素）⇒ 启动即 2 倍整数缩放、无黑边 |
| `WINDOW_H` | `2 × VIEWPORT_H` | 窗口初始高。同上 |
| `WINDOW_W_SPLIT` | `2 × WINDOW_W` | 本地双人进入游戏时的窗口宽（两屏并排、各 2×） |
| `WINDOW_H_SPLIT` | `WINDOW_H` | 本地双人窗口高 |
| `MIN_CANVAS_SCALE` | `1` | 整数放大倍数 `k` 的下限。`k = max(1, floor(min(w / (N×VIEWPORT_W), h / VIEWPORT_H)))`，`N` = 视图数 |
| `WINDOW_TITLE` | `"animj"` | 窗口标题 |

- **视图数 `N`**：单人/主机/客户端 = 1；本地双人 = 2（左右分屏）。布局宽 = `N × VIEWPORT_W`，高 = `VIEWPORT_H`。
  每个视图有**自己的** 320×320 离屏纹理与内层相机；`MAX_VIEWS = 2` 定义在 `render::pixel_camera`。
  
> **关于 `320` 这个数字**：它只会在本文里作为 `VIEWPORT_W`/`VIEWPORT_H` 的**展示值**出现（10 × 32 = 320，用于读者对照与图形示意图）。
> 它的唯一权威来源是 `VIEWPORT_TILES` 与 `00` §0.3 的 `TILE_PX`；本文不另立数字。

**窗口尺寸选定理由**：窗口仍取 `2 ×` 视口（640²），与上一版保持相同的屏幕像素倍率：角色在屏幕上的大小不变，只是世界视野收窄。
若改为 `3 ×`（960²）会把角色与 HUD 一起放大，那是另一种缩放需求，不属于本次「相机视角」调整。
像素风窗口的边长应为 `VIEWPORT` 的整数倍，这样启动时 `k` 恰好是整数、四周无黑边。
视野选 `10×10` 瓦片（而不是整间房 `16×16`）是产品需求：相机贴近角色，玩家看不到太多地图；它同时满足「与整间房同源、仍是瓦片整数倍」。
证据（API 面）：`WindowResolution::new(physical_width: u32, physical_height: u32)`、`Window::width() -> f32`、`Window::height() -> f32`、`Window::size() -> Vec2`
【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowResolution.html> ；<https://docs.rs/bevy/0.20.0/bevy/window/struct.Window.html>

### 7.1.2 渲染层与相机

| 常量 | 值 | 说明 | 证据 |
|---|---|---|---|
| `PIXEL_PERFECT_LAYERS` | `RenderLayers::layer(0)` | 所有游戏内 `Sprite` / `Text2d` 所在层；内层相机只看这一层 | 【已验证】官方 `pixel_grid_snap` 同名常量 <https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html> ；`RenderLayers::layer` 是 `const fn`（同源常量定义） |
| `HIGH_RES_LAYERS` | `RenderLayers::layer(1)` | 只放「承载离屏纹理的 canvas sprite」；外层相机只看这一层 | 同上 |
| `PARALLAX_LAYER_BASE` | `8` | 视差背景层基准；视图 `i` 的背景在 `8+i`，该视图的内层相机 = `[layer(0), layer(8+i)]`（两屏各自相位，见 §7.13.5） | 【已验证】`RenderLayers::from_layers` / `with`：<https://docs.rs/bevy/0.20.0/bevy/camera/visibility/struct.RenderLayers.html> |
| `INNER_CAMERA_ORDER` | `-1` | 每个内层相机先渲染（各自填满自己的离屏纹理） | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Camera.html>（`order`：高值后渲染、覆盖在上）+ 官方 example 注释 `order: -1 // Render before the "main pass" camera` |
| `OUTER_CAMERA_ORDER` | `0` | 外层相机（默认值，**不要**改成别的） | 同上 |
| `UI_CAMERA_ORDER` | `OUTER_CAMERA_ORDER + 1` | UI 相机：目标 = 主窗口，`clear_color = None`、`RenderLayers::none()`；必须在 canvas 之后渲染 | 同上 |

- UI 用**独立窗口相机**渲染（`IsDefaultUiCamera` 挂在它上面），`UiScale = k` 把逻辑坐标钉在 `N × VIEWPORT_W` 宽、`VIEWPORT_H` 高（详见 §7.10.1）。
  这不是可选优化：`bevy_picking` 的 UI 后端只把「与指针同渲染目标」的相机纳入命中测试，内层相机目标是离屏纹理，按钮会永远点不动。

- **未插 `RenderLayers` 的相机只渲染 layer 0**。【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/visibility/struct.RenderLayers.html>（example 注释原文：“This camera has no `RenderLayers` component, so it only renders entities belonging to render layer `0`.”）
- **UI 节点不受 `RenderLayers` 影响**，只能被一个相机渲染。【已验证】同上（原文：“UI nodes can only be rendered by one camera at a time and ignore `RenderLayers`.”）

### 7.1.3 Z 层常量（**数值一律见 `00` §0.4，本文不复制**）

| 常量名（`config::render::z`） | 用途 | 值 |
|---|---|---|
| `z::PARALLAX` | 远景视差 | 见 `00` §0.4 |
| `z::ROOM_BG` | 房间背景板 | 见 `00` §0.4 |
| `z::TILE_DECAL` | 瓦片贴花（草、砖纹） | 见 `00` §0.4 |
| `z::TILE` | **实体瓦片** | 见 `00` §0.4 |
| `z::PICKUP` | 可拾取物（果实） | 见 `00` §0.4 |
| `z::ENEMY` | 敌人 | 见 `00` §0.4 |
| `z::PLAYER` / `z::player_z(id)` | **角色本体**：`40.0 + 0.1 × 玩家编号` | 见 `00` §0.4；多人会互相重叠，必须不同 Z。每个视图的相机跟随自己的玩家（`02` §2.2）；视野仍小于一个房间 |
| `z::DECORATION_BASE` | 装饰品图层基准；实际 Z = `DECORATION_BASE + layer_index as f32`（槽位语义与上限见 `04` §6.1） | 见 `00` §0.4 |
| `z::PARTICLE` | 粒子 / 特效 | 见 `00` §0.4 |
| `z::TILE_FOREGROUND` | 前景瓦片 | 见 `00` §0.4 |
| `z::TEXT2D` | 世界空间文字（`Text2d`） | 见 `00` §0.4 |
| `z::CANVAS` | **外层视图**里那张离屏纹理 sprite 的 Z | `0.0`（**本文定义**，不属于 `00` §0.4 的游戏 Z 表） |

> `z::CANVAS` 与游戏 Z 表**不共享语义**：排序是按「视图」（`Camera` + `RenderTarget`）各自进行的。外层相机的视图里只有 canvas sprite（外加可选的外层调试 UI），
> 因此 `0.0` 不会与任何游戏 sprite 冲突。**不要**把 `z::CANVAS` 拿到内层视图里使用，反之亦然。

### 7.1.4 采样 / 抗锯齿 / 色调映射的固定取值

| 项 | 本项目取值 | 后果（不按此设置） |
|---|---|---|
| 纹理采样 | `DefaultPlugins.set(ImagePlugin::default_nearest())` | 见 §7.3.1 |
| MSAA | 两个相机**都**显式 `Msaa::Off` | 见 §7.3.2 |
| `Tonemapping` | 保持 `Camera2d` 默认的 `Tonemapping::Linear`，**不插 `Hdr`** | 见 §7.3.3 |
| 离屏纹理格式 | `TextureFormat::Bgra8UnormSrgb` | 见 §7.1.5 |
| 相机 `Projection` | 见 §7.5（内层 `Fixed`，外层 `WindowSize`） | 见 §7.5 |

### 7.1.5 离屏渲染目标的格式与色彩空间

- 格式：`TextureFormat::Bgra8UnormSrgb`（【已验证】官方 `pixel_grid_snap` 的 `Image::new_target_texture(RES_WIDTH, RES_HEIGHT, TextureFormat::Bgra8UnormSrgb, None)`，<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>）。
- 必须是 **sRGB** 变体：它与窗口交换链同色彩空间，内层（sRGB 纹理 → 线性着色 → 写 sRGB 目标）与外层（sRGB → 线性 → 写 sRGB 窗口）两次转换相互抵消。
  若改成非 sRGB（如 `Bgra8Unorm`），整条管线的颜色会被系统性压暗/提亮，而且**不会**报错。【INFERENCE】（由 sRGB 纹理的线性化语义 + 官方 example 的确定取值推断）
- `Image::new_target_texture(width: u32, height: u32, format: TextureFormat, view_format: Option<TextureFormat>)`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/image/struct.Image.html>（含官方 example 的调用形态）
- 该纹理由 `Assets<Image>` 持有并作为 `RenderTarget` 引用；**不得**给它设 `ImageSampler`（渲染目标自动使用 nearest 语义的 1:1 采样），也**不得**给它设 `custom_size`。
- 资产规格（PNG/RGBA/sRGB/8bit/零 padding）见 `04` §7.1、§3.1；`.hdr` 图片格式与相机 HDR 无关（见 §7.3.3）。

---

## 7.2 双层相机管线

### 7.2.1 结构

```
                          世界（关卡 = cols×rows 个房间；内层视野 320×320）
                                   │
   ┌───────────────────────────────┴───────────────────────────────┐
   │  内层相机  InGameCamera                                        │
   │  Camera { order: -1 }                                          │
   │  Projection = Orthographic { ScalingMode::Fixed{320,320}, 1.0} │
   │  RenderTarget::Image(canvas)      ← 离屏纹理 320×320, Bgra8Srgb │
   │  Msaa::Off · RenderLayers::layer(0)                            │
   │  Transform.x/y = 玩家中心（钳制在地图内；由 02 写入，整像素；z 默认）│
   └───────────────────────────────┬───────────────────────────────┘
                                   │  画进离屏纹理（1 世界单位 = 1 texel）
   ┌───────────────────────────────┴───────────────────────────────┐
   │  canvas sprite  Canvas · RenderLayers::layer(1) · z::CANVAS    │
   │  源纹理 = 上面那张离屏纹理；Transform 在原点（外层世界原点）      │
   └───────────────────────────────┬───────────────────────────────┘
                                   │
   ┌───────────────────────────────┴───────────────────────────────┐
   │  外层相机  OuterCamera                                          │
   │  Camera { order: 0 }                                            │
   │  Projection = Orthographic { ScalingMode::WindowSize, scale=1/k}│
   │  RenderTarget = 默认（主窗口）  Msaa::Off  RenderLayers::layer(1)│
   │  Transform 恒为原点                                             │
   └───────────────────────────────┬───────────────────────────────┘
                                   │  整数倍放大 k×，四周是 ClearColor（黑）
                                窗口
```

- 每个内层视野固定为 `VIEWPORT_W × VIEWPORT_H`（`VIEWPORT_TILES` × `VIEWPORT_TILES` 瓦片，**小于一个房间**，见 `00` §0.1 **D5/D13** 与 `02`）。每个视图的相机跟随自己的玩家，因此视野通常落在一个房间内，靠近边界时可同时覆盖 2–4 个相邻房间。
- `N = 2` 时：两张离屏纹理、两台内层相机、两个 canvas sprite 在窗口里左右并排；外层相机仍只有 `scale = 1/k` 这一条缩放规则，`k` 按**总布局尺寸**计算。
  从单视图进入本地双人时，窗口默认从 `WINDOW_W × WINDOW_H` 调到 `WINDOW_W_SPLIT × WINDOW_H_SPLIT`（仅窗口模式；全屏或用户改过尺寸则不动），退出时恢复。
- 外层视图里只有 canvas sprite ⇒ 黑边 = 外层相机的清屏色 = `ClearColor` 资源。因此把 `ClearColor` 设为纯黑，
  这样「视口外区域」与「房间外区域」都是同一种黑，不存在两种黑的接缝。
- **参考分辨率固定**：本项目不做「多分辨率适配」。改 `VIEWPORT_TILES` / `VIEWPORT_W/H` 等于改契约，必须同步 `00` §0.1 与 `02`/`04`/`05`。

### 7.2.2 结构要点（源码为准）

文件：`crates/game/src/render/pixel_camera.rs`。

- 启动时创建 `MAX_VIEWS = 2` 张 320×320 的 `Image::new_target_texture`（`Bgra8UnormSrgb` + `COPY_SRC`），
  每张对应一台 `InGameCamera`（`ScalingMode::Fixed{VIEWPORT_W,VIEWPORT_H}`、`Msaa::Off`、`PlayerView(i)`）
  和一个 `Canvas { view: i }` sprite。
- `ViewLayout { views }` 由 `SessionMode` 决定：`LocalCoop = 2`，其它 = 1。
  `apply_view_activation` 把 `view >= views` 的内层相机 `is_active = false`；`fit_canvas` 隐藏多余 canvas。
- 内层相机 i 的 `RenderLayers = [layer(0), layer(PARALLAX_LAYER_BASE + i)]`：世界实体在 layer 0，
  视差背景按视图分到 8/9，因此两屏背景相位互不干扰（§7.13.5）。
- canvas sprite i 的世界 x = `(i - (views-1)/2) × VIEWPORT_W`，整体居中；外层相机仍在原点。
- `fit_canvas` 计算 `k = max(1, floor(min(window_w / (views×VIEWPORT_W), window_h / VIEWPORT_H)))`，
  写外层 `scale = 1/k`，并把 UI 相机视口设为居中的 `(views×VIEWPORT_W×k, VIEWPORT_H×k)`。
- 相机位置由 `camera::rig` 写：`follow_target` 直接钳制到地图内，随后由 `render::snap` 统一取整。
- UI 逻辑尺寸 = 视口物理尺寸 / `UiScale` = `views×VIEWPORT_W × VIEWPORT_H`；单人仍是 320×320。

**数学推导（为什么 `scale = 1 / k`）**
`ScalingMode::WindowSize` + `scale = s` 时，视图覆盖的世界范围 = 窗口逻辑尺寸 × `s`（【已验证】`ScalingMode` 文档：“The effect of these scaling modes are combined with the `OrthographicProjection::scale` property… if the scaling mode is `Fixed { width: 100, height: 300 }` and the scale is `2.0`, the projection will be 200 world units wide and 600 world units tall.”）。
要让 `VIEWPORT_W` 世界单位宽的画面占满 `k × VIEWPORT_W` 物理像素，需要视图宽度 = `窗口宽 / k`，故 `s = 1 / k`。
证据：<https://docs.rs/bevy/0.20.0/bevy/camera/enum.ScalingMode.html> ；官方 `fit_canvas` 同式（`projection.scale = 1. / h_scale.min(v_scale).floor()`）<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>

**行为边界（必须明确）**

| 情况 | 行为 |
|---|---|
| 窗口 = 总布局或更小 | `k = 1`，画面 1:1，**不做**缩小、不裁切 |
| 窗口 < `VIEWPORT` 的某一轴 | `k = 1`，窗口裁掉超出的部分（保像素完美，不引入分数缩放） |
| 非整数倍窗口 | `k = floor(min(w / (views×VIEWPORT_W), h / VIEWPORT_H))` ⇒ 整数倍 + 黑边。这是**故意**的取舍：任何分数缩放都会破坏像素格 |
| 窗口内容按 `k` 居中 | 由「canvas sprite 在原点 + 外层相机在原点 + 默认 `viewport_origin = (0.5, 0.5)`」保证，无需额外计算 |

### 7.2.3 四处易于踩错的 0.20 写法

| 错误写法 | 0.20 正确写法 | 证据 |
|---|---|---|
| `Camera { target: RenderTarget::Image(h), .. }` | `RenderTarget::Image(h.into())` 作为**独立组件**与 `Camera` 并列插入（`Camera` 上已无 `target` 字段） | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Camera.html>（字段表中无 `target`）；<https://docs.rs/bevy/0.20.0/bevy/camera/enum.RenderTarget.html>（`RenderTarget` 实现 `Component`）；<https://bevy.org/learn/migration-guides/0-17-to-0-18/#rendertarget-is-now-a-component> |
| `RenderTarget::Image(handle)` 直接传 `Handle<Image>` | 变体字段是 `ImageRenderTarget { handle, scale_factor }`，用 `Handle<Image> → ImageRenderTarget → RenderTarget` 的两级 `From`：`canvas_image.clone().into()` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.ImageRenderTarget.html>（`impl From<Handle<Image>> for ImageRenderTarget`、`scale_factor` 文档原文 “should almost always be 1.0”） |
| `ScalingMode::FixedVertical(270.0)` | `ScalingMode::FixedVertical { viewport_height: 270.0 }`（具名字段变体） | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/enum.ScalingMode.html> |
| `Camera2dBundle { .. }` | 不存在。`Camera2d` 是 unit component，直接作为元组元素 spawn | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Camera2d.html> |

- 外层相机不插 `RenderTarget` ⇒ 默认渲染到主窗口。【已验证（官方 example 即如此）】<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>
- 内层相机的清屏：保持 `Camera::default()` 的 `ClearColorConfig::Default`（即 `ClearColor` 资源 = 黑）。
  房间内部由 `z::ROOM_BG` 的背景板 sprite 覆盖整房间，所以清屏色只在「房间内容缺失」时可见（异常可视化，是好事）。

---

## 7.3 三个必须显式设置的坑

### 7.3.1 `ImagePlugin::default_nearest()`（全局最近邻采样）

```rust
App::new().add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
```

- **不设会怎样**：`ImagePlugin` 默认用线性采样（`default_linear()`），所有 sprite 在放大时被双线性插值 ⇒
  像素块之间出现过渡色、图集相邻单元互相渗色（“糊”）——像素风最致命的失败模式，而且只在放大后可见，早期很容易漏掉。
- 证据：【已验证】`ImagePlugin::default_nearest()` <https://docs.rs/bevy/0.20.0/bevy/image/struct.ImagePlugin.html> ；
  官方用法与注释（“prevents blurry sprites”）<https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html>、<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html> ；
  `ImagePlugin` 在 prelude。<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html>
- 逐资源覆盖（少数需要线性缩放的 UI 图）：`ImageLoaderSettings.sampler` / `AssetServer::load_builder().with_settings(..)`，写法见 `10` §10.2.9；资产规格与「何时允许线性」见 `04` §8。

### 7.3.2 `Msaa::Off`（两个相机都要写）

- **不设会怎样**：`Msaa` 的 `Default::default()` 是 `Msaa::Sample4`，并且 `RenderPlugin::build` 对每个 `Camera` 执行了
  `register_required_components::<Camera, Msaa>()` ⇒ **每个相机都自带 4× MSAA**。后果：
  1. sprite 边缘被抗锯齿成半透明过渡像素（像素艺术被“磨圆”），与 §7.3.1 的 nearest 采样叠加后表现为边缘色带；
  2. 每个相机多一次 MSAA resolve 的 GPU 开销（离屏目标也是 MSAA 的话更贵）。
- 正确做法：**内层与外层相机都**显式插 `Msaa::Off`（`Msaa` 是**相机组件**，不是全局资源）。
- 证据：【已验证】`Msaa` 枚举 `{ Off = 1, Sample2, Sample4, Sample8 }` 与默认值 `Sample4`
  <https://docs.rs/bevy/0.20.0/bevy/render/view/enum.Msaa.html> ；官方像素示例在两个相机上都插了 `Msaa::Off`
  <https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>（第 95、105 行）。

### 7.3.3 `Tonemapping::Linear` + **不要**给 `Camera2d` 加 `Hdr`

- `Camera2d` **默认就是 `Tonemapping::Linear`**：`Core2dPlugin::build` 运行期执行
  `register_required_components_with::<Camera2d, Tonemapping>(|| Tonemapping::Linear)`，同时注册 `DebandDither`。
  ⇒ **不需要**手工插 `Tonemapping`；也**不要**改成 `Tonemapping::None`。
- **`Tonemapping::None` 在 0.20 是「完全直通」**（迁移指南原文级别的事实）：`ColorGrading` / `DebandDither` 都不生效、负通道不再 clamp。
  「关掉色调曲线」的正确表达是 `Linear`，不是 `None`。
- **不要给 `Camera2d` 加 `Hdr`**：`Hdr` 是相机组件（`bevy::camera::Hdr`），加了之后 tonemapping pass 会**真正运行**，
  并且中间渲染纹理变成 HDR（更多带宽/一次全屏 pass），对 2D 像素画面零收益。
- **`hdr` cargo feature ≠ 相机 HDR**：`hdr` 只是「HDR 图片格式支持」（`.hdr`/`.exr` 解码），在 feature 表里属 `common_api` 集合，与本条无关。【已验证】<https://docs.rs/bevy/0.20.0/bevy/>（§Collections 表 `common_api` 行含 `hdr`）
- **`tonemapping_luts` 为什么不需要**：只有 `AgX` / `TonyMcMapface` / `BlenderFilmic` 三种曲线需要 LUT。
  我们用 `Linear`，因此在 `09` 的 feature 决策里**不开** `tonemapping_luts`，也就避开了 feature 表警告的现象 —— 原文：
  “If everything is pink, you need to enable this feature or change the `Tonemapping` method for your `Camera2d` or `Camera3d`.”
- 证据：【已验证】`Tonemapping` 变体表 <https://docs.rs/bevy/0.20.0/bevy/core_pipeline/tonemapping/enum.Tonemapping.html> ；
  `Core2dPlugin` 运行期注册 <https://docs.rs/bevy_core_pipeline/0.20.0/x86_64-unknown-linux-gnu/src/bevy_core_pipeline/core_2d/mod.rs.html> ；
  迁移指南 §“`Tonemapping::None` is now a full passthrough” / §“Tonemapping and DebandDither moved”
  <https://bevy.org/learn/migration-guides/0-19-to-0-20/> ；`Hdr` 组件 <https://docs.rs/bevy/0.20.0/bevy/camera/struct.Hdr.html> ；
  feature 表 `tonemapping_luts` 行 <https://docs.rs/bevy/0.20.0/bevy/>。

---

## 7.4 窗口配置

```rust
App::new().add_plugins(
    DefaultPlugins
        .set(ImagePlugin::default_nearest()) // §7.3.1
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: WINDOW_TITLE.into(),
                // ★ 像素风必须：锁死 1 逻辑像素 = 1 物理像素
                resolution: WindowResolution::new(WINDOW_W, WINDOW_H)
                    .with_scale_factor_override(1.0),
                // 60 Hz 固定步长 + 像素完美 ⇒ 跟随显示器刷新率，避免撕裂
                present_mode: PresentMode::AutoVsync,
                mode: WindowMode::Windowed,
                resizable: true,
                ..default()
            }),
            ..default()
        }),
);
```

### 7.4.1 字段名（0.20）

| 类型 | 相关字段 / 方法 | 证据 |
|---|---|---|
| `WindowPlugin` | `primary_window: Option<Window>`、`primary_cursor_options`、`exit_condition`、`close_when_requested` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowPlugin.html> |
| `Window` | `present_mode`、`mode`、`position`、`resolution`、`title`、`name`、`composite_alpha_mode`、`resize_constraints`、`resizable`、`enabled_buttons`、`decorations`、`transparent`、`focused`、`window_level`、`canvas`、`fit_canvas_to_parent`、`prevent_default_event_handling`、`internal`、`ime_enabled`、`ime_position`、`window_theme`、`visible`、`skip_taskbar`、`clip_children`、`desired_maximum_frame_latency` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.Window.html>（`pub struct Window { … }` 字段清单） |
| `WindowResolution` | `WindowResolution::new(physical_width: u32, physical_height: u32)`（**参数名是 physical**）、`.with_scale_factor_override(f32)`、`.scale_factor()`、`.base_scale_factor()`、`.set_scale_factor_override(f32)` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowResolution.html> |
| `Window` 便捷读取 | `width() -> f32`（逻辑像素）、`height() -> f32`、`size() -> Vec2`、`physical_width() -> u32`、`physical_size() -> UVec2` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.Window.html> |
| `PresentMode` | `AutoVsync = 0`、`AutoNoVsync = 1`、`Fifo = 2`、`FifoRelaxed = 3`、`Immediate = 4`、`Mailbox = 5` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/enum.PresentMode.html> |
| `WindowMode` | `Windowed`、`BorderlessFullscreen(MonitorSelection)`、`Fullscreen(MonitorSelection, VideoModeSelection)` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/enum.WindowMode.html> |
| 尺寸变化消息 | `MessageReader<WindowResized>`，字段 `{ window: Entity, width: f32, height: f32 }`，实现 `Message`（**不在 prelude**，显式 `use bevy::window::WindowResized;`） | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowResized.html> |

### 7.4.2 像素风推荐与理由

| 设置 | 取值 | 理由 / 不这么设的后果 |
|---|---|---|
| `with_scale_factor_override(1.0)` | **必选** | OS 的 DPI 缩放（Windows 125%/150%）会引入 `logical ≠ physical`。`Window::width()` 文档明确返回**逻辑像素**，`k` 的计算会失配；更糟的是 320×320 的画面会被 OS 再做一次非整数缩放（模糊）。锁成 1.0 后「1 逻辑像素 = 1 物理像素」，`k` 才严格是整数倍【INFERENCE，基于 `width()` 的文档语义】 |
| `PresentMode::AutoVsync` | 默认值，保持 | 与 60 Hz 固定步长（`00` §0.1 **D11**）对齐，无撕裂。`AutoNoVsync`（`Immediate`→`Mailbox`→`Fifo`）会跑满 GPU、产生撕裂，只在测「真实帧时间上限」时临时一用 |
| `Immediate` / `Mailbox` | **禁止** | 文档原文：两者在不支持的平台上会 **panic**（“will panic if not supported by the platform”） |
| `mode` | `Windowed` | 开发期首选，便于看日志/切窗口。发布版若要 `BorderlessFullscreen(MonitorSelection::Current)`，必须复核 `k` 在目标分辨率的整数性 |
| `resizable` | `true` | 允许；`fit_canvas` 每次窗口变化重算 `k`，画面始终居中 + 黑边 |
| `WINDOW_W/H` = `2 × VIEWPORT` | 见 §7.1.1 | 启动即 `k = 2`，无黑边 |
| `title` | `WINDOW_TITLE` | 常量集中管理（`00` §0.5） |

---

## 7.5 `ScalingMode` 选型

0.20 全部变体（【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/enum.ScalingMode.html>；`ScalingMode` 在 `bevy::camera`，**不在 prelude**）：

| 变体 | 语义 | 本项目用法 |
|---|---|---|
| `WindowSize` | 匹配视口（窗口 / 渲染目标）尺寸；`scale = 1` 时 1 世界单位 = 1 像素 | **外层相机**：`scale = 1/k` |
| `Fixed { width: f32, height: f32 }` | 手动指定投影覆盖的世界范围，忽略视口尺寸（会按需拉伸） | **内层相机**：`Fixed { VIEWPORT_W, VIEWPORT_H }` + `scale = 1.0` |
| `AutoMin { min_width, min_height }` | 保纵横比，不小于给定值 | 不用 |
| `AutoMax { max_width, max_height }` | 保纵横比，不大于给定值 | 不用 |
| `FixedVertical { viewport_height }` | 固定高度，宽度随纵横比调整 | 不用（见下） |
| `FixedHorizontal { viewport_width }` | 固定宽度，高度随纵横比调整 | 不用（见下） |

### 7.5.1 内层相机为什么用 `Fixed { VIEWPORT_W, VIEWPORT_H }` 而不是 `FixedVertical`

- 内层的契约是「**视野恰好一屏**进渲染目标」（`00` §0.1 **D5/D13**）。`Fixed { VIEWPORT_W, VIEWPORT_H }` 把这个契约**显式写在代码里**：
  即使有人改了渲染目标的尺寸，投影覆盖的世界范围仍然是 `VIEWPORT_W × VIEWPORT_H`，不会突然多/少半个瓦片。
- `FixedVertical { viewport_height }` 只在「渲染目标恰好是正方形」时才等价；目标一旦不是正方形，宽度会按纵横比变化 ⇒ 契约被破坏。
  本项目渲染目标恒为方形（一屏），所以两者当前等价，**但前者把前提写死了**，是可维护性更好的选择。
- `scale = 1.0` ⇒ 1 世界单位 = 1 texel，与 `00` §0.3 的「1 世界单位 = 1 像素」契约一致。
- 若要显式断言这条契约，可在 `Startup` 加一句：`assert_eq!(VIEWPORT_W, VIEWPORT_H)`（视口是方形）。

### 7.5.2 内层与外层的关系（唯一耦合点）

| | 内层 | 外层 |
|---|---|---|
| `ScalingMode` | `Fixed { VIEWPORT_W, VIEWPORT_H }` | `WindowSize` |
| `scale` | 恒 `1.0`（不动） | `1 / k`，由 `fit_canvas` 写 |
| 决定的事 | **哪些世界坐标**进入那 `VIEWPORT_W × VIEWPORT_H` 个 texel | 那 `VIEWPORT_W × VIEWPORT_H` 个 texel **被放大到多少物理像素** |
| 不决定的事 | 窗口尺寸、黑边（完全不关心窗口） | 世界内容（它只看见一个 sprite） |

- 两者**互不影响**：改窗口只改 `k`，不改内层；改内层投影只改「房间在纹理里的位置」，不改缩放。
- **唯一耦合点**：内层渲染目标的像素尺寸必须等于外层那个 canvas sprite 的源纹理尺寸（同一个 `Image` 句柄，天然满足）。
  因此「`RenderTarget::Image` 的 `Image`」与「`Sprite` 的 `Image`」**必须**来自同一次 `images.add(...)`。
- `Camera { viewport: Option<Viewport> }` **不使用**。变体做法（把外层相机限制成居中的 `k·VIEWPORT × k·VIEWPORT` 物理矩形）在本项目没有收益：
  黑边已经是清屏色，再加 `Viewport` 只会多一处能与 `k` 失配的手算（`Viewport` 字段：`physical_position: UVec2`、`physical_size: UVec2`、`depth: Range<f32>`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Viewport.html>）

---

## 7.6 像素吸附

### 7.6.1 为什么需要

- 内层视口的 1 个 texel = 1 个世界单位。sprite 若落在非整数世界坐标上，nearest 采样会让它的**渲染宽度在 `n-1 / n / n+1` 个 texel 之间抖动**
  （`n` = 该 sprite 源纹理的整数宽度；俗称「像素爬行」/ pixel shimmer）：静止时看不出来，移动时非常明显。
- 逻辑层（`Aabb`）继续用 `f32`（`00` §0.3、`06` §6.1 约束 **C3**：位置/速度用 `f32`，渲染前才取整）。
  **取整只发生在「`Aabb`/速度 → `Transform` 的渲染投影」这一步**，物理内核里不存在任何 `round`/`floor`（`06` §6.8.2）。

### 7.6.2 唯一取整函数

```rust
// crates/game/src/render/snap.rs
/// 本项目**唯一**的像素取整入口。禁止在别处手写 `round` / `floor`。
#[inline]
pub fn snap_px(v: f32) -> f32 {
    v.round() // f32::round：四舍五入，恰好 .5 时远离 0
}
```

| 规则 | 内容 |
|---|---|
| 取整对象 | 只取整 `Transform.translation.x` 与 `.y` |
| **不动** | `translation.z`、`scale`、`rotation` |
| 函数 | `f32::round()`（std）。**禁止**改用 `(x + 0.5).floor()` 或 `floor()` |
| 与 `06` 的关系 | `06` §6.8.2 的 `render_center = aabb.center().round()` 用的是同一条规则；两个系统共用 `snap_px` |

**为什么是 `round` 而不是 `floor`**：本项目所有可渲染单元的边长都是**偶数**（`TILE_PX` 为偶数，见 `00` §0.3；角色/敌人 AABB 尺寸见 `06`），
且所有世界空间 sprite 统一用 `Anchor::BOTTOM_CENTER`（`04` §4）。以整数**底边中点**定位、且尺寸为偶数时，sprite 的四条像素边界都落在整像素上 ⇒ 与瓦片网格严格对齐。
`floor` 会引入 −0.5 的系统性偏移（对正坐标偏左/偏下半个像素），且对负坐标行为不直观（世界原点在地图左下角，坐标可为负，见 `00` §0.3）。
`(x + 0.5).floor()` 与 `round()` 在正数上等价、在恰好 `.5` 的负数上不同 ⇒ 两者**不得混用**，统一 `f32::round()`。

### 7.6.3 调度位置（**本节的结论与 `05` 的调度表有冲突，以本文为准**）

```rust
app.configure_sets(PostUpdate, GameSet::Snap.before(TransformSystems::Propagate));
app.add_systems(
    PostUpdate,
    (sync_player_transform, snap_render_transforms) // 两个系统都在 GameSet::Snap 内
        .chain()
        .in_set(GameSet::Snap),
);
```

- `sync_player_transform` = `06` §6.8.2 的「`Aabb` → `Transform` 投影」（命名沿用 `06`；实际覆盖玩家与其它物理驱动实体），内部使用 §7.6.2 的 `snap_px`。
- `snap_render_transforms` = 通用兜底：`Query<&mut Transform, With<PixelSnap>>`，对 `translation.x/y` 求 `snap_px`（幂等）。
- 系统集：`GameSet::Snap`（定义与装配归 `05`）。
- **必须在 `TransformSystems::Propagate` 之前**。依据（原文）：
  > “This system runs during `PostUpdate`. If you update the `Transform` of an entity in this schedule or after, you will notice a 1 frame lag before the `GlobalTransform` is updated.”
  【已验证】<https://docs.rs/bevy/0.20.0/bevy/transform/components/struct.GlobalTransform.html>
  以及 `TransformSystems::Propagate` 确实在 `PostUpdate` 运行。【已验证】<https://docs.rs/bevy_transform/0.20.0/x86_64-unknown-linux-gnu/src/bevy_transform/plugins.rs.html>（`add_systems(PostUpdate, (… mark_dirty_trees, propagate_parent_transforms, …).chain().in_set(TransformSystems::Propagate))`）
- 把吸附放在 `Propagate` **之后**会有两个后果：
  1. 本帧提取用的 `GlobalTransform` 仍是**未取整**的值 ⇒ 像素爬行照样发生（吸附形同虚设）；
  2. 即便某帧侥幸生效，也永远慢一帧（渲染位置落后逻辑 1 帧）。
- ⚠️ **冲突声明**：`05` 的调度表把 `GameSet::Snap` 记为 `.after(TransformSystems::Propagate)`。`00` §0.2 把「像素吸附」判归 `07`，
  因此**此处以本文为准**，`05` 的表需同步为 `.before(TransformSystems::Propagate)`。
  `10` §10.2.9 里「`PostUpdate` 且位于 `TransformSystems::Propagate` 之后」的要求只针对**读取** `GlobalTransform` 的鼠标↔世界转换（`viewport_to_world_2d`），
  不适用于**写入** `Transform` 的吸附系统。

### 7.6.4 哪些实体需要吸附

- 统一做法：所有由本项目 spawn 的可渲染实体都带标记组件 `PixelSnap`，吸附系统 `Query<&mut Transform, With<PixelSnap>>`。
- `PixelSnap` 由 `render` 模块的构造器（`pixel_sprite(..)`、`pixel_text2d(..)`、相机 spawn 助手）自动加上，避免漏标。
- **`PixelSnap` 只挂在 root 实体上**。带父实体的瓦片（§7.8 的 `RoomRoot` 子实体）不挂：父级世界位置是整像素 + 子级局部偏移是整像素（相对**房间中心**：`tx*TILE_PX + TILE_PX/2 − ROOM_PX/2`, `ty*TILE_PX − ROOM_PX/2`）⇒ 子级世界位置天然是整数。
  这条与 `04` §6.1 的「装饰品 `offset` 必须是整数像素」是同一约束的两种表述。
- 相机（内层）同样带标记：`02` 的跟随目标由玩家 AABB 中心与整数关卡尺寸算出，吸附是幂等的兜底。

### 7.6.5 渲染插值的边界（本地不插值，客户端远端例外）

| # | 理由 |
|---|---|
| 1 | **收益被取整吃掉**：插值产生亚像素位置，吸附又把它取整回整数 ⇒ 绝大多数插值结果被丢弃（`06` §6.8.2 同一结论）。 |
| 2 | **视野小于一个房间**（`00` §0.1 **D5**）：画面的绝大部分是静止瓦片，插值只对角色/敌人可见，收益面极小。 |
| 3 | **本地实体不插值**：渲染帧率 ≠ 逻辑帧率时，重复帧是正确表现；插值会制造「同一逻辑位置落在不同 texel」的抖动。 |
| 4 | **契约成本**：插值只允许作为**渲染层**状态，不得回写 `Aabb`。本地物理/相机仍读 `Aabb`。 |

> 结论：
> - 本地实体：`snap_px(aabb.center())` → 写 `Transform`，**不做**位置插值。
> - 相机：每个视图直接跟随自己的玩家 AABB 中心（无平滑，`02` §2.2）。
> - **联机客户端的远端玩家**：快照以 60 Hz 到达、且没有本地物理；为了消除「停几帧 → 跳一段」的拖影感，
>   用 `RenderCenter` 在两次快照之间做**线性渲染插值**（`net/client.rs`），最多增加一个快照间隔的视觉延迟。
>   `PlayerMotion(Aabb)` 仍直接使用快照权威值，逻辑/相机/自检不读插值结果。

---

## 7.7 Z 层落地

### 7.7.1 数值来源

**Z 数值的唯一来源是 `00` §0.4 的分层表**（远景 → 相机共 12 行）。本文只做三件事：
① 定义 Rust 常量名（§7.1.3 的表）；② 说明 0.20 的排序机制；③ 给出「同 Z 不得重叠」的工程结论与检查方法。**任何一次复制数值都算违规。**

### 7.7.2 排序机制（0.20）

- 2D 透明排序依据**视图空间 Z**（即 `Transform.translation.z`），使用 **`FloatOrd` 的快速不稳定排序**，小值先画。【已验证】
  `Transparent2d` 实现 `SortedPhaseItem`、`SortKey = FloatOrd`：
  <https://docs.rs/bevy/0.20.0/bevy/core_pipeline/core_2d/struct.Transparent2d.html>
- `bevy_ui` 的 `ZIndex` / `GlobalZIndex` **只对 `Node` 有效**，对 `Sprite` 无效。【已验证】<https://docs.rs/bevy/0.20.0/bevy/ui/struct.ZIndex.html>
- 所有 sprite 的 Z **必须显式写入**、取值只能来自 §7.1.3 的常量；禁止依赖 spawn 顺序（`00` §0.4 末条）。

### 7.7.3 工程结论：同 Z 重叠顺序无保证

> **任何彼此可能重叠的 sprite 不得共享 Z。**（`00` §0.4 的硬规则）

后果：两个 sprite 若 Z 相同且屏幕投影相交，绘制顺序在帧间可能翻转 ⇒ 表现为闪烁、前后跳变、边缘互相啃咬。
0.20 把 `Sprite` 迁到了 Mesh2d/Material2d 后端（`Sprite` 新增 `alpha_mode` 字段），同 Z 的顺序比旧后端**更不可预测**。【已验证】
<https://bevy.org/learn/migration-guides/0-19-to-0-20/#migrate-sprite-to-use-mesh2d-spritematerial> ；
`Sprite` 字段表 <https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Sprite.html>

### 7.7.4 检查方法（三层，都要能落地）

**(a) 静态 —— CI 检查（首选，零运行成本）**

Z 只能写成常量名。在 CI 里加一条：`crates/game/src` 下不得出现「`from_xyz` 的第三个参数是裸数字」：

```bash
# 期望：无输出
grep -rnE 'from_xyz\([^)]*,[[:space:]]*-?[0-9]' crates/game/src
```

合法的 `Transform` 写法只有两种：
1. 游戏内 sprite / `Text2d`：`Transform::from_xyz(x, y, z::TILE)`（第三参必须取自 `00` §0.4 的 `z::*` 常量，此处 `z::TILE` 仅为示例）；
2. 相机 / UI 根节点：**整条 `Transform` 不写**（Z 对它们无意义，见 `00` §0.4 末行与 §7.2.2），或写 `Transform::default()`。

同理禁止 `translation.z =` 的字面量赋值（唯一例外：`config/render.rs` 里 Z 常量的定义本身）。
这条检查的价值：把「Z 只能来自常量表」从约定变成机械可验证的事实。

**(b) 运行期 —— dev 下的同 Z 重叠检测器**（首批必做）

```rust
/// dev feature 下注册，每 60 帧跑一次。
/// 输出：所有「Z 相同且屏幕投影重叠」的实体对。
fn check_same_z_overlap(
    q: Query<(Entity, &GlobalTransform, &Sprite)>,
    images: Res<Assets<Image>>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    if *tick % 60 != 0 {
        return;
    }

    // (z, entity, 屏幕 AABB)
    let mut items: Vec<(f32, Entity, Rect)> = Vec::new();
    for (entity, gt, sprite) in &q {
        let translation = gt.translation();
        let size = sprite.custom_size.unwrap_or_else(|| {
            if sprite.texture_atlas.is_some() {
                // 图集单元严格 32×32、零 padding（04 §3.1）⇒ 尺寸恒为 TILE_PX
                Vec2::splat(TILE_PX as f32)
            } else {
                // 找不到纹理时退化为 1×1（宁可漏报，不可 panic）
                images
                    .get(&sprite.image)
                    .map(|img| img.size_f32())
                    .unwrap_or(Vec2::ONE)
            }
        });
        items.push((
            translation.z,
            entity,
            Rect::from_center_size(translation.truncate(), size),
        ));
    }

    // 按 Z 排序后只比较相邻对：O(n log n)（同 Z 组内退化为 O(m²)，本项目 m ≤ 256，可接受）
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    for pair in items.windows(2) {
        let (za, ea, ra) = pair[0];
        let (zb, eb, rb) = pair[1];
        // ★ 用「正面积相交」而不是「接触」：相邻瓦片共边不算重叠，用 intersects() 会误报满屏
        let overlaps = ra.min.x < rb.max.x
            && rb.min.x < ra.max.x
            && ra.min.y < rb.max.y
            && rb.min.y < ra.max.y;
        if za == zb && overlaps {
            warn!("same-Z overlap: z={za}, {ea:?}, {eb:?}");
        }
    }
}
```

- 复杂度：按 Z 排序后只比较相邻对，O(n log n)（同 Z 组内若实体数很大，退化到 O(m²)，本项目 m ≤ 256，可接受）。
- **必须用正面积相交**：相邻瓦片的 AABB 共边，用 `intersects()` 会误报满屏。上式的严格不等号就是为此。
- 用到的 API 全部已取证：`Sprite { image, texture_atlas, custom_size, .. }`（0.20 字段表）、`Image::size_f32() -> Vec2`、`Rect::from_center_size(center, size)`、`GlobalTransform::translation() -> Vec3`。
  【已验证】<https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Sprite.html> ；<https://docs.rs/bevy/0.20.0/bevy/image/struct.Image.html> ；<https://docs.rs/bevy/0.20.0/bevy/math/struct.Rect.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.GlobalTransform.html>
- 实体上必须有 `Name`（`10` 的约定）才能打印名字；本检测器只打印 `Entity`，避免依赖 `Name` 一定存在。

**(c) 可视化 —— 用 `Gizmos` 高亮可疑实体**（见 §7.9）：检测器命中时把实体 id 记进一个 `Resource`，`Gizmos` 系统下一帧给它们画 `rect_2d` 外框。

---

## 7.8 瓦片渲染策略

### 7.8.1 结论（先说结论）

> **`bevy_ecs_tilemap` 及其下游在 0.20 上不可用 ⇒ 本项目自研 sprite 瓦片渲染**（每瓦片一个 `Sprite` 实体 + `TextureAtlas`）。
> 首批**做房间粒度的可见性裁剪**，**不做**逐瓦片/分块裁剪。

**证据（0.20 不可用）**

| crate | 最新版 | 兼容的 bevy | 结论 | 证据 |
|---|---|---|---|---|
| `bevy_ecs_tilemap` | `0.19.0`（2026-07-04） | 最新只到 **0.19**（依赖行 `bevy ^0.19.0`） | **不可用** | <https://docs.rs/crate/bevy_ecs_tilemap/latest>（依赖清单 + README 兼容表最高行 `0.18→0.18`；`main` 声称 track bevy，无 0.20 行） |
| `bevy_ecs_ldtk` | `0.15.0`（2026-07-05） | 最新只到 **0.19**（自身 + `bevy_ecs_tilemap ^0.19.0` 双阻塞） | **不可用** | <https://docs.rs/crate/bevy_ecs_ldtk/latest> |
| `bevy_ecs_tiled` | `0.13.4`（2026-08-24） | 最新只到 **0.19**（同上） | **不可用** | <https://docs.rs/crate/bevy_ecs_tiled/latest> |

⇒ LDtk / Tiled 工作流在 0.20 上同样不可选。依赖矩阵与「观望/替换条件」见 `09` §9.2、§9.3。

### 7.8.2 实体结构

```
RoomRoot { col, row }          // 父实体：Transform = 房间中心（世界整像素），Visibility 由裁剪系统控制
  ├─ TileSprite   z::TILE               // 实体瓦片（Solid / OneWay / Hazard，语义见 03/06）
  ├─ TileSprite   z::TILE
  ├─ TileDecal    z::TILE_DECAL         // 纯外观贴花（草、砖纹）
  ├─ TileSprite   z::TILE_FOREGROUND    // 前景瓦片
  └─ RoomBg       z::ROOM_BG            // 房间背景板（整房间一张，不是逐瓦片）
```

| 规则 | 内容 |
|---|---|
| **只生成非空瓦片** | `Air` 不生成任何实体。【`03` §3.3 明确「`07` 的瓦片渲染规模裁剪依赖这一点」】（语义与 legend 见 `03` §3.3） |
| **按房间分组** | 一个房间 = 一个 `RoomRoot` + 若干子实体（用 `ChildOf`，见 `10` §10.2.3） |
| 图集 | 单一图集 `assets/sprites/tiles/tileset.png`（路径/物理布局见 `04` §10；索引语义见 `03` §3.3）。`TextureAtlasLayout::from_grid(UVec2::splat(TILE_PX), cols, rows, None, None)`，列/行数由 `04` §10 的算法给出，**不得**另立布局 |
| 锚点 | `Anchor::BOTTOM_CENTER`，Transform = 瓦片 **AABB 底边中点**（`min + (TILE_PX/2, 0)`），与 `04` §4 的全局约定一致。`Anchor` **不在 prelude**，必须 `use bevy::sprite::Anchor;`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html> |
| Z | 只能取 `z::TILE` / `z::TILE_DECAL` / `z::TILE_FOREGROUND` / `z::ROOM_BG`（数值见 `00` §0.4）。**同一层内的瓦片互不重叠**（网格无重叠），因此可以共 Z；跨层必须有不同 Z |
| 渲染层 | 所有瓦片带 `PIXEL_PERFECT_LAYERS`（layer 0） |
| 吸附 | 子实体**不挂** `PixelSnap`（局部偏移是 `TILE_PX` 整数倍，父级是整像素，见 §7.6.4） |

**哪些实体挂 `RoomRoot` 下，哪些不挂**

| 类别 | 归属 | 理由 |
|---|---|---|
| 瓦片（实体瓦片 / 贴花 / 前景）、房间背景板 | `RoomRoot` 的**子实体** | 生成后位置固定；整房间隐藏 = 改父级一个 `Visibility` |
| 会移动的实体（角色、敌人、可拾取物、粒子、装饰品） | **世界根**（不是 `RoomRoot` 的子） | 它们随 `Aabb` 移动（`06`），若成为 `RoomRoot` 的子，房间切换会连带移动它们；它们的房间归属用 `00` §0.3 的 `world_to_room` 计算（相机/房间状态见 `02`） |

### 7.8.3 房间可见性裁剪（M2 起：按**相机视野矩形**判定）

**结论：做，但只做房间粒度，且判定条件是「与相机视野矩形相交」而不是「是否当前房间」。**

| 粒度 | 现役 | 理由 |
|---|---|---|
| 房间粒度（视野相交） | ✅ **做** | 把不可见的 `RoomRoot` 设为 `Visibility::Hidden` ⇒ 其全部 `Inherited` 子实体一并隐藏。判定：房间 AABB 与**任一激活视图**的「相机中心 ± `VIEWPORT_W/2`（Y 为 `VIEWPORT_H/2`）」做**严格重叠**（共边不算）。玩家靠近房间边界时视野会同时覆盖相邻房间（最多四个）；本地双人两个视图分别判定后取并集，不会露出黑块。 |
| 逐瓦片 / 分块 | ❌ **不做** | 同房间内所有瓦片都在视口内（或部分在视口外时由 Bevy 的视锥剔除处理），逐瓦片裁剪**收益恒为 0**。 |

- 早期结论（"只显示当前房间"）在 M1 时成立，因为相机尚未移动；相机跟随玩家后必须改为视野判定，否则边界处会有一半画面是黑的。
- `N = 2` 时两个玩家可能在不同房间：任一视图能看到就显示；两个 `RoomRoot` 的可见性是统一集合，不按视图拆子树。
- 逻辑层**不做**房间激活/卸载（`02` §2.4 的决定），视觉层的裁剪与之完全解耦。
- 写入必须**幂等**（只在值变化时改 `Visibility`），避免每帧触发 change detection。

### 7.8.4 规模估算

估算基准：房间 = `ROOM_TILES × ROOM_TILES` 个瓦片（`00` §0.3），非空瓦片上界 = 一房间全部瓦片（`Air` 不生成实体）。

| 关卡规模 | 房间数 | 非空瓦片上界/房间 | 瓦片实体总量上界 | **同屏（可见）瓦片实体** |
|---|---|---|---|---|
| 首批目标 `10 × 3` | 30 | 256 | **7 680** | **≤ 256** |
| `01` 的规模上限（单关卡 ≤ 64 房间） | 64 | 256 | 16 384 | ≤ 256 |

- 每个瓦片实体携带：`Sprite`（含 `texture_atlas: Option<TextureAtlas>`）、`Transform`、`GlobalTransform`（自动插入）、`Visibility` + `InheritedVisibility` + `ViewVisibility`（`Visibility` 的 required components）、`ChildOf`、`RenderLayers` ⇒ 约 8–10 个组件。
  **7 680 个实体是安全量级**（【INFERENCE】：0.20 的 `Sprite` 走 Mesh2d/Material2d 批处理后端，提取后由批渲染处理；具体的批次划分行为未取证）。
- **同屏只有 ≤ 256 个瓦片实体**才是真正的渲染负载 —— 房间裁剪的核心价值就在这里。

**升级触发条件（写死，避免含糊的「以后再说」）**

1. 单关卡房间数 > 64（`01` 的规模上限），或
2. dev 构建下 `FrameTimeDiagnosticsPlugin` 的 `frame_time` p99 > 16.6 ms（60 Hz 预算，`00` §0.1 **D11**）。

触发后**按此顺序**升级，不要跳步：
① 保持「一房间一实体树」，改为**按需实体化**（只有当前房间被 spawn，切房间时 despawn/spawn）；
② 若 spawn 抖动明显，改为**分块常驻 + 池化**（房间 = 天然 chunk）；
③ 仍不足，才考虑把一房间瓦片烘成单张 mesh（自定义 `Mesh2d` + `Material2d`）。

---

## 7.9 调试可视化

全部调试内容在 `dev` feature 下注册（我们自己的 feature，「生产构建零成本」的策略见 `09` §9.5）。
**Debug 覆盖层不得使用游戏 Z 表之外的绘制方式**：它的绘制目标就是「相机视图」，因此要么用 `Gizmos`（不参与 sprite 排序），要么用 UI。

### 7.9.1 `Gizmos`（AABB / 房间网格线）

- 类型：`Gizmos` 是 `SystemParam`（即时模式，每帧重建、帧末清空）。【已验证】`pub struct Gizmos<'w,'s,Config = DefaultGizmoConfigGroup, Clear = ()>`、`SystemParam`：
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Gizmos.html>
- 本项目用到的方法（**0.20 精确形态**，均来自 docs.rs 页面内的官方 example 源码）：

| 调用 | 用途 |
|---|---|
| `gizmos.rect_2d(Isometry2d::IDENTITY, Vec2::splat(650.), BLACK)` / `gizmos.rect_2d(center: Vec2, size: Vec2, color)` | AABB、房间边界框（首参是 `impl Into<Isometry2d>`，`Vec2` 中心与 `Isometry2d` 都能传） |
| `gizmos.line_2d(a: Vec2, b: Vec2, RED)` | 网格线、速度矢量 |
| `gizmos.linestrip_2d([Vec2; N], GREEN)` | 折线（如路径/接触点） |
| `gizmos.circle_2d(center: Vec2, r: f32, WHITE)`、`gizmos.cross_2d(center, size, FUCHSIA)` | 出生点、落点标记 |

- 证据：【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Gizmos.html>（页面 example：`gizmos.rect_2d(Isometry2d::IDENTITY, Vec2::splat(650.), BLACK)`、`gizmos.line_2d(Vec2::Y * -sin_t_scaled, Vec2::splat(-80.), RED)`、`gizmos.rect_2d(center, aabb.half_size() * 2., YELLOW)`、`gizmos.linestrip_2d([Vec2::ZERO, Vec2::X, Vec2::Y], GREEN)`）；`Gizmos` 在 prelude。
- 示例（碰撞盒 + 房间网格）：

```rust
/// dev：画角色/敌人的 AABB（真值来自 06 的 `Aabb` 组件，不是 `Transform`）
fn gizmo_colliders(q: Query<&Aabb, With<DebugDraw>>, mut gizmos: Gizmos) {
    for aabb in &q {
        gizmos.rect_2d(aabb.center(), aabb.size(), Color::srgba(0.0, 1.0, 0.0, 0.6));
    }
}

/// dev：只画**当前房间**的 `TILE_PX` 网格（每轴 `ROOM_TILES + 1` 条 ⇒ 共 34 条 `line_2d`）。
/// 不画全部房间：30 房间 × 34 条 = 1020 条/帧 是无谓开销。
/// 房间中心走 00 §0.3 的唯一转换入口 `room_local_to_world`（禁止手写该算术）。
fn draw_room_grid(col: u32, row: u32, grid_rows: u32, gizmos: &mut Gizmos) {
    let center = room_local_to_world(col, row, grid_rows, Vec2::splat(ROOM_PX / 2.0));
    let half = ROOM_PX / 2.0;
    let color = Color::srgba(1.0, 1.0, 1.0, 0.15);
    let mut offset = 0.0;
    while offset <= ROOM_PX {
        let o = -half + offset;
        // 竖线 / 横线各一条
        gizmos.line_2d(
            Vec2::new(center.x + o, center.y - half),
            Vec2::new(center.x + o, center.y + half),
            color,
        );
        gizmos.line_2d(
            Vec2::new(center.x - half, center.y + o),
            Vec2::new(center.x + half, center.y + o),
            color,
        );
        offset += TILE_PX;
    }
}

/// 薄系统：只负责把 02 的「当前房间」状态取出来喂给上面的纯函数。
/// `CurrentRoom` / 关卡网格尺寸的类型与字段归 `02` / `03`，此处不重复定义。
fn gizmo_room_grid(current: Res<CurrentRoom>, level: Res<LoadedLevel>, mut gizmos: Gizmos) {
    draw_room_grid(current.col, current.row, level.grid_rows, &mut gizmos);
}
```

- 房间中心**必须**用 `00` §0.3 的 `room_local_to_world(col, row, grid_rows, local)` 得到（本项目的唯一坐标入口；禁止在渲染代码里手写房间中心的算术）。
- `ROOM_PX` / `TILE_PX` / `ROOM_TILES` 的定义见 `00` §0.3（本文不复制数值）。
- `Gizmos` 的画线宽度、是否受相机影响等配置走 `GizmoConfigStore`（**本项目不改默认配置**，保持 1 texel 线宽，避免亚像素线）。

### 7.9.2 帧时间与调试覆盖层

| 工具 | 路径 | 用法 | feature 依赖 |
|---|---|---|---|
| `FrameTimeDiagnosticsPlugin` | `bevy::diagnostic`（**不是** `dev_tools`） | 由 `FpsOverlayPlugin` 自动添加（见下）；**不要**重复显式添加 | 【已验证】feature 列表中**不存在** `diagnostic` 项 ⇒ 不受 feature 门控 <https://docs.rs/bevy/0.20.0/bevy/> |
| `EntityCountDiagnosticsPlugin` | 同上 | 观察实体数（§7.8.4 的规模是否失控） | 同上 |
| `LogDiagnosticsPlugin` | 同上 | 需要把指标打进日志时加 | 同上 |
| `FpsOverlayPlugin` | `bevy::dev_tools::fps_overlay` | **本项目禁用**（见下方陷阱说明）：改由本模块自己的 `Node`+`Text` 显示 FPS 与实体数 | 它由 `bevy_dev_tools/render` 门控，该 feature 会拖入 `bevy_pbr`（3D） |

### 7.9.2.1 ⚠️ 陷阱：`bevy/dev` 集合与 2D-only feature 集不兼容（**实测启动即 panic**）

- `bevy/dev` 集合 = `debug` + `bevy_dev_tools` + `render_dev_tools` + `file_watcher`。
- `render_dev_tools` 会启用 `bevy_dev_tools/render`，该 feature 的依赖里有 **`bevy_pbr`**（3D）。
  证据：<https://docs.rs/crate/bevy_dev_tools/0.20.0/features>（`render` 行列出 `bevy_pbr ^0.20.0` 等依赖）。
- 后果（实测，本机 `rustc 1.99.1` / bevy 0.20.0 / `default-features = false, features = ["2d","ui","audio","dev"]`）：
  启动即 panic —— `bevy_dev_tools::render_debug::init_render_debug_overlay_pipeline` 的
  `Res<MeshPipelineViewLayouts>` 校验失败（该资源由 3D 网格管线注册，本项目不含 `3d` profile）。
  `MeshPipelineViewLayouts` 类型在编译期存在（因为 `bevy_pbr` 被拖了进来），运行期资源不存在。
- **结论**：本项目**不使用** `bevy/dev` 集合，而在 `crates/game/Cargo.toml` 里逐个列出需要的子 feature：
  `dev = ["bevy/debug", "bevy/bevy_dev_tools", "bevy/file_watcher"]`。
  `FpsOverlayPlugin` 与 `render_debug` 同属 `render` 门控 ⇒ **一并不可用**，诊断信息改用本模块的 `Text` 节点显示
  （`FrameTimeDiagnosticsPlugin` 与 `DiagnosticsStore` 都不受 feature 门控，照常可用）。

```rust

/// 调试绘制总开关（dev 构建下默认开；F3 切换，切换系统只读 `ButtonInput<KeyCode>`）
#[derive(Resource)]
pub struct DebugDrawEnabled(pub bool);

impl Default for DebugDrawEnabled {
    fn default() -> Self {
        // 不要写成 `Self(cfg!(feature = "dev"))`：clippy 会误以为能 `#[derive(Default)]`，
        // 而 derive 得到的是 `false`（与「dev 下默认开」相反）。
        #[cfg(feature = "dev")]
        let enabled = true;
        #[cfg(not(feature = "dev"))]
        let enabled = false;
        Self(enabled)
    }
}

fn debug_enabled(enabled: Res<DebugDrawEnabled>) -> bool {
    enabled.0
}

/// F3 的判定逻辑（纯函数，便于单测；`just_pressed` 才翻转，长按不重复翻转）
pub(crate) fn toggled(keys: &ButtonInput<KeyCode>, current: bool) -> Option<bool> {
    keys.just_pressed(KeyCode::F3).then_some(!current)
}

fn toggle_debug_draw(keys: Res<ButtonInput<KeyCode>>, mut enabled: ResMut<DebugDrawEnabled>) {
    let Some(next) = toggled(&keys, enabled.0) else {
        return;
    };
    enabled.0 = next;
}

#[cfg(feature = "dev")]
pub fn add_debug_visualization(app: &mut App) {
    app.init_resource::<DebugDrawEnabled>()
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_systems(Update, (gizmo_colliders, gizmo_room_grid).run_if(debug_enabled))
        .add_systems(Update, (check_same_z_overlap, toggle_debug_draw).run_if(debug_enabled));
}
```

- 生产构建调用的是 `#[cfg(not(feature = "dev"))] pub fn add_debug_visualization(app: &mut App) { app.init_resource::<DebugDrawEnabled>(); }`（同一函数名、空实现）—— 这样调用点不必写 `cfg`。
- `Aabb`（`06`）的 `center()` / `size()` 是 `06` 提供的读取方法；渲染侧只读不写。
- F3 文本行（`render/debug.rs::update_debug_text`）包含：FPS/实体数、`GameState`（`Debug` 名）、像素缩放倍率、
  关卡名与网格尺寸、当前房间坐标、通关数与装饰品数、当前房间是否已通关、资产根路径。
  这些都是**开发信息**，不进入常驻 HUD（§7.10.1）；缺失关卡/进度资源时对应字段显示 `-`。

- `FpsOverlayPlugin { config: FpsOverlayConfig }`，实现 `Default`；运行期改配置走 `ResMut<FpsOverlayConfig>`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/dev_tools/fps_overlay/struct.FpsOverlayPlugin.html>（`config` 字段；“this can be later be changed through `FpsOverlayConfig` resource”）
- 覆盖层内部用 `GlobalZIndex(FPS_OVERLAY_ZINDEX)` 定位（`bevy_ui` 的 Z，不参与 sprite 排序）。【已验证】模块页常量 `FPS_OVERLAY_ZINDEX` <https://docs.rs/bevy/0.20.0/bevy/dev_tools/fps_overlay/index.html>
- 官方提示：能用平台原生 overlay 时优先用（开销更低、数值更准），例如 Metal 下 `MTL_HUD_ENABLED=1`。【已验证】`FpsOverlayPlugin` 页面原文。
- 因为 UI 由 **UI 窗口相机**渲染（§7.10.1），FPS 覆盖层按 `UiScale = k` 放大到原生窗口分辨率；它仍在 `VIEWPORT` 逻辑坐标系里排版，但不与游戏画面共享像素格。

### 7.9.3 feature 依赖汇总

| 能力 | 需要的 feature | 说明 / 证据 |
|---|---|---|
| `Gizmos` 的 SystemParam | `bevy_gizmos` | 【已验证】`bevy_gizmos` 属于 `common_api`，而 `2d_api = common_api + bevy_sprite` ⇒ 使用 `09` 选定的 2D profile 时已开启。证据：<https://docs.rs/bevy/0.20.0/bevy/>（§Collections 表 `common_api` / `2d_api` 行） |
| `Gizmos` 的**渲染** | `bevy_gizmos_render` | 【已验证】属于 `2d_bevy_render`（`2d_bevy_render = 2d_api, bevy_render, bevy_core_pipeline, bevy_post_process, bevy_sprite_render, bevy_gizmos_render`）。同上 URL |
| 诊断插件（`FrameTime*` / `EntityCount*` / `Log*`） | 无 | 【已验证】feature 列表中无 `diagnostic` 项 |
| `FpsOverlayPlugin` | ❌ 本项目不用 | 它由 `bevy_dev_tools/render` 门控，该 feature 拖入 `bevy_pbr` 并让 `render_debug` 的初始化系统 panic（见 §7.9.2.1） |
| 资产热重载（`.level.ron`） | `file_watcher`（`dev` 集合展开项） | 见 `09` §9.4、`08` |
| 我们自己的 `dev` feature | 见 `09` §9.5 | 生产构建不启用（官方原文：`dev` “should not be enabled for published apps!”）<https://docs.rs/bevy/0.20.0/bevy/> |
| 自定义 feature 集合（`default-features = false`） | 见 `09` | 【UNVERIFIED】上两行「属于哪个集合」的结论只对 `2d`/`ui` 等 profile 成立；自建集合必须回查 <https://docs.rs/bevy/0.20.0/bevy/> 的 feature 表 |

---

## 7.10 HUD 与文字

### 7.10.1 选型决策表

| 用途 | 用什么 | 坐标空间 | 由哪个相机渲染 |
|---|---|---|---|
| **游戏 HUD**：房间目标进度（含收集计数）、提示条 | `Node` + `Text`（`bevy_ui`） | 屏幕空间，逻辑像素 = **`VIEWPORT` 坐标系**（320 宽，靠 `UiScale = k` 保持） | **UI 相机**（窗口目标，`IsDefaultUiCamera`） |
| **提示条**（通关 / 获得装饰品） | 同上，单行文本 + `HUD_TOAST_SECS` 计时 | 同上 | 同上 |
| **结算面板**（`GameComplete`） | 固定 px 几何的 `Node` + 居中 `Text` | 同上 | 同上 |
| **世界内文字**：告示牌、道具名、飘字 | `Text2d` | 世界像素（Z = `z::TEXT2D`，数值见 `00` §0.4） | 内层相机（走 `PIXEL_PERFECT_LAYERS`，与 sprite 一起被放大 `k` 倍） |
| 编辑器 UI | `bevy_egui` | 原生窗口分辨率 | 见 `08` |

**为什么 UI 走独立窗口相机（而不是内层相机）**

- **可点击性是硬约束**：`bevy_picking` 的 UI 后端按「相机 `RenderTarget` == 指针 `NormalizedRenderTarget`」把指针分配给相机。
  内层相机的目标是离屏纹理，指针永远匹配不上 ⇒ `PointerPress` 不会发给任何 UI 节点 ⇒ 按钮点不动。
- **字体精细度**：内层目标只有 320×320，12 px 字形先在该分辨率光栅化、再最近邻放大 `k` 倍，看起来就是「低像素文本放大」。
  UI 相机直接渲染到窗口，同一字号按 `k` 倍物理像素光栅化，边缘由 `FontSmoothing::AntiAliased`（默认）平滑。
- **布局不变**：`UiScale = k` 时 `logical_size = 视口物理尺寸 / k = (N×VIEWPORT_W) × VIEWPORT_H`。
  单人仍是 320×320；本地双人是 640×320（左右两屏）。菜单/暂停面板按 `ViewLayout` 的宽高排版，
  `Val::Px` / `FontSize::Px` 仍乘以 `k`。
- **代价**：HUD 不再与游戏画面共享像素网格（无整数 texel 对齐）。这是明确的取舍：可点击、可读优先。
- UI 相机：`Camera { order: UI_CAMERA_ORDER, clear_color: ClearColorConfig::None }`、`RenderLayers::none()`、`Msaa::Off`。
  `clear_color = None` 保证它不擦掉外层相机画好的 canvas；`RenderLayers::none()` 保证它不重复渲染世界。
- **UI 相机的 `Viewport` 必须等于游戏区域在窗口中的居中矩形**
  （尺寸 `(N×VIEWPORT_W×k) × (VIEWPORT_H×k)`，位置 `(窗口 - 画布)/2`，整数像素）。
  否则全屏时逻辑尺寸会随窗口比例变化，菜单按固定坐标写就会偏。
  限定视口后逻辑尺寸恒为 `(N×VIEWPORT_W) × VIEWPORT_H`，菜单/HUD 与画布严格重合，黑边只留在窗口两侧。
- UI 节点不受 `RenderLayers` 影响，只能被一个相机渲染。【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/visibility/struct.RenderLayers.html>
- **所有 UI 尺寸/字号都按 `ViewLayout` 的逻辑尺寸写**（单人 `VIEWPORT_W` 宽，本地双人 `2×VIEWPORT_W` 宽），且**一律用 `px(..)` 的整数值**。
  **禁止**用「窗口/视口比例」单位（`vw()` / `vh()` / `percent()`）做 HUD 的定位与尺寸：它们会解析出小数像素。
  唯一例外是「铺满视口」这类不关心像素对齐的容器。

### 7.10.2 0.20 正确写法（可直接抄写）

```rust
use bevy::prelude::*;

/// 屏幕空间 HUD（由 UI 窗口相机渲染；逻辑坐标仍按 VIEWPORT_W × VIEWPORT_H）
fn spawn_hud(commands: &mut Commands, ui_font: &UiFont) {
    commands.spawn((
        Text::new("目标：到达出口"),
        ui_font.text_font(HUD_FONT_PX), // 字体 + FontSize::Px（默认抗锯齿）
        // 不要在 UI 上关抗锯齿：原生分辨率下 `FontSmoothing::None` 会让中文边缘发毛。
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),      // 枚举名是 Justify（不是 JustifyText）
        Node {
            position_type: PositionType::Absolute,
            top: px(HUD_MARGIN_PX),
            left: px(HUD_MARGIN_PX),
            ..default()
        },
    ));
}

/// 世界空间文字（跟着被放大 k 倍，和 sprite 一个像素格）
fn spawn_world_label(commands: &mut Commands, font: &Handle<Font>, at: Vec2, text: &str) {
    commands.spawn((
        Text2d::new(text),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(WORLD_TEXT_FONT_PX),
            font_smoothing: FontSmoothing::None,
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(at.x, at.y, z::TEXT2D), // z 见 00 §0.4
        PIXEL_PERFECT_LAYERS,
        PixelSnap,                                   // §7.6.4
    ));
}
```

- 两个函数都是**构造器**（不是系统），由 `Startup` / 状态进入系统调用；系统侧只传 `&mut Commands` 与已加载的字体句柄。
- `PIXEL_PERFECT_LAYERS` 见 §7.1.2；`PIXEL_PERFECT_LAYERS` 对 UI 节点无意义（UI 忽略 `RenderLayers`），只对 `Text2d` 需要。
- **UI 窗口相机**插 `IsDefaultUiCamera`（内层/外层都不插）。UI 相机的 `RenderTarget` 必须是窗口，否则拾取后端匹配不到指针，按钮点不动。

### 7.10.3 必须记住的 0.20 细节

| 项 | 0.20 事实 | 证据 |
|---|---|---|
| 对齐枚举名 | **`Justify`**（不是 `JustifyText`）：`Left` / `Center` / `Right` / `Justified` / `Start` / `End`；`TextLayout::justify(Justify::Center)` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.Justify.html> ；`TextLayout { justify: Justify, linebreak: LineBreak }` <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextLayout.html> |
| 字号字段类型 | `TextFont { font: FontSource, font_size: FontSize, weight, width, style, font_smoothing, font_features, font_variations }`；`FontSize::Px(f32)` / `Rem(f32)` / `Vw` / `Vh` / `VMin` / `VMax` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextFont.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.FontSize.html> |
| **字号必须显式 `Px`** | `TextFont::default()` 在 0.20 是 `FontSize::Rem(1.)` ⇒ 不写就跟随 `RemSize` 资源漂移，破坏像素对齐 | 【已验证】`FontSize` 变体表（`Rem` = “relative to the value of the `RemSize` resource”）+ `10` 的 C5 警告 |
| 字体源 | `FontSource` 泛型族：`sans_serif()` / `serif()` / `monospace()` / `cursive()` / `fantasy()` / `system_ui()` / `family(name)` / `families(..)`；`Handle<Font>` 可 `.into()` | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.FontSource.html> ；example 写法 `font: font_handle.clone().into()` <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Text2d.html> |
| 抗锯齿 | `FontSmoothing::None` 文档原文：“No antialiasing. Useful for when you want to render text with a pixel art aesthetic.” ⇒ 本项目**一律** `None`；用 `AntiAliased` 会在放大 k 倍后出现灰色过渡像素 | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.FontSmoothing.html> |
| `Text2d` 的定位 | `Text2d(pub String)`；`justify` **只影响块内对齐**，位置由 **`Anchor`**（默认 `CENTER`，`bevy::sprite::Anchor`，不在 prelude）决定 | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Text2d.html>（原文：“With `Text2d` the `justify` field of `TextLayout` only affects the internal alignment … and not its relative position, which is controlled by the `Anchor` component.”） |
| 长度单位 | `px(f32)`（0.20 的长度构造函数）；`UiRect` 用 `px(8.0).all()`；`Val::Px` 亦可 | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Node.html>（`Node` 字段用 `px(..)`/`percent(..)`） |
| 字体资源 | `assets/fonts/*.ttf`（目录见 `00` §0.6，规格见 `04` §9）；由 `UiFontPlugin` 同步加载并暴露句柄 | — |

### 7.10.4 HUD 常量（本文所有）

| 常量 | 值 | 说明 |
|---|---|---|
| `HUD_FONT_PX` | `12.0` | HUD 字号（`FontSize::Px`，逻辑视口 320 宽）。本仓实际取 `12.0` |
| `HUD_MARGIN_PX` | `8.0` | HUD 距视口边缘的像素 |
| `WORLD_TEXT_FONT_PX` | `12.0` | 世界空间文字字号 |
| `HUD_LINE_HEIGHT_PX` | `20.0` | 多行 HUD 的行高 |

> 字号约束：一律显式 `FontSize::Px`，不得使用 `Rem`（见 §7.10.3）。UI 走原生分辨率后不再要求与画布像素格对齐；世界文字（`Text2d`）仍随画布放大。
> 具体 UI 布局（哪个面板放哪、有几行）属于对应玩法文档，本文只拥有尺寸常量与写法。

### 7.10.5 菜单界面（主菜单 / 关卡选择 / 设置）

所有者：`crates/game/src/render/menu.rs`。只由发布 bin 注册（`05` §5.3）。

| 项 | 约定 |
|---|---|
| 相机 | 与 HUD 相同：UI 由 **UI 窗口相机**（`IsDefaultUiCamera`）渲染，逻辑坐标系 = `VIEWPORT_W × VIEWPORT_H`（`UiScale = k`） |
| 背景 | 每个界面一个全屏不透明 `Node`（`left/top = 0`，`width/height = px(VIEWPORT_W/H)`），`GlobalZIndex(100)` 盖住 HUD 与暂停面板 |
| 生命周期 | 根节点带 `DespawnOnExit(对应 GameState)`；设置界面在 `GameSettings` 变化时整块重建 |
| 文本 | 中/英文案来自 `i18n`（默认中文）；字体来自 `UiFont`（`04` §9）。缺字体时 Bevy 只显示 ASCII |
| 尺寸 | 一律 `px(..)` 整数；**禁止** `vw()`/`vh()`/`percent()` |

| 常量 | 值 | 说明 |
|---|---|---|
| `MENU_FONT_PX` | `12.0` | 菜单正文/按钮字号 |
| `MENU_TITLE_FONT_PX` | `24.0` | 界面标题字号（2 × 正文） |
| `MENU_PADDING_PX` | `12.0` | 面板内边距；实现里取 2 × 作为左上边距 |
| `MENU_GAP_PX` | `6.0` | 行/按钮间距 |

> 这些常量的职责是「菜单类 UI 的排版网格」；具体有哪些按钮属于 `05` §5.5 的流程。
> 菜单同样参与「窗口 == 画布 × k」的逐像素验证（截图见 `AGENTS.md` §4）。

---

## 7.13 2D 视差背景

数据形状与校验见 `03` §3.10（`Level.background`，最多 3 层）。本节只定义运行时怎么画。

### 7.13.1 结构（每层一个 sprite）

| 项 | 取值 |
|---|---|
| 实体 | 每个视图每层一个 `Sprite` + `ParallaxLayer { view, index, period }` + `RenderLayers::layer(8 + view)` |
| 平铺 | `SpriteImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 }`【已验证】<https://docs.rs/bevy/0.20.0/bevy/sprite/enum.SpriteImageMode.html>；`stretch_value = 1.0` ⇒ 一个 texel = 一个世界像素 |
| 尺寸 | `custom_size = 视口世界尺寸 + 2 × 贴图尺寸`（保证任意相位都盖满视口） |
| Z | `z::PARALLAX + index`，`index` 越大越靠近镜头；见 `00` §0.4 |
| 采样 | 贴图用 `load_image` 同步解码，`ImageSampler::nearest`（`04` §3.2） |

### 7.13.2 相位公式（唯一实现：`game::render::parallax::parallax_placement`）

记 `c` = 相机中心（世界像素），`v` = 视口世界尺寸，`p` = 贴图尺寸，`s ∈ [0,1]` = 滚动百分比 / 100：

```
offset = (-c * s).rem_euclid(p)          // 贴图原点相对视口左上角的屏幕偏移
center = c + offset                     // sprite 中心（贴图原点在 sprite 左上角）
size   = v + 2 * p
```

- `s = 0`：`offset = 0` ⇒ 背景固定在屏幕上（无限远）。
- `s = 1`：`offset = (-c) mod p` ⇒ 贴图相对世界固定（与地形同速）。
- 中心取整到整像素：像素风不允许亚像素采样（`00` §0.4 / `06` §6.8）。

### 7.13.3 调度与相机

- 系统在 `PostUpdate`、`.before(TransformSystems::Propagate)` 执行；**读相机的 `Transform` 而不是 `GlobalTransform`**（后者在本帧尚未传播，会晚一帧、平移时产生拖影）。
- 视口世界尺寸 = `Camera::logical_viewport_size() * OrthographicProjection::scale`；相机中心 = `Transform::translation.truncate()`。
- 需要背景的相机插 `ParallaxCamera`：发布 bin 是每个内层 `InGameCamera`（带 `PlayerView`）；编辑器是 `EditorCamera`（没有 `PlayerView` ⇒ 视图 0）。
- 每个视图的背景用**自己的相机**算相位（`parallax.rs` 按 `PlayerView` 分组）；编辑器没有像素管线，`ViewLayout` 缺省为 1。

### 7.13.4 与房间背景板的关系

`z::ROOM_BG` 的房间背景板是**完全不透明**的整房间色块。关卡配置了 `background` 时：
**不生成房间背景板**（否则会整片盖住 `z=-100` 的视差层）。无背景时保持原行为，
截图里仍能靠色块区分房间与窗口黑边。

### 7.13.5 编辑器预览与外部改动

- 编辑器插入资源 `BackgroundOverride(doc.background)`（`08` §8.5.7 C），优先于 `LoadedLevel.background`，
  因此改图层/滚动百分比**无需保存、无需试玩**即可在画布看到。
- 贴图身份键 = `(路径, mtime)`：美术软件重画背景 PNG 后自动重新解码（`04` §5.6）。
- 滚动百分比变化只更新 `Transform`/`custom_size`，**不重新解码贴图**。

### 7.13.6 本地双人分屏

- `ViewLayout.views = 2` 时，每个背景图层生成两套 sprite：视图 0 在 layer 8、视图 1 在 layer 9。
- 内层相机 0 只渲染 `[0, 8]`，相机 1 只渲染 `[0, 9]` ⇒ 两屏各自使用自己的相机中心计算相位，互不串扰。
- 视图数量变化会进入贴图身份键 `(views, paths)`，从而销毁旧副本并按新布局重建。

---

## 7.11 验收检查表（渲染部分）

| # | 检查项 | 通过标准 |
|---|---|---|
| R1 | 像素完美缩放 | 把窗口从 640×640 拖到任意尺寸：画面始终是 `k × VIEWPORT` 的整数倍、居中、四周黑边；拖动过程中不出现半像素模糊 |
| R2 | 采样 | 放大到 `k ≥ 3`，瓦片/角色插画无过渡色、图集相邻单元无渗色 |
| R3 | MSAA | 两个相机都插了 `Msaa::Off`；sprite 半透明边缘不出现多级灰阶（用 `alpha = 255` 的对比色边缘目测） |
| R4 | 色调映射 | 两个相机都没有 `Hdr`；`Tonemapping` 未被显式改成 `None`；`09` 的 feature 里没有 `tonemapping_luts`；画面无粉色 |
| R5 | 色彩空间 | 内层离屏纹理格式为 `Bgra8UnormSrgb`；房间背景的实心色与美术工具里的取色一致（无系统性偏亮/偏暗） |
| R6 | 像素吸附 | 角色缓慢水平移动时，它的左边缘不会在「源宽度 ±1 texel」之间抖动；`grep` 检查 `crates/game/src` 中无第二处 `round`/`floor` 用于位置 |
| R7 | 吸附调度 | `GameSet::Snap` 位于 `PostUpdate` 且 `.before(TransformSystems::Propagate)`（`05` 的表已同步） |
| R8 | Z 卫生 | §7.7.4(a) 的 CI `grep` 无输出；dev 下同 Z 重叠检测器在整关卡跑满 60 s 无 `warn`。**M0 只做 (a)**：(b) 的检测器推迟到 M3 —— M0 只有两种 sprite（瓦片 / 房间背景板）且 Z 天然不同，检测器无检测面（有非平凡重叠面的是可拾取物与角色） |
| R9 | 瓦片规模 | 加载 30 房间关卡：瓦片实体 ≤ 7 680；同屏（同房间）≤ 256；房间裁剪生效（隐藏其它房间的 `RoomRoot` 后 `EntityCountDiagnosticsPlugin` 的实体数不变，但被 `ViewVisibility` 标记为可见的实体数下降到 ≤ 256） |
| R10 | 调试可视化 | `dev` 构建下 FPS 覆盖层可见、AABB/网格 `Gizmos` 可见；`release`（不带 `dev`）构建下两者都不存在（feature 门控生效） |
| R11 | UI 可点击 + 清晰 | UI 位于窗口相机（`RenderTarget` 与指针一致）⇒ 按钮可点击；文字按 `k` 倍原生分辨率光栅化，不做「低分辨率文本放大」；逻辑布局仍随 `k` 不变形 |
| R12 | Y 方向 | 世界坐标 `+Y` 向上（Bevy 原生），房间局部坐标 `+Y` 向下（`00` §0.3）——用网格 gizmo 目测一次即可确认没有多做一次翻转 |

---

## 7.12 与其它文档的接口

| 本文依赖 | 内容 |
|---|---|
| `00` §0.3 / §0.4 | `TILE_PX` / `ROOM_TILES` / `ROOM_PX`、坐标转换函数、房间中心公式、**Z 层数值表** |
| `00` §0.1 | **D5**（房间 = 16×16 瓦片；视野小于房间）、**D11**（60 Hz 固定步长）、**D13**（双层相机） |
| `02` | 相机跟随与地图边界、当前房间判定、房间无缝衔接的语义 |
| `03` §3.3 | legend → 瓦片语义、`Air` 不生成实体、`tileset.png` 索引语义 |
| `03` §3.10 | `Level.background` 的字段形状、层数上限与 V20 校验 |
| `04` §3.1 / §7.1 / §8 / §10 | 图集零 padding/零 margin、PNG 规格与 sRGB、字体资源、`tileset.png` 物理布局 |
| `05` §5.x | 模块划分（`render/pixel_camera.rs`、`render/snap.rs`、`render/tiles.rs`、`render/parallax.rs`）、`GameSet::Snap` |
| `06` §6.1 / §6.8 | `Aabb` 是逻辑真相、投影单向、取整用 `f32::round`、本地实体不做位置插值（客户端远端快照插值例外，§7.6.5） |
| `09` §9.2 / §9.4 / §9.5 | 依赖矩阵（瓦片/像素相机 crate 不可用）、feature 集合、`dev` feature |
| `10` §10.2.8 / §10.2.9 / §10.2.10 | `Sprite` / 相机的 0.20 精确字段与黑名单（C5/C6/C7 与本文章节一一对应） |
| 本文被依赖 | `Z` 常量名、`VIEWPORT_W/H`、`WINDOW_W/H`、`CanvasScale`、`PixelSnap`、像素吸附调度、瓦片渲染策略 |

**待同步项（需由 `05` 的所有者执行）**

1. `05` 的调度表中 `GameSet::Snap` 的排序必须从 `.after(TransformSystems::Propagate)` 改为 `.before(TransformSystems::Propagate)`（理由见 §7.6.3）。
2. `05` 的模块目录表需要补一行 `render/debug.rs`（调试绘制与 dev 覆盖层，见 §7.9）。
