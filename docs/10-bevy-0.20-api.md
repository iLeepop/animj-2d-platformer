# 10 — Bevy 0.20 API 速查 + 过期名称黑名单

> **本文是 API 事实的唯一所有者**（见 `00` §0.2）。任何关于 Bevy 0.20 API 的写法，以本文为准；本文没有的符号，**先查证再写码**。
> 本文不定义任何玩法/渲染数值常量（Z 层、分辨率、手感参数等归 `06`/`07`/`04` 所有），需要时引用「见 `07` §…」。
> 目标读者：AI coding agent + 人类开发者。**唯一用途：反幻觉。**

**版本基准**：Bevy `0.20.0`（2026-10-08 发布，<https://bevy.org/news/bevy-0-20>）。
**凡本文未列出的 Bevy API，一律视为「未知」，禁止凭模型内建记忆书写。** Bevy 0.14→0.20 之间发生了大量破坏性重命名，模型的默认知识与本版本**系统性不符**。

---

## 10.0 证据标记与使用纪律

### 标记定义（与 `00` §0.9 一致）

| 标记 | 含义 |
|---|---|
| **【已验证】** | 有 0.20.0 的 rustdoc 页面、0.20 官方 example 源码、或 bevy.org 迁移指南直接支撑，并跟 URL |
| **【UNVERIFIED】** | 无 0.20 直接证据。**不得据此写代码**，须先做最小验证 |
| **【INFERENCE】** | 由已验证事实推断，非文档直述 |

### 使用纪律（强制）

1. 写任何 `bevy::*` 符号前，**先在本文检索**。命中黑名单 → 用「0.20 正确写法」列。
2. 本文没收录的符号 → 打开 `https://docs.rs/bevy/0.20.0/bevy/…` 现查现用，并把结论回填本文（不要写进代码注释里）。
3. 命中【UNVERIFIED】→ 先写 20 行以内的最小验证程序跑通，再回填并升级标记。
4. 迁移指南里的代码块是**权威 Before/After**：`https://bevy.org/learn/migration-guides/<a>-to-<b>/`。

### 常用证据 URL 别名（下文表格中使用全 URL，此表仅供快速跳转）

| 别名 | URL |
|---|---|
| M020 | https://bevy.org/learn/migration-guides/0-19-to-0-20/ |
| M019 | https://bevy.org/learn/migration-guides/0-18-to-0-19/ |
| M018 | https://bevy.org/learn/migration-guides/0-17-to-0-18/ |
| M017 | https://bevy.org/learn/migration-guides/0-16-to-0-17/ |
| M016 | https://bevy.org/learn/migration-guides/0-15-to-0-16/ |
| M015 | https://bevy.org/learn/migration-guides/0-14-to-0-15/ |
| PRELUDE | https://docs.rs/bevy/0.20.0/bevy/prelude/index.html |

---

## 10.1 过期名称黑名单（本文最高价值部分，共 46 条：A 事件 10 + B 文本/UI 13 + C 渲染 15 + D ECS 8）

> 用法：左列是**模型极可能吐出的旧写法**（0.13–0.19 时代）。命中即按中列改写。
> 「后果」列写的是真实代价：`E` = 编译错误，`S` = 静默行为变化（最危险），`D` = 仅弃用警告。

### A. 事件 / observer（0.17 大拆分 + 0.20 细化）

| # | 旧写法（错误） | 0.20 正确写法 | 后果 / 备注 | 证据 URL |
|---|---|---|---|---|
| A1 | `#[derive(Event)]` + `EventWriter<E>` / `EventReader<E>` / `Events<E>`（当作 buffered event 用） | buffered（拉取式）用 `#[derive(Message)]` + `MessageWriter<M>` / `MessageReader<M>` / `Messages<M>`；写 `writer.write(m)`，读 `reader.read()` | **E**。0.20 中 `EventWriter`/`EventReader` 不存在（prelude 索引无此符号）；`Event` 一名现在专属 observer | https://bevy.org/learn/migration-guides/0-16-to-0-17/ ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.MessageWriter.html ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.MessageReader.html ；https://docs.rs/bevy/0.20.0/bevy/prelude/index.html |
| A2 | `app.add_event::<E>()` | `app.add_message::<M>()` | **E**。0.20 `App` 上只有 `add_message`（grep 全页无 `add_event`） | https://docs.rs/bevy/0.20.0/bevy/app/struct.App.html#method.add_message |
| A3 | `commands.send_event(x)` / `world.send_event(x)` / `events.send(x)` | `commands.write_message(x)` / `world.write_message(x)` / `messages.write(x)`（`send_default`→`write_default`，`send_batch`→`write_batch`，`SendBatchIds`→`WriteBatchIds`） | **E** | https://bevy.org/learn/migration-guides/0-16-to-0-17/ |
| A4 | observer 参数 `Trigger<E>`（`.target()`） | `On<E>`（直接字段 `e.entity`；原 `.target()`） | **E**。`Trigger` 是 0.16 及更早的名字 | https://bevy.org/learn/migration-guides/0-16-to-0-17/#observer-event-api-changes |
| A5 | 生命周期 observer 写 `OnAdd` / `OnInsert` / `OnReplace` / `OnRemove` / `OnDespawn` | `Add` / `Insert` / `Replace` / `Remove` / `Despawn` | **E** | https://bevy.org/learn/migration-guides/0-16-to-0-17/#observer-event-api-changes |
| A6 | `On<Add, A>`（0.19 写法，`B: Bundle` 挂在 observer 上） | `On<Add<A>>`（`B` 泛型移入事件类型本身；动态组件用 `On<Add<()>>`） | **E**。`IntoObserverSystem` 的泛型个数也变了 | https://bevy.org/learn/migration-guides/0-19-to-0-20/#moved-observer-events-b-bundle-generic-into-the-event-type |
| A7 | `world.trigger_targets(E, entity)` / `commands.trigger_targets(...)` | `#[derive(EntityEvent)] struct E { entity: Entity }` + `world.trigger(E { entity })`（多个目标逐个 trigger） | **E**。`trigger_targets` 已从 API 中移除 | https://bevy.org/learn/migration-guides/0-16-to-0-17/#observer-event-api-changes |
| A8 | `On<Pointer<Press>>` / `On<Pointer<Over>>` / `On<Pointer<Click>>` / `On<Pointer<Drag>>` … | `On<PointerPress>` / `On<PointerOver>` / `On<PointerClick>` / `On<PointerDrag>` …（不再 `Deref`，字段直接挂在事件上） | **E**。0.20 指针事件「扁平化」 | https://bevy.org/learn/migration-guides/0-19-to-0-20/#flat-pointer-events |
| A9 | `press.pointer_id` / `press.pointer_location.position` | `press.pointer.id` / `press.pointer.position`（后者是 `Vec2`，不再是 `Location`；要 `Location` 用 `pointer.location()`） | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#flat-pointer-events |
| A10 | 泛型指针 observer：`fn f<E: Debug + Clone + Reflect>(e: On<Pointer<E>>)` | `fn f<E: PointerEvent>(e: On<E>)` | **E**。新 trait `PointerEvent` | https://bevy.org/learn/migration-guides/0-19-to-0-20/#flat-pointer-events |

### B. 文本 / UI

| # | 旧写法（错误） | 0.20 正确写法 | 后果 / 备注 | 证据 URL |
|---|---|---|---|---|
| B1 | `JustifyText::Center` / `JustifyText::Left` | `Justify::Center` / `Justify::Left` | **E**。`enum.JustifyText.html` 在 0.20 返回 HTTP 404 | https://bevy.org/learn/migration-guides/0-16-to-0-17/ ；https://docs.rs/bevy/0.20.0/bevy/prelude/enum.JustifyText.html（404） |
| B2 | `TextLayout::new_with_justify(...)` / `new_with_linebreak` / `new_with_no_wrap` | `TextLayout::justify(...)` / `TextLayout::linebreak(...)` / `TextLayout::no_wrap(...)` | **E** | https://bevy.org/learn/migration-guides/0-18-to-0-19/ |
| B3 | `TextFont { font: handle, font_size: 30.0 }` | `TextFont { font: handle.into(), font_size: FontSize::Px(30.0) }`（`font` 类型是 `FontSource`，`font_size` 类型是 `FontSize`） | **E** | https://bevy.org/learn/migration-guides/0-18-to-0-19/ ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextFont.html |
| B4 | `font: FontSource::SansSerif` / `::Monospace` / `::Serif` | `FontSource::sans_serif()` / `monospace()` / `serif()`（泛型族改由 `GenericFontFamily` 承载） | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#fontsource-generic-family-variants |
| B5 | 依赖 `TextFont::default()` = 20 逻辑像素 | 默认改为 `FontSize::Rem(1.)`（默认 `RemSize` = 20 逻辑像素时渲染一致，但会随 `RemSize` 变化）。**像素风需要固定字号时必须显式写 `font_size`** | **S** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#textfont-default-font-size-is-now-fontsize-rem-1 |
| B6 | `BorderRadius { top_left: px(10.) }`（字段是 `Val`） | `BorderRadius { top_left: CornerRadius::circular(px(10.)) }` 或 `px(10.).into()`（椭圆角 = 其中一个轴设 `Val::Auto`） | **E**。`BorderRadius::resolve_single_corner` 移除，改 `CornerRadius::resolve`；构造/更新函数不再是 `const` | https://bevy.org/learn/migration-guides/0-19-to-0-20/#borderradius-and-resolvedborderradius-fields-are-now-2d |
| B7 | `commands.spawn((Node{..}, BorderRadius::all(px(4.))))` 把 `BorderRadius` 当独立组件 | `Node { border_radius: BorderRadius::all(px(4.)), ..default() }` | **E**。0.18 起 `BorderRadius` 不再是组件 | https://bevy.org/learn/migration-guides/0-17-to-0-18/ |
| B8 | `val.resolve(size)` / `val2.resolve(size)` / `border_radius.resolve(...)` 两参写法 | 追加 `em_size: EmSize` 与 `rem_size: RemSize` 两个参数；已有节点上可从 `ComputedNode.em_size`/`rem_size` 取 | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/ |
| B9 | `UiImage::new(handle)` / `UiImageSize` | `ImageNode::new(handle)` / `ImageNodeSize`（图集版 `ImageNode::from_atlas_image(handle, TextureAtlas::from(layout_handle))`） | **E**。`struct.UiImage.html` 在 0.20 返回 HTTP 404 | https://bevy.org/learn/migration-guides/0-15-to-0-16/ ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.UiImage.html（404） ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.ImageNode.html |
| B10 | `ImageScaleMode::Sliced(slicer)` 作为组件 | `image_mode` 字段：`Sprite { image_mode: SpriteImageMode::Sliced(slicer), .. }` / `ImageNode::new(h).with_mode(NodeImageMode::Sliced(slicer))` | **E**。`ImageScaleMode` 已删除 | https://bevy.org/learn/migration-guides/0-15-to-0-16/ |
| B11 | `bevy::ui::widgets::Button` + `bevy::ui::Interaction` | `bevy::ui_widgets::Button`；交互改由 `bevy_picking` 的 `Hovered` + `bevy::ui::Pressed` 组件表达 | **D**（已弃用，但新代码不得用旧写法）。编辑器/菜单按钮全受影响 | https://bevy.org/learn/migration-guides/0-19-to-0-20/#ui-widgets-button-and-ui-interaction-are-deprecated |
| B12 | 只用 `EditableText` 就得到一个文本输入框 | 需**同时**插入 `EditableText` + `TextInput` | **S**（功能不完整，无编译错误） | https://bevy.org/learn/migration-guides/0-19-to-0-20/ |
| B13 | 把 `CalculatedClip` 当结构体 / 读 `.clip` 矩形 | `CalculatedClip` 现为 enum：`Rects(Vec<(Rect, Affine2)>)` / `FullyClipped`；判定点用 `CalculatedClip::contains_point` | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/ |

### C. 渲染 / sprite / 资源

| # | 旧写法（错误） | 0.20 正确写法 | 后果 / 备注 | 证据 URL |
|---|---|---|---|---|
| C1 | `Camera2dBundle::default()` / `SpriteBundle { .. }` | `Camera2d` / `Sprite`（都是组件，直接 spawn 元组） | **E**。`struct.Camera2dBundle.html` 在 0.20 返回 HTTP 404 | https://bevy.org/learn/migration-guides/0-14-to-0-15/ ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Camera2dBundle.html（404） |
| C2 | 把 `Handle<Image>` 和 `TextureAtlas` 作为独立组件塞给 sprite 实体 | 放进 `Sprite { image: .., texture_atlas: Some(TextureAtlas { layout, index }), ..default() }` 字段 | **S**（编译可能过，但 sprite 不渲染/不受控） | https://bevy.org/learn/migration-guides/0-14-to-0-15/ |
| C3 | 用 `Sprite { image, ..default() }` 之外手写全部字段 | 0.20 新增 `alpha_mode: SpriteAlphaMode`（默认 `Blend`，可 `Opaque` / `Mask(f32)`）⇒ 手写全字段必编译失败，**始终加 `..default()`** | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#migrate-sprite-to-use-mesh2d-spritematerial |
| C4 | 假设「同 Z 的 sprite 按 spawn 顺序绘制」 | **无保证**（0.20 `Sprite` 迁到 Mesh2d/Material2d 后端，排序可能变）。任何可能重叠的 sprite 必须显式分级 Z（Z 层表见 `00` §0.4 / `07`） | **S** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#migrate-sprite-to-use-mesh2d-spritematerial |
| C5 | 依赖「默认不开抗锯齿」，或把 `Msaa` 当全局资源设置 | `Msaa` 是**相机组件**，且每个 `Camera` 都被注册为 required `Msaa::Sample4` ⇒ **像素风必须显式插 `Msaa::Off`** | **S**（画面发糊/半透明边缘发虚） | https://bevy.org/learn/migration-guides/0-14-to-0-15/ ；https://docs.rs/bevy/0.20.0/bevy/render/view/enum.Msaa.html ；https://docs.rs/bevy/0.20.0/bevy/prelude/index.html |
| C6 | `Camera { target: RenderTarget::Image(h), .. }` | `RenderTarget::Image(h.into())` 作为**独立组件**与 `Camera` 并列插入（`Camera` 上已无 `target` 字段） | **E** | https://bevy.org/learn/migration-guides/0-17-to-0-18/#rendertarget-is-now-a-component ；https://docs.rs/bevy/0.20.0/bevy/camera/enum.RenderTarget.html |
| C7 | `ScalingMode::FixedVertical(270.0)`（元组变体） | `ScalingMode::FixedVertical { viewport_height: 270.0 }`（具名字段变体） | **E** | https://docs.rs/bevy/0.20.0/bevy/camera/enum.ScalingMode.html ；https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html |
| C8 | 用 `Tonemapping::None` 来表达「关掉色调曲线」 | 用 `Tonemapping::Linear`。0.20 的 `None` 语义变为**完全直通**：`ColorGrading`/`DebandDither` 失效、负通道不再 clamp | **S**。另：`Camera2d` 默认已是 `Linear`，2D 无需改动；**不要**给 `Camera2d` 加 `Hdr` | https://bevy.org/learn/migration-guides/0-19-to-0-20/#tonemapping-none-is-now-a-full-passthrough |
| C9 | 反射/场景文件/BRP 里写 `bevy_core_pipeline::tonemapping::Tonemapping`（`DebandDither` 同理） | 反射路径变为 `bevy_render::view::Tonemapping` / `bevy_render::view::DebandDither`（Rust `use` 经 re-export 仍可用） | **S**（场景文件反序列化失败） | https://bevy.org/learn/migration-guides/0-19-to-0-20/#tonemapping-and-debanddither-moved-to-bevy-render-view |
| C10 | `bevy_sprite_render::SpriteMaterial` / `SpriteMaterialPlugin` | `SpriteMeshMaterial` / `SpriteMeshMaterialPlugin`（自定义 sprite 材质用 `MaterialExtension2d` / `ExtendedMaterial2d`） | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#spritematerial-and-spritematerialplugin-rename |
| C11 | 写 `#import bevy_pbr::…` 的 `.wgsl` 自定义着色器 | 0.20 全面改用 **WESL**：文件后缀 `.wesl`，`import bevy_pbr::…;`；`#define_import_path` 取消（`embedded://<crate>/<file>.wesl` 即 `crate::file`）；`shader_format_wesl` feature 移除，**GLSL 支持已删除** | **E/S**。纯 WGSL（无预处理指令）仍可用 | https://bevy.org/learn/migration-guides/0-19-to-0-20/#wesl-shaders |
| C12 | `use bevy_math::primitives::*` / `bevy_math::{Primitive2d, Bounded2d, …}` | 几何图元与相关 trait 拆到 **`bevy_shape`**（`bevy_shape::*` / `bevy_shape::prelude::*`）。用 `bevy::prelude::*` 时无需改 | **E**（`default-features = false` 时需显式加 `bevy/bevy_shape` 对应 feature） | https://bevy.org/learn/migration-guides/0-19-to-0-20/#bevy-shape-split-out-from-bevy-math |
| C13 | `use bevy_math::curve::*` / `bevy_math::cubic_splines::*` / `bevy_math::Curve` | 曲线拆到 **`bevy_curve`**：`bevy_curve::*`、`bevy_curve::cubic_splines::*`、`bevy_curve::Curve`；`bevy_math` 的 `curve` feature 已删除 | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#bevy-curve-split-out-from-bevy-math |
| C14 | `bevy::scene::SceneSpawner` / `DynamicScene` 序列化当「场景系统」 | 旧场景系统在 0.19 改名 **`bevy_world_serialization`**；`bevy::scene` 现在是 BSN（`bsn!` / `bsn_list!` / `spawn_scene`） | **S** | https://bevy.org/learn/migration-guides/0-18-to-0-19/ |
| C15 | 用 `bevy_animation::AnimationPlayer` 播 sprite/图集帧动画 | **0.20 无 sprite 帧动画能力**，必须自写系统（官方 `sprite_sheet.rs` 也是自写 `AnimationIndices` + `AnimationTimer` 改 `Sprite.texture_atlas.index`） | **E/S**。`bevy_animation` 只处理 `AnimationGraph` 驱动的属性动画 | https://docs.rs/bevy/0.20.0/bevy/animation/index.html ；https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html ；https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html |

### D. ECS / 调度 / 其它

| # | 旧写法（错误） | 0.20 正确写法 | 后果 / 备注 | 证据 URL |
|---|---|---|---|---|
| D1 | `#[derive(Component, Resource)] struct X;`（同一类型既当资源又当组件） | 拆成两个类型：`#[derive(Component)] struct XComp` + `#[derive(Resource)] struct XRes` | **E**。0.19 起 `Resource: Component`，双 derive 不可行；插入同名组件会 despawn 其它实体上的同类型副本 | https://bevy.org/learn/migration-guides/0-18-to-0-19/#resources-as-components |
| D2 | `next_state.set_if_neq(s)` / `NextState::PendingIfNeq` / `commands.set_state_if_neq(s)` | `set_if_different(s)` / `PendingIfDifferent` / `set_state_if_different(s)` | **D**（旧名有弃用包装，但新代码必须用新名） | https://bevy.org/learn/migration-guides/0-19-to-0-20/#nextstate-set-if-neq-renamed-to-set-if-different |
| D3 | `for x in q.iter_many(list)` 直接拿到 item | 现 yield `Result<Item, QueryEntityError>`：旧行为写 `.matched()`，或 `.flat_map(Result::ok)`。涉及 `iter_many` / `iter_many_mut` / `iter_many_unique*` / `par_iter_many*` | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/#query-iterators-from-many-entities-now-iterate-over-result-instead-of-querydata-item |
| D4 | `trait Condition` / 自己 impl `Condition` | 改名 `SystemCondition` | **E** | https://bevy.org/learn/migration-guides/0-16-to-0-17/ |
| D5 | `world.iter_entities()` / `iter_entities_mut()` | 已弃用：`world.query::<EntityMut>().iter(&world)` / `world.query::<EntityRef>().iter(&mut world)`（注意默认查询过滤器会漏掉 `Disabled` 实体，必要时加 `Allows<T>`） | **D** | https://bevy.org/learn/migration-guides/0-16-to-0-17/ |
| D6 | `Handle::Weak` / `weak_handle!(...)` / `Handle::weak_from_u128` | `Handle::Uuid` / `uuid_handle!("<uuid-v4>")`；`clone_weak()` → 通常直接 `clone()` | **E** | https://bevy.org/learn/migration-guides/0-16-to-0-17/ ；https://bevy.org/learn/migration-guides/0-15-to-0-16/ |
| D7 | `assets.insert(&h, x);`（忽略返回值） | 0.16 起返回 `Result`（handle 已 drop 时为错误），按需 `?` 或 `.unwrap()` | **E** | https://bevy.org/learn/migration-guides/0-16-to-0-17/ |
| D8 | `Name::from(some_runtime_string)` | `Name` 只实现 `From<&'static str>`；非静态字符串用 `Name::new(s.to_owned())` | **E** | https://bevy.org/learn/migration-guides/0-19-to-0-20/ |

---

## 10.2 API 速查（本项目会用到的部分）

> 每组给「最小可编译片段 + 证据」。片段里的常量一律引用其它文档所有者，**不在本文件定义数值**。

### 10.2.1 App / Plugin / DefaultPlugins / feature profile

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(my_plugin)                 // 任何 `fn(&mut App)` 也是 Plugin
        .add_systems(Update, hello_world_system)
        .run();
}

fn hello_world_system() { info!("hello"); }

// trait 形式：required method 只有 build
struct MyPlugin;
impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) { app.add_systems(Update, hello_world_system); }
}
```

- `Plugin` 的 required method 是 `fn build(&self, app: &mut App)`；另有 `ready(&self, &App) -> bool`、`finish`、`cleanup`（`&mut App`）、`name()`、`is_unique()`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/app/trait.Plugin.html>
- 0.20 引入 **Profiles**：`default`(= 2d + 3d + ui + audio)、`2d`、`3d`、`ui`。关 3D 的官方写法是 `bevy = { version = "0.20", default-features = false, features = ["2d"] }`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/index.html>
- **【UNVERIFIED】** `2d` profile 是否已含 `bevy_ui`/`bevy_text`：官方只给描述，未列展开集合。依赖 feature 的最终结论归 `09`。

### 10.2.2 Component / `#[require]` / Bundle

```rust
#[derive(Component)]
#[require(
    Velocity,                  // Default::default()
    Health(100),               // 元组结构体构造
    Marker::Active,            // 枚举变体 / 单位结构体
    Scale = Vec2::ONE,         // 任意表达式
)]
struct Player;

#[derive(Component)]
#[component(immutable)]
struct Shield;

#[derive(Component)]
#[component(storage = "SparseSet")]
struct RareTag;
```

- `#[derive(Component)]` 可用属性：`component`、`require`、`relationship`、`relationship_target`、`entities`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/ecs/component/derive.Component.html>
- **Bundle 未废弃**：每个 `Component` 自动实现 `Bundle`；元组（≤15）也是 `Bundle`；`()` 是空 Bundle ⇒ `commands.spawn((Sprite::from_image(h), Transform::default()))` 合法。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/trait.Bundle.html>
- **【UNVERIFIED】** `#[derive(Bundle)]` 在 0.20 是否仍存在/推荐：未在 0.20 文档中找到独立页面。**写代码时不要用它**，直接用元组 + `#[require]`。

### 10.2.3 层级：`ChildOf` / `Children`

```rust
// 1) 直接插 ChildOf（0.20 example 的实际写法）
commands.spawn(ChildOf(parent_entity));

// 2) 父实体侧批量生成子实体
commands.spawn_empty().with_children(|p| {
    p.spawn(Sprite::from_image(handle.clone()));
});

// 3) children! 宏（prelude 导出）
commands.spawn((Node::default(), children![
    (Text::new("a"),),
    (Text::new("b"),),
]));
```

- `pub struct ChildOf(pub Entity)`（`ChildOf::parent()`），是 `Relationship` 组件，配对 `Children`（`RelationshipTarget`）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/ecs/hierarchy/struct.ChildOf.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html>
- **`Parent` 已不存在**（0.15 起改 `ChildOf`）。本项目任何父子关系一律用 `ChildOf`/`Children`。

### 10.2.4 States

```rust
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
enum AppState { #[default] Menu, Playing }

fn build(app: &mut App) {
    app.init_state::<AppState>()               // 需要 StatesPlugin（含于 DefaultPlugins）
       .add_systems(OnEnter(AppState::Playing), setup_level)
       .add_systems(OnExit(AppState::Playing), teardown_level)
       .add_systems(
           OnTransition { from: AppState::Menu, to: AppState::Playing }, fade_in,
       )
       .add_systems(Update, tick.run_if(in_state(AppState::Playing)));
}

fn go(mut next: ResMut<NextState<AppState>>) { next.set_if_different(AppState::Playing); }
```

- `AppExtStates`（prelude）：`init_state` / `insert_state` / `add_sub_state` / `add_computed_state`；当前状态 = `State<S>`，队列 = `NextState<S>`（方法 `set`、`set_if_different`）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/state/app/trait.AppExtStates.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.NextState.html>
- schedule 标签 `OnEnter<S>` / `OnExit<S>` / `OnTransition<S>`（`from`/`to` 字段）；run condition `in_state` / `state_changed`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.OnEnter.html>
- 便捷标记组件 `DespawnOnEnter<S>` / `DespawnOnExit<S>`（在 prelude）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html>
- `SubStates` 的 derive 语法：`#[source(AppState = AppState::Playing)]`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/trait.SubStates.html>

### 10.2.5 Message（buffered / 拉取式）——本项目「房间达成」「拾取」等一律用这个

```rust
#[derive(Message)]
struct RoomCleared { col: u32, row: u32 }

fn emit(mut w: MessageWriter<RoomCleared>) { w.write(RoomCleared { col: 1, row: 0 }); }

fn consume(mut r: MessageReader<RoomCleared>) {
    for m in r.read() { info!("cleared {} {}", m.col, m.row); }
}

// 注册（缺了就收不到消息）
fn build(app: &mut App) { app.add_message::<RoomCleared>(); }

// Commands / World 侧
fn quit(mut commands: Commands) { commands.write_message(AppExit::Success); }
```

- `MessageWriter::write(m) -> MessageId<M>`；`MessageReader::read()` 迭代未读消息；`PopulatedMessageReader<T>` 在无消息时跳过系统；同类型既要读又要写用 `MessageMutator<T>`（`MessageReader` + `MessageWriter` 同类型会参数冲突）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.MessageWriter.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.MessageReader.html>
- `App::add_message::<M>()`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/app/struct.App.html#method.add_message>
- **命名约定**：buffered 消息用过去式（见 `00` §0.5）。
- 0.20 中 `AppExit`、`AssetEvent<T>`、`StateTransitionEvent<S>`、`WindowResized` 都是 **Message**，用 `MessageReader<...>` 读。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.MessageReader.html>
- **【UNVERIFIED】** `MessageWriter::send` 是否还留有弃用别名：`MessageWriter` 页面公开方法只有 `write`（官方 example 里出现过 `send` 字样的注释）。**统一写 `write`**。

### 10.2.6 Event / observer（push 式）——瞬时、需要冒泡/定向时用这个

```rust
// 全局事件
#[derive(Event)]
struct LevelRestart;

fn build(app: &mut App) {
    app.add_observer(|_e: On<LevelRestart>| info!("restart"));
}
fn fire(mut commands: Commands) { commands.trigger(LevelRestart); }

// 定向到实体的 —— 必须 EntityEvent
#[derive(EntityEvent)]
struct CollectReward { entity: Entity, reward: u32 }

fn build2(app: &mut App) {
    app.add_observer(|e: On<CollectReward>| {
        let _target = e.entity;      // 目标实体字段
        let _hint = e.event().reward; // 事件负载
    });
}
// 实体级 observer：
// commands.spawn(Player).observe(|e: On<CollectReward>| { /* ... */ });

// 沿 ChildOf 向上冒泡（0.20 写法）
#[derive(EntityEvent)]
#[entity_event(propagate)]
struct Poke { entity: Entity }
```

- observer 参数就是 `On<E>`（`E: EventPattern`）：`event()` / `event_mut()` / `trigger()` / `event_key()`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.On.html>
- `EntityEvent: Event`，trait 方法 `event_target(&self) -> Entity`；`#[entity_event(propagate)]` / `#[entity_event(propagate, auto_propagate)]` 挂在 **EntityEvent** 派生上（普通 `Event` 派生没有）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/ecs/event/trait.EntityEvent.html>
- 触发入口：`World::trigger`、`Commands::trigger`（`trigger_ref` / `trigger_dynamic*` 用于动态场景）；**`trigger_targets` 不存在**。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Commands.html> ；<https://bevy.org/learn/migration-guides/0-16-to-0-17/#observer-event-api-changes>
- **命名约定**：observer 事件用命令式/名词化动作（见 `00` §0.5）。
- 选择规则（本项目）：**需要「帧内持续可读 / 跨系统拉取 / 可回溯」→ Message；需要「立刻响应、带实体目标、可冒泡」→ Event(observer)**。

### 10.2.7 调度与 `SystemSet`（含固定步长）

```rust
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct PhysicsSet;

fn build(app: &mut App) {
    // 固定步长 60 Hz 由 `00` D11 锁定，数值只在 `06` 定义
    app.insert_resource(Time::<Fixed>::from_duration(core::time::Duration::from_secs_f64(1.0 / crate::config::physics::TICK_HZ)))
       .add_systems(PreUpdate, read_input)
       .add_systems(FixedUpdate, (apply_gravity, resolve_solid_collisions, integrate).chain().in_set(PhysicsSet))
       .add_systems(Update, animate.after(PhysicsSet));
    app.configure_sets(FixedUpdate, PhysicsSet.after(read_input));
}
```

- 内建 schedule：`First` / `PreUpdate` / `Update` / `PostUpdate` / `Last` / `FixedPreUpdate` / `FixedUpdate` / `FixedPostUpdate` / `Startup` 等。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/ecs/schedule/index.html>
- 排序/分组方法属于 trait `IntoScheduleConfigs`（prelude）：`chain` / `chain_weak` / `after` / `before` / `after_weak` / `before_weak` / `in_set` / `run_if`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/trait.IntoScheduleConfigs.html>
- `App::configure_sets(schedule, sets)` 确实存在。【已验证】<https://docs.rs/bevy/0.20.0/bevy/app/struct.App.html#method.configure_sets>
- `Time<Fixed>`：`Time::<Fixed>::from_duration(Duration)`；在 `FixedUpdate` 内 `Res<Time>` 即 `Time<Fixed>`。物理步长 **60 Hz** 由 `00` D11 锁定，数值只在 `06` 定义。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Time.html>

### 10.2.8 输入与时间

```rust
fn input_system(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
) {
    if keys.pressed(KeyCode::ArrowLeft) { /* 持续 */ }
    if keys.just_pressed(KeyCode::Space) { /* 本帧按下 */ }
    for pad in &pads {
        if pad.just_pressed(GamepadButton::South) { /* ... */ }
        if let Some(x) = pad.get(GamepadAxis::LeftStickX) { let _ = x; }
        let _: Vec2 = pad.left_stick();
    }
    let _ = time.delta_secs();
}
```

- `ButtonInput<T>`：`pressed` / `just_pressed` / `just_released` / `any_pressed` / `get_pressed` / `clear_just_pressed`。`ButtonInput<KeyCode>` 与窗口焦点绑定。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.ButtonInput.html>
- `Gamepad` 是**组件**（`Query<&Gamepad>`），方法 `get` / `left_stick` / `right_stick` / `dpad` / `pressed` / `just_pressed` / `all_pressed` 等。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Gamepad.html>
- 本项目输入收敛策略见 `00` D16（直接用 `ButtonInput<KeyCode>`，在 `input` 模块内转成 `PlayerInput` 动作结构）。
- **无窗口测试输入链路**（本项目已在用）：`KeyboardInput` 的字段是 `{ key_code: KeyCode, logical_key: Key, state: ButtonState, text: Option<SmolStr>, repeat: bool, window: Entity }`，并实现 **`Message`** ⇒ 可直接 `world.write_message(...)` 注入；`InputPlugin`（unit struct）负责把它写进 `ButtonInput<KeyCode>`（`PreUpdate`）；`MinimalPlugins` 可搭无头 App（无需窗口/GPU）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/input/keyboard/struct.KeyboardInput.html>（字段表 + `impl Message for KeyboardInput`）、<https://docs.rs/bevy/0.20.0/bevy/input/struct.InputPlugin.html> —— 用法见 `crates/game/src/render/debug.rs` 的 `keyboard_message_reaches_the_toggle`
- 时间族：`Time`（默认时钟）/ `Time<Real>` / `Time<Virtual>` / `Time<Fixed>`；`ResMut<Time<Virtual>>` 上有 `pause()` / `unpause()`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Time.html>

### 10.2.9 相机 / `Sprite` / 图集 / 离屏渲染目标

```rust
use bevy::camera::{ScalingMode, RenderTarget};   // 这两个不在 prelude
use bevy::sprite::Anchor;                        // Anchor 也不在 prelude

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
) {
    // 内层相机：渲染到离屏纹理（像素完美视口；尺寸常量见 `07`）
    let target = images.add(Image::new_target_texture(
        crate::config::render::VIEWPORT_W,
        crate::config::render::VIEWPORT_H,
        TextureFormat::Bgra8UnormSrgb,
        None,
    ));
    commands.spawn((
        Camera2d,
        Camera { order: -1, ..default() },
        RenderTarget::Image(target.clone().into()),
        Msaa::Off,
    ));

    // 外层相机：默认 WindowSize 投影，scale 由 fit 系统按整数倍写入
    commands.spawn((Camera2d, Msaa::Off));

    // 图集 sprite（严格网格、无 padding/margin，规格见 `04`）
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(crate::config::TILE_PX as u32), 4, 1, None, None);
    let layout_handle = layouts.add(layout);
    commands.spawn((
        Sprite::from_atlas_image(
            asset_server.load("sprites/player/cat_sheet.png"),
            TextureAtlas { layout: layout_handle, index: 0 },
        ),
        Anchor::BOTTOM_CENTER,
        Transform::from_xyz(0.0, 0.0, crate::config::render::Z_PLAYER),
    ));
}
```

- `Camera2d` 是 unit-struct 组件；**`Camera2dBundle` 不存在**。`OrthographicProjection::default_2d()` / `ScalingMode` 变体见 <https://docs.rs/bevy/0.20.0/bevy/camera/enum.ScalingMode.html>。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Camera2d.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.OrthographicProjection.html>
- `TextureAtlasLayout::from_grid(tile_size: UVec2, columns: u32, rows: u32, padding: Option<UVec2>, offset: Option<UVec2>)`，索引**从左到右、从上到下**；`TextureAtlasLayout` / `TextureAtlas` 都在 prelude。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html>
- `Sprite::from_image` / `from_atlas_image` / `from_color` / `sized`；字段含 `image` / `texture_atlas` / `custom_size` / `image_mode` / **`alpha_mode`**。用结构体字面量**必须**带 `..default()`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Sprite.html> ；<https://bevy.org/learn/migration-guides/0-19-to-0-20/#migrate-sprite-to-use-mesh2d-spritematerial>
- 平铺 sprite：`Sprite { image, custom_size: Some(size), image_mode: SpriteImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 }, ..default() }`（`SpriteImageMode` 在 `bevy::sprite`）。UV 在 shader 内取模，**不要求** sampler 为 `Repeat`；`stretch_value = 1.0` 表示一个 texel 对应一个世界像素。【已验证】（源码核对 `bevy_sprite-0.20.0/src/sprite.rs` 的 `SpriteImageMode::Tiled` 与 `sprite_mesh/functions.wesl::apply_tiling`）<https://docs.rs/bevy/0.20.0/bevy/sprite/enum.SpriteImageMode.html>
- **`Anchor` 不在 prelude**：`use bevy::sprite::Anchor;`（`Anchor(pub Vec2)`，常量 `TOP_LEFT…BOTTOM_RIGHT`）。`bevy/prelude/struct.Anchor.html` 在 0.20 返回 404。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html>
- **`ScalingMode` / `RenderTarget` 不在 prelude**（分别在 `bevy::camera::ScalingMode`、`bevy::camera::RenderTarget`）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html> ；<https://docs.rs/bevy/0.20.0/bevy/camera/enum.RenderTarget.html>
- 2D 透明排序依据**视图空间 Z**（`Transform.translation.z`），且是**不稳定排序** ⇒ 重叠 sprite 必须分级 Z，Z 取值表由 `00` §0.4 / `07` 所有。`ZIndex` / `GlobalZIndex` 是 `bevy_ui` 专用，对 `Sprite` 无效。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/core_pipeline/core_2d/struct.Transparent2d.html> ；<https://docs.rs/bevy/0.20.0/bevy/ui/struct.ZIndex.html>
- 相机前后顺序 = `Camera { order: isize, .. }`（高 order 后渲染、覆盖在上）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Camera.html>
- 鼠标↔世界互转：`camera.viewport_to_world_2d(&GlobalTransform, Vec2) -> Result<Vec2, ViewportConversionError>`（官方要求系统在 `PostUpdate` 且位于 `TransformSystems::Propagate` 之后运行）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/camera/struct.Camera.html> ；<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>
- 像素完美管线的完整方案（内层离屏 + 外层整数倍 `scale` + 黑边）见 `07`；本文只保证 API 名称正确。全局最近邻采样：`DefaultPlugins.set(ImagePlugin::default_nearest())`（`ImagePlugin` 在 prelude）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/image/struct.ImagePlugin.html>

### 10.2.10 UI / `Text` / `Text2d`

```rust
use bevy::prelude::*;

// 屏幕空间 UI 文本
commands.spawn((
    Text::new("HP: 3"),
    TextFont { font: FontSource::sans_serif(), font_size: FontSize::Px(20.0), ..default() },
    TextColor(Color::WHITE),
    TextLayout::justify(Justify::Center),
    Node { position_type: PositionType::Absolute, top: px(12.), left: px(12.), ..default() },
));

// UI 节点 + 图片
commands.spawn((
    Node { width: px(64.), height: px(64.), border: px(4.).all(), ..default() },
    BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
    ImageNode::new(asset_server.load("ui/icon.png")),
));

// 世界空间文字（0.20 里 Text2d 在 prelude）
commands.spawn((
    Text2d::new("Hello"),
    TextFont { font_size: FontSize::Px(24.0), ..default() },
    TextColor(Color::srgb(1.0, 1.0, 0.0)),
    TextLayout::justify(Justify::Center),
    Transform::from_xyz(0.0, 32.0, crate::config::render::Z_TEXT2D),
));
```

- 关键类型：`Node`（含 `border_radius` 字段）、`Text(String)`、`Text2d(String)`、`TextFont`、`TextColor`、`TextLayout`、`TextSpan`、`TextBackgroundColor`、`ImageNode`、`BackgroundColor`、`BorderColor`、`Outline`、`ImagePlugin`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/index.html>
- `TextFont` 字段：`font: FontSource`、`font_size: FontSize`、`weight`、`width`、`style`、`font_smoothing`、`font_features`、`font_variations`；构造 `TextFont::from_font_size(impl Into<FontSize>)`。**默认 `font_size` = `FontSize::Rem(1.)`**。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextFont.html> ；<https://bevy.org/learn/migration-guides/0-19-to-0-20/#textfont-default-font-size-is-now-fontsize-rem-1>
- 对齐枚举是 **`Justify`**（`TextLayout { justify, linebreak }`；`TextLayout::justify(Justify::Center)`）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextLayout.html>
- 长度单位：`px()` / `percent()` / `vw()` / `vh()`，0.20 新增 `em()` / `rem()`；`UiRect::all(px(10))`。【已验证】<https://bevy.org/learn/migration-guides/0-19-to-0-20/>
- UI 里不用 Z，用 `bevy_ui` 的 `ZIndex` / `GlobalZIndex`（见 `00` §0.4）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/ui/struct.ZIndex.html>

### 10.2.11 Asset / `AssetLoader`（本项目要解析 `.level.ron`）

```rust
use bevy::asset::io::Reader;          // 调 read_to_end 需要该 trait 在作用域
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;

/// 资产 = level_format 解析结果（level_format 不依赖 bevy，见 `00` §0.6）
#[derive(Asset, TypePath)]
struct LevelAsset(level_format::Level);

#[derive(Default, Serialize, Deserialize)]     // Settings: Settings + Default + Serialize + Deserialize
struct LevelLoaderSettings {}

#[derive(Default, TypePath)]                    // AssetLoader: TypePath + Send + Sync + 'static
struct LevelLoader;

impl AssetLoader for LevelLoader {
    type Asset = LevelAsset;
    type Settings = LevelLoaderSettings;
    type Error = BevyError;   // BevyError 对任何 impl std::error::Error 的类型有 blanket From

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        // 解析 + 校验（版本不匹配必须硬失败并给出清晰错误，见 `00` §0.7 与 `03`）
        let level: level_format::Level = level_format::parse_and_validate(&bytes)?;
        Ok(LevelAsset(level))
    }

    fn extensions(&self) -> &[&str] { &["level.ron"] }
}

fn build(app: &mut App) {
    app.init_asset::<LevelAsset>()
       .init_asset_loader::<LevelLoader>();
}

fn use_it(asset_server: Res<AssetServer>) -> Handle<LevelAsset> {
    asset_server.load("levels/world_01.level.ron")
}
```

- `AssetLoader` 精确 associated types：`Asset: Asset`、`Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>`、`Error: Into<BevyError>`；方法 `load(&self, reader: &mut dyn Reader, settings, load_context) -> impl ConditionalSendFuture`、可选 `extensions()`；注册走 `AssetApp::init_asset::<A>()` + `init_asset_loader::<L>()`（`AssetApp` 在 prelude）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/asset/trait.AssetLoader.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/trait.AssetApp.html>
- `AssetServer::load<A>(path) -> Handle<A>`；`Assets<A>`：`add` / `insert`(返回 `Result`) / `get` / `get_mut` / `reserve_handle` 。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.AssetServer.html> ；<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Assets.html>
- **热重载**：cargo feature `file_watcher`（或 `dev` 集合）+ `AssetPlugin.watch_for_changes_override`；变更后 `AssetServer` 重载并发出 `AssetEvent::Modified { id }`（0.20 中是 **Message**，用 `MessageReader<AssetEvent<T>>` 读）。`.level.ron` 这类文本资产**不需要** `asset_processor`。官方无内置 RON 资产插件，必须自写 loader。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/asset/struct.AssetPlugin.html> ；<https://docs.rs/bevy/0.20.0/bevy/asset/enum.AssetEvent.html>
- 加载状态：`AssetServer::load_state` / `is_loaded_with_dependencies`，`LoadState::{ NotLoaded, Loading, Loaded, Failed(..) }`。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/asset/enum.LoadState.html>
- **【INFERENCE】** `extensions()` 返回 `&["level.ron"]`（双扩展名整体匹配）的依据是 `AssetPath` 的 full-extension 语义；**本项目 `.level.ron` 的匹配规则必须先做最小验证**（见 §10.4）。
- **`level_format` crate 不依赖 bevy**（见 `00` §0.6）：loader 里只做「读字节 → 交给 `level_format` 解析校验 → 转成 Bevy 资产」。

### 10.2.12 sprite 帧动画（结论：**必须自写**）

```rust
#[derive(Component)]
struct AnimationIndices { first: usize, last: usize }

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(&AnimationIndices, &mut AnimationTimer, &mut Sprite)>,
) {
    for (indices, mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.just_finished()
            && let Some(atlas) = &mut sprite.texture_atlas
        {
            atlas.index = if atlas.index == indices.last { indices.first } else { atlas.index + 1 };
        }
    }
}
```

- **结论【已验证】：0.20 的 `bevy_animation` 不提供任何 sprite / 图集 / 帧动画能力**（模块内无 sprite/atlas/frame 相关类型）；官方 `sprite_sheet.rs` 就是上面这种「用户自定义组件 + 每帧系统」写法。
  <https://docs.rs/bevy/0.20.0/bevy/animation/index.html> ；https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html
- 落地方式（【INFERENCE】，与 `05` 的模块划分一致）：把这套收进本项目自己的 `sprite_anim` 插件，**不要**依赖 `bevy_animation` 做角色帧动画，也不要引入第三方帧动画 crate（见 `09`）。

### 10.2.13 `Timer` / `Gizmos` / 诊断

```rust
let mut timer = Timer::from_seconds(0.1, TimerMode::Repeating);
timer.tick(time.delta());
if timer.just_finished() { /* .. */ }

fn debug_draw(mut gizmos: Gizmos) {
    gizmos.rect_2d(Isometry2d::IDENTITY, Vec2::splat(crate::config::TILE_PX), Color::GREEN);
}

app.add_plugins((
    FrameTimeDiagnosticsPlugin::default(),
    EntityCountDiagnosticsPlugin,
    LogDiagnosticsPlugin::default(),
));
```

- `Timer` / `TimerMode`（`Once` / `Repeating`）：`Timer::new(Duration, TimerMode)`、`Timer::from_seconds`、`tick`、`just_finished`、`fraction`、`pause`（都在 prelude）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Timer.html>
- `Gizmos` 是 `SystemParam`，`Gizmos::config` / `config_ext` 为公开字段。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Gizmos.html>
- **`FrameTimeDiagnosticsPlugin` / `EntityCountDiagnosticsPlugin` / `LogDiagnosticsPlugin` 在 `bevy::diagnostic`**（不是 `dev_tools`）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/diagnostic/index.html>
- `bevy::dev_tools`（feature `bevy_dev_tools`）另有 `fps_overlay` / `frame_time_graph` / `inspection` / `schedule_data` / `states` 等。**【UNVERIFIED】** `FpsOverlayPlugin` 的确切可用性/feature 组合：本文件未取证其 0.20 签名，用前先查 <https://docs.rs/bevy/0.20.0/bevy/dev_tools/index.html>。

### 10.2.14 查询迭代（0.20 的 `Result` 语义）

```rust
// 单个：0.16 起 single() 返回 Result，不再 panic
fn one(q: Query<&Player>) {
    let Ok(p) = q.single() else { return; };
    let _ = p;
}

// 批量（"many"）迭代器 0.20 起 yield Result
fn many(list: Res<MyEntityList>, q: Query<&Health>) {
    for h in q.iter_many(list.0.iter()).matched() {   // .matched() == 旧行为
        let _ = h;
    }
}
```

- `Query::single()` / `single_mut()` 返回 `Result`（0.16 起）。【已验证】<https://bevy.org/learn/migration-guides/0-15-to-0-16/>
- `iter_many*` / `par_iter_many*` yield `Result<Item, QueryEntityError>`；`.matched()` 等价旧行为。【已验证】
  <https://bevy.org/learn/migration-guides/0-19-to-0-20/#query-iterators-from-many-entities-now-iterate-over-result-instead-of-querydata-item>

### 10.2.15 渲染关键开关（API 名称速记）

| 目标 | 0.20 写法 | 证据 |
|---|---|---|
| 全局最近邻采样 | `DefaultPlugins.set(ImagePlugin::default_nearest())` | <https://docs.rs/bevy/0.20.0/bevy/image/struct.ImagePlugin.html> |
| 关抗锯齿 | 每个相机插 `Msaa::Off`（默认是 `Sample4`，必须显式关） | <https://docs.rs/bevy/0.20.0/bevy/render/view/enum.Msaa.html> |
| 逐资源覆盖采样 | `asset_server.load_builder().with_settings(\|s: &mut ImageLoaderSettings\| s.sampler = ImageSampler::nearest()).load("x.png")` | <https://docs.rs/bevy/0.20.0/bevy/image/struct.ImageLoaderSettings.html> |
| 2D 色调映射 | 保持 `Camera2d` 默认 `Tonemapping::Linear`；不要加 `Hdr`；不需要 `tonemapping_luts` | <https://bevy.org/learn/migration-guides/0-19-to-0-20/#tonemapping-none-is-now-a-full-passthrough> |
| 离屏渲染目标 | `images.add(Image::new_target_texture(w, h, TextureFormat::Bgra8UnormSrgb, None))` + `RenderTarget::Image(handle.into())` | https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html ；<https://bevy.org/learn/migration-guides/0-17-to-0-18/#rendertarget-is-now-a-component> |
| 窗口配置 | `DefaultPlugins.set(WindowPlugin { primary_window: Some(Window { resolution: WindowResolution::new(crate::config::render::WINDOW_W, crate::config::render::WINDOW_H).with_scale_factor_override(1.0), ..default() }), ..default() })`（窗口尺寸归 `07`） | <https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowResolution.html> ；<https://docs.rs/bevy/0.20.0/bevy/window/struct.WindowPlugin.html> |
| 窗口尺寸变化消息 | `MessageReader<WindowResized>`（**不在 prelude**，显式 `use bevy::window::WindowResized;`） | <https://docs.rs/bevy/0.20.0/bevy/prelude/index.html>（索引无此符号） |

- `hdr` cargo feature **不等于**相机 HDR（它只是 HDR 图片格式解码）；相机 HDR 组件是 `bevy::camera::Hdr`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/camera/struct.Hdr.html>

### 10.2.16 全局音量 / 窗口显示模式（M7 设置界面）

```rust
use bevy::audio::{GlobalVolume, Volume};
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

fn apply(
    settings: Res<GameSettings>,
    mut global: ResMut<GlobalVolume>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    global.volume = Volume::Linear(settings.volume);          // 0.0..=1.0
    window.mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
}
```

- `GlobalVolume { volume: Volume }` 是 **Resource**（0.19 起 `Resource: Component`，所以页面同时列出 Component impl）；`GlobalVolume::new(volume)` / `From<Volume>`；**修改只影响之后创建的音源**。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/audio/struct.GlobalVolume.html>
- `Volume::{ Linear(f32), Decibels(f32) }`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/audio/enum.Volume.html>
- `WindowMode::{ Windowed, BorderlessFullscreen(MonitorSelection), Fullscreen(MonitorSelection, VideoModeSelection) }`；无边框全屏会按显示器分辨率改窗口物理尺寸（scale factor 规则见 `07`）。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/window/enum.WindowMode.html>
- `MonitorSelection::{ Current, Primary, Index(usize), Entity(Entity) }`；`Current` = 窗口当前所在显示器。【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/window/enum.MonitorSelection.html>
- 窗口实体用 `Single<&mut Window, With<PrimaryWindow>>` 取得；写 `Window::mode` 即请求 winit 切换。【已验证】（本仓 M7 实机：`全屏：开` → `全屏：关`，窗口尺寸随之变化，`fit_canvas` 重新计算 `k`）

---

## 10.3 0.20 破坏性变更 Top 10（含对本项目影响）

> 来源：<https://bevy.org/learn/migration-guides/0-19-to-0-20/>（条目细节见各节链接）。「影响」列只写本项目真实会碰到的点。

| # | 变更 | 对本项目的影响 |
|---|---|---|
| 1 | **Observer 去掉 `B: Bundle` 泛型**：`On<Add, A>` → `On<Add<A>>` | 组件新增/移除的 observer 全部改写；`#[require]` 相关的观察逻辑受影响 |
| 2 | **指针事件扁平化**：`Pointer<Press>` → `PointerPress` | 编辑器（`08`）拖拽/框选、菜单点击全受影响；`PointerEvent` trait 用于泛型 |
| 3 | **`Sprite` 迁到 Mesh2d/Material2d**，新增 `alpha_mode`；**同 Z 的绘制顺序可能变化** | 直接威胁像素风叠层正确性 ⇒ 所有 sprite 必须显式分级 Z，Z 取值见 `00` §0.4；手写 `Sprite { .. }` 必须 `..default()`；角色与装饰品（`04`）的叠层顺序受此影响 |
| 4 | **`SpriteMaterial` → `SpriteMeshMaterial`** | 若将来做角色描边/闪白等自定义 sprite 材质，用 `MaterialExtension2d`/`ExtendedMaterial2d` |
| 5 | **文本字号/字体源**：`TextFont::default()` 从 `Px(20)` 变 `Rem(1.)`；`font` 变 `FontSource`；`FontSource::SansSerif` → `sans_serif()`；新增 `Val::Em`/`Val::Rem`，`Val::resolve` 增参 | HUD（`07`）与编辑器（`08`）里**所有文本必须显式 `FontSize::Px(..)`**，否则会跟随 `RemSize` 漂移 |
| 6 | **状态 API 改名**：`NextState::set_if_neq` → `set_if_different` | 房间/游戏状态切换代码统一用新名 |
| 7 | **"many" 查询迭代器 yield `Result`** | 房间内实体列表（`RoomMarkers`、拾取物集合）批量查询需 `.matched()` |
| 8 | **crate 拆分**：几何 → `bevy_shape`，曲线 → `bevy_curve` | 用 `bevy::prelude::*` 时无感；一旦 `default-features = false` 必须补 feature（与 `09` 的 feature 决策联动） |
| 9 | **UI 交互**：`ui::widgets::Button` / `ui::Interaction` 弃用 → `ui_widgets::Button`、`Hovered`、`Pressed` | 编辑器（`08`）所有按钮/开关控件必须用新组件；`bevy_egui` 内部的交互不受此影响（走 egui 自己的输入） |
| 10 | **独占系统与普通系统统一**：`&mut World` 成为 `SystemParam`，可出现在参数列表任意位置；`System::is_exclusive()` 移除 | 物理内核是纯函数（`00` D12），ECS 侧几乎不写独占系统；若有 `fn(&mut World, ...)` 形式的系统，去掉旧约束 |

补充（同样与 2D 相关，须记住）：

- `Camera2d` 默认 `Tonemapping::Linear`；`Tonemapping::None` 现为完全直通（见 C8）。
- **`ScreenSpaceTransmission` 改为 opt-in**：只影响 `Camera3d`，本项目不用 3D ⇒ 无影响。
  <https://bevy.org/learn/migration-guides/0-19-to-0-20/#opt-in-screenspacetransmission>
- 着色器改用 WESL（`.wgsl` → `.wesl`，`#import` → `import …;`，GLSL 支持移除）。
- UI 渲染改为 retained（`ExtractedUiNode` 结构变化）；`CalculatedClip` 变 enum。
- `EditableText` / `TextInput` 拆分（编辑器文本输入需两个组件）。
- `Name: From<&'static str>`（非 `'static` 需 `to_owned()`）。
- `bevy_scene`（旧）改名 `bevy_world_serialization`，`bevy::scene` 现在是 BSN。
- 0.20 新增 `spawn_scene(bsn! { .. })` 场景写法（官方 example 已在用），但与元组 spawn 并存，**本项目暂不采用 BSN**（保持与 `00` 的组件/关系约定一致）。

---

### 10.3.1 本项目**实测**的环境级坑（不是 API 重命名，但同样会让人查半天）

| # | 现象 | 根因与处置 | 证据 |
|---|---|---|---|
| E1 | 启动即 panic：`Res<MeshPipelineViewLayouts>` / `Parameter ... failed validation: Resource does not exist` | 用了 `bevy/dev` **集合**：它含 `render_dev_tools` → `bevy_dev_tools/render` → 拖入 `bevy_pbr`（3D），而 `render_debug` 的初始化系统要 3D 网格管线资源；本项目是 2D-only（`3d` profile 未开）⇒ 资源不存在。**处置**：`dev` 只列 `bevy/debug` + `bevy/bevy_dev_tools` + `bevy/file_watcher`（`09` §9.4.1、`07` §7.9.2.1） | `bevy_dev_tools/render` 的依赖含 `bevy_pbr ^0.20.0`：<https://docs.rs/crate/bevy_dev_tools/0.20.0/features> |
| E2 | 依赖解析阶段直接失败：`bevy@0.20.0 requires rustc 1.97.1` | Bevy 0.20.0 的 MSRV 是 **1.97.1**，不是「最新 stable」的模糊说法。**处置**：`rustup update stable`（本机实测到 1.99.1） | 实测 cargo 输出；`09` §9.1 的 `rustc` 行 |
| E3 | 改一行本地代码却触发 Bevy **全量重编译**（~6 分钟） | Bevy 的 feature 集由「被选中的包集合」决定：单包入口（`-p game`）与全仓入口（`--workspace`）解析出**不同**的 bevy feature 集（少了另一个包带来的 feature）⇒ 缓存失效。**处置**：一次会话只用一类入口；`--workspace` 与 `--bin` 不能组合，所以别指望统一 | 实测：先 `--workspace --features dev` 再 `-p game --features dev`，重编 `bevy_internal`/`bevy`/`game`，耗时 5m54s |
| E4 | 直接运行 exe 时找不到 `assets/` | `get_base_path()` 顺序是 `BEVY_ASSET_ROOT` → `CARGO_MANIFEST_DIR` → **exe 所在目录**；裸跑 `target/debug/animj.exe` 时前两者都不在 ⇒ 去 `target/debug/assets/` 找。**处置**：开发期用 `cargo game`（Cargo 会设 `CARGO_MANIFEST_DIR`）；发布期把 `assets/` 放在 exe 同级（`09` §9.6.3） | 实测日志：`Path not found: ...\target\debug\assets\levels/world_01.level.ron` |
| E5 | 截图 API 找不到 | 0.20 的路径是 `bevy::render::view::window::screenshot`（**带 `window` 段**），不是 0.18 时代的 `bevy::render::view::screenshot`；`Screenshot::image(handle)` 可抓离屏渲染目标。**注意**：被截图的渲染目标必须带 `TextureUsages::COPY_SRC`（`new_target_texture` 默认不含） | 【已验证】<https://docs.rs/bevy/0.20.0/bevy/render/view/window/screenshot/struct.Screenshot.html>（`image`/`primary_window`）、<https://docs.rs/bevy/0.20.0/bevy/render/view/window/screenshot/fn.save_to_disk.html> |
| E6 | 某个系统**完全没有任何效果**，日志也**什么都不打** | `Single<T>` 在匹配到 **0 个或 2 个以上**实体时会**静默跳过整个系统**（这是设计行为，不是错误）。实测：HUD 的 `Single<&mut Text>` 同时匹配到 HUD 与调试文本两个实体 ⇒ `update_hud` 从未执行，屏幕上是生成时的初值，看截图会误判成「接线错了」。**处置**：`Single` 一律配上 `With<Marker>` 过滤条件 | 实测（本项目 M0），本仓 `crates/game/src/render/hud.rs` 的 `HudText` 标记 |
| E7 | 文本只渲染出一部分（或只剩第一个词） | Bevy 的 `default_font` **只含 ASCII 字形**，**遇到第一个缺字形后会把该行后续文本一起丢掉**（实测：`"animj · M0 骨架"` 只出 `animj`）。**处置**：TTF 入库前 UI/世界文本只用 ASCII（`04` §9） | 实测（本项目 M0）；feature 表原文 "Include a default font, containing only ASCII characters" <https://docs.rs/bevy/0.20.0/bevy/> |

| E8 | 「按键被吞」：按键/注入的消息读不到 | `ButtonInput<T>` 由 Bevy 的输入系统在 `PreUpdate` 内更新；同一 schedule 内**没排序**的读取系统可能先跑 ⇒ 读到上一帧的状态。**处置**：自己的采样系统必须 `.after(bevy::input::InputSystems)`（0.20 的集合名是**复数** `InputSystems`） | 实测（M1）；<https://docs.rs/bevy/0.20.0/bevy/input/struct.InputSystems.html> |
| E9 | 按渲染帧计时的脚本/计时器在无人值守会话里"跑不完" | 本会话窗口不受 vsync 限流（实测 ~1000–2000 fps），渲染帧数比固定步数多两个数量级 ⇒ 一个 120 帧的短跑只推进约 4 个物理步。**处置**：验证脚本一律按**固定步**计时（M1 的 `demo.rs` 用 `FixedPreUpdate`）。**反面同样致命**：需要"运行中做某件事"的验证（如 M3 热重载）不能按帧数估算存活时长——窗口失焦时 >200 fps，`--frames 300` 两秒就跑完，实测连续两轮把"改文件"做在了进程退出之后，误判为「watcher 不工作」。**处置**：用很大的帧数启动，动作完成后主动 `kill` | 实测（M1/M3） |
| E10 | `AssetServer::reload` 的入参 | 0.20 里接收 **`AssetPath`**（`&str` / `String`，如 `"levels/world_01.level.ron"`），**不是** `Handle<A>`；传 handle 会报 `AssetPath<'_>: From<Handle<A>> is not satisfied`。热重载的正解是**监听** `MessageReader<AssetEvent<A>>` 的 `Modified`，不需要手动 reload | 实测（M3） |
| E11 | 资产热重载的可用性 | `dev` 特性已含 `bevy/file_watcher` ⇒ `AssetPlugin` 默认即监听文件（**不需要** `watch_for_changes_override`，加了反而冗余）。实测：运行中保存 `*.level.ron` → 资产管线重新加载并发 `AssetEvent::Modified` ⇒ 自己监听即可。`AssetEvent` 属于 Message 体系（`MessageReader`） | 实测（M3） |
| E12 | 碰撞分离把 AABB 压扁 → 碰撞静默关闭 | 分离只改行进侧一面（`max.x = 墙面`）而不恢复另一侧：若身体**已经**与某格重叠（重生点埋进地板），盒子会被压扁/压反。反向盒子的 `tile_range` 返回空区间 ⇒ 一张瓦片都不查 ⇒ 穿地板、掉出相机、贴墙"慢慢卡出去"，**且没有任何报错**。规范：分离保持尺寸 + 只认前方的面 + `step` 开头自检修复（`06` §6.4.9、R1–R5）；根因常常是"重生点写成地板顶面而不是 AABB 中心" | 实测（实机三连 bug 回归） |
| E13 | 输入"很不灵敏"：按下被吞 | `just_pressed()` 只活**一帧**，而 `FixedUpdate` 每帧可能跑 **0 次**（渲染帧 60 fps 与固定步 60 Hz 不同频，抖动下约一半帧跑 0 或 2 步）⇒ 采样若每帧直接覆盖为 `just_pressed`，没有固定步的帧就把按下丢了。规范：采样**锁存**（只置位），物理步消费后清零 ⇒ 恰好一个固定步看到一次按下；这样"按住连跳"仍不可能（用 `pressed()` 的 `jump_held` 区分） | 实测（实机手感 bug） |

---

## 10.4 【UNVERIFIED】集中清单

> **以下条目不得据此写代码，需先做最小验证**（20 行以内、单独 example/测试），验证后回填本文并升级标记。

| # | 未取证项 | 现状 / 已尝试 | 验证方式 |
|---|---|---|---|
| U1 | `2d` profile 是否已含 `bevy_ui`/`bevy_text`/`bevy_audio` | 官方 Profiles 表只给一句描述（"core Bevy framework, 2D functionality, scenes and picking"），**未列展开集合**。已读 <https://docs.rs/bevy/0.20.0/bevy/index.html> | 在空 crate 里用 `default-features = false, features = ["2d"]` 编译 `use bevy::prelude::*; Node::default()`，看是否缺符号。最终结论写 `09` |
| U2 | `#[derive(Bundle)]` 在 0.20 是否仍存在/推荐 | 0.20 文档未找到 `derive.Bundle` 页面；`Bundle` trait 与元组 impl 已确认存在 | 不验证、不使用：一律元组 + `#[require]` |
| U3 | `AssetLoader::extensions()` 对 `.level.ron` 双扩展名的匹配规则（`"ron"` 还是 `"level.ron"`） | 未找到 0.20 直接证据；本文件 §10.2.11 给的是【INFERENCE】 | 写一个最小 loader，分别注册 `&["ron"]` 与 `&["level.ron"]`，观察哪个被调用 |
| U4 | `MessageWriter::send` 是否仍作为弃用别名存在 | `MessageWriter` 页面公开方法只有 `write`；官方 example 注释里出现过 `send` 字样 | 不验证：统一写 `write` |
| U5 | `bevy_dev_tools::fps_overlay::FpsOverlayPlugin` 的 0.20 签名与所需 feature | 仅确认 `bevy::dev_tools` 模块与 `fps_overlay` 子模块存在 | 用前现查 <https://docs.rs/bevy/0.20.0/bevy/dev_tools/index.html> |
| U6 | `Projection` 在运行时被改为 required component 的顺序/时机（`Camera2d` 编译期 required 列表与运行期注册不一致） | `Camera2d` 的 rustdoc required components 列表（`Camera`/`Projection`/`Frustum`）与 `Core2dPlugin` 运行期注册的 `Tonemapping`/`DebandDither` 不一致 | 以源码/迁移指南为准；管线实现见 `07` |
| U7 | `viewport_to_world_2d` 的正确调度位置（`PostUpdate` + `.after(TransformSystems::Propagate)`）在 0.20 的等价写法 | 官方 example 注释提到该要求，但 0.20 `TransformSystems` 的确切符号未逐字取证 | 编辑器（`08`）取鼠标世界坐标时先按此写，并做最小验证 |
| U8 | BSN（`bsn!` / `spawn_scene`）的成熟度与推荐度 | 0.19 引入、0.20 改语法；官方 example 已大量使用 | 本项目保守：**不用 BSN**，继续元组 + `#[require]` + `ChildOf` |
| U9 | `Single<T>` 与 `Query::single()` 在「多实体匹配」时的行为差异细节 | 0.17 指南记录了 `Option<Single<..>>` 语义变化 | 需要时按 `Query<..>` + `single()` 写，错误分支显式处理 |

---

## 10.5 取证方法备忘（给下一个 agent）

### 可用/不可用的域名（本环境实测）

| 域名 | 状态 | 替代方案 |
|---|---|---|
| `https://docs.rs/bevy/0.20.0/…` | ✅ 可用，**首选** | — |
| `https://bevy.org/…`（迁移指南、release notes、quick start） | ✅ 可用 | — |
| `https://github.com/bevyengine/bevy/…` | ⚠️ 直读经常 TLS/证书失败 | 用 `docs.rs` 的源码页（见下） |
| `https://crates.io/api/v1/…` | ❌ HTTP 403 | 用 `https://docs.rs/crate/<crate>/<ver>` 或 `https://docs.rs/crate/<crate>/latest` 看依赖表 |
| `https://raw.githubusercontent.com/…` | ⚠️ 与 github.com 同样不稳定 | 同上 |

### 读 0.20 官方 example 源码（与 0.20 完全一致，比 GitHub main 更可信）

```
https://docs.rs/bevy/0.20.0/src/<example_name>/<example_name>.rs.html
例：
https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html
https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html
```

rustdoc 页面本身也会附带 **scraped examples**（页内 "Examples found in repository" 区块直接给出官方 example 的相关行），例如
<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html> 内就贴了 `sprite_sheet.rs` 的完整 `setup`。

### 判定「某符号不存在」的标准手法

1. 抓 `https://docs.rs/bevy/0.20.0/bevy/prelude/index.html`，在页面内全文检索该符号。
2. 直接请求该符号的 rustdoc URL：**HTTP 404 = 该路径不存在**。
   例：`struct.Camera2dBundle.html` → 404；`enum.JustifyText.html` → 404；`struct.UiImage.html` → 404；`struct.Anchor.html`（prelude 路径）→ 404，而 `bevy/sprite/struct.Anchor.html` 正常。
3. `App` 等巨型页面用页面内检索（本仓库工具可直接 grep URL）代替人工翻页。

### 交叉验证顺序（强制）

1. `docs.rs/bevy/0.20.0/**`（类型/方法签名，最权威）
2. `bevy.org/learn/migration-guides/<a>-to-<b>/`（Before/After，判断「旧写法→新写法」）
3. `docs.rs/bevy/0.20.0/src/<example>.rs.html`（真实可编译用法）
4. `agent://CoreApi` / `agent://PixelPipeline` / `agent://Ecosystem`（已整理的 0.20 事实清单）

**任何一步拿不到证据 ⇒ 写进 §10.4，不得写进代码。**
