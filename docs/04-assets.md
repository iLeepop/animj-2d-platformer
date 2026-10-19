# 04 — 美术资源规格（assets）

> 本文是**资源路径与物理尺寸**的唯一所有者：目录树、文件命名、id 规则、图集物理布局、锚点、动画帧排列、占位资产、导入设置。
> 常量归属见 `00` §0.2；**本文只在自己主题内定义常量**，其余一律跨文档引用（写「见 `00` §…」形式，不复制数值）。
> 关于 Bevy 0.20 API 的断言带标记：【已验证】/【UNVERIFIED】/【INFERENCE】（定义见 `00` §0.9）。

---

## 0. 边界与所有权

| 内容 | 归属文档 |
|---|---|
| 资源根目录 `assets/` 的顶层子目录 | `00` §0.6（**本文不得增删顶层目录**） |
| 瓦片网格边长 `TILE_PX`、房间尺寸 | `00` §0.3 |
| Z 层分配（含装饰品图层 Z 公式） | `00` §0.4 |
| 资源文件命名 / 装饰品 id 规则 | `00` §0.5（本文 §2 展开为可执行规则，不新增命名空间） |
| legend 字符 → `TileKind` 映射、`tileset.png` 的**索引语义** | `03-level-format.md`（**本文只定义 `tileset.png` 的物理布局，§10**） |
| 采样器滤波、`ImagePlugin`、相机/像素吸附等渲染配置 | `07-rendering.md` |
| 装饰品**清单（哪些 id 存在）**与解锁规则 | `01-game-design.md` |
| 物理/手感常量（角色 AABB 尺寸、速度） | `06-physics-and-movement.md` |

**原则**：本文中出现的每个像素尺寸、帧数、目录都必须是「生成器可执行 + 真美术可无痛替换」的硬约束。改本文的这些值 = 破坏性变更，必须同步改占位生成器与已产出的 PNG。

---

## 1. 目录结构

顶层严格等于 `00` §0.6 的 `assets/` 布局；下列为展开后的**完整子目录树与允许出现的文件**（不允许出现清单外的子目录）：

```
assets/
├─ levels/
│  └─ <level_id>.level.ron          # 见 03，非美术资源，仅登记目录
├─ sprites/
│  ├─ player/
│  │  └─ cat_sheet.png              # 角色动画图集（§3.2 / §5）
│  ├─ tiles/
│  │  └─ tileset.png                # 唯一瓦片图集（§3.3 / §10）
│  ├─ decorations/
│  │  └─ <id>.png                   # 每个装饰品一张 32×32 静态贴图（§6）
│  ├─ backgrounds/
│  │  └─ <name>.png                 # 视差背景层（`03` §3.10 / `07` §7.13）；尺寸自定
│  └─ ui/
│     ├─ ui_atlas.png               # UI 图标图集（可选，§9）
│     └─ <name>.png                 # UI 单图（可选，§9）
└─ fonts/
   └─ <font_name>.ttf               # 字体（§9）
```

- **不允许**新增顶层目录（如 `audio/`、`palettes/`）。需要新类别时先改 `00` §0.6。
- **不允许**在 `assets/` 下放 `.psd`/`.aseprite`/`.kra` 等工程源文件；源文件放仓库外或 `art-src/`（非运行时资产，见 §7 零代码约束）。
- 文件系统路径**一律小写 snake_case**，目录名复数（`sprites`/`levels`/`fonts`）。

---

## 2. 命名与 id 规则

| 类别 | 规则 | 示例 | 依据 |
|---|---|---|---|
| 角色图集 | 固定文件名 `assets/sprites/player/cat_sheet.png` | — | `00` §0.6 |
| 瓦片图集 | 占位资产固定为 `assets/sprites/tiles/tileset.png`；真实美术可由关卡 `tileset` 字段指向独立文件（如 `tileset_n.png`），并用该关卡的 `tileset_columns` 描述列数 | — | `00` §0.5 / §0.6、本文 §7.3 |
| 装饰品 | `snake_case`，**文件名（去扩展名）严格等于装饰品 id** | id `straw_hat` → `assets/sprites/decorations/straw_hat.png` | `00` §0.5 |
| 背景层 | `snake_case`，可自由命名；尺寸自定（建议 ≥ 512×512），必须能无缝平铺 | `assets/sprites/backgrounds/sky.png` | `03` §3.10 / `07` §7.13 |
| 关卡文件 | `assets/levels/<level_id>.level.ron`，文件内 `name` 必须等于 `<level_id>` | — | `00` §0.5 |
| 字体 | `<font_name>.ttf`，`snake_case` | `assets/fonts/pixel_sans.ttf` | 本文 §9 |
| UI 图集/单图 | `snake_case`，图集固定名 `ui_atlas.png` | `assets/sprites/ui/heart.png` | 本文 §9 |

id 规则（与 `00` §0.5 一致）：

```
<id> ::= [a-z][a-z0-9_]*        // 以小写字母开头，仅小写字母/数字/下划线
```

- **禁止**在 id 或文件名中出现大写字母、空格、连字符、非 ASCII。
- 装饰品 id 是**唯一键**：`01` 的装饰品清单、`assets/sprites/decorations/<id>.png`、存档 `SaveData` 中的记录、运行时组件字段必须**逐字节相同**。
- 角色图集/瓦片图集**不接受**多版本命名（禁止 `cat_sheet_v2.png`、`tileset_dark.png`）；换美术 = 覆盖同名文件（§7）。

---

## 3. 基础尺寸与图集布局

### 3.1 尺寸契约

| 项 | 值 | 说明 |
|---|---|---|
| 网格单元（像素艺术「逻辑像素」） | `TILE_PX`（见 `00` §0.3） | 所有贴图/图集的单元边长唯一来源 |
| 运行时缩放 | `Transform.scale = 1.0`，**禁止** `Sprite::custom_size` | 1 世界单位 = 1 逻辑像素（`00` §0.3），画布尺寸 = 源图像素尺寸 |
| 图集 `padding` | `0`（调用处传 `None`） | 像素完美必需，单元间不得有空隙 |
| 图集 `offset` | `0`（调用处传 `None`） | 同上 |
| 图集总尺寸 | **宽与高都必须能被 `TILE_PX` 整除** | 否则索引与像素吸附错位 |
| 图集内索引顺序 | **行主序：从左到右、从上到下** | 由 `from_grid` 保证（见下） |

### 3.2 精确调用写法【已验证】

`TextureAtlasLayout::from_grid` 精确签名（0.20.0）：

```rust
pub fn from_grid(
    tile_size: UVec2,          // 单元尺寸
    columns: u32,              // 列数
    rows: u32,                 // 行数
    padding: Option<UVec2>,    // 单元间空隙；None = 0
    offset: Option<UVec2>,     // 全局偏移；None = 0
) -> TextureAtlasLayout
```

> "Resulting layout is indexed left to right, top to bottom."
> 证据：<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html#method.from_grid>【已验证】

本项目统一以**常量**调用，`padding`/`offset` **永远传 `None`**：

```rust
use bevy::prelude::*;               // TextureAtlasLayout / TextureAtlas 均在 prelude
use bevy::sprite::Anchor;           // Anchor 不在 prelude，见 §4

// —— 角色图集：6 列 × 4 行 ——
const PLAYER_SHEET_COLUMNS: u32 = 6;
const PLAYER_SHEET_ROWS: u32 = 4;

fn player_layout(layouts: &mut Assets<TextureAtlasLayout>) -> Handle<TextureAtlasLayout> {
    layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(TILE_PX),          // 32×32 单元；TILE_PX 非 u32 时写 TILE_PX as u32
        PLAYER_SHEET_COLUMNS,
        PLAYER_SHEET_ROWS,
        None,                           // padding = 0
        None,                           // offset  = 0
    ))
}
```

- `TextureAtlasLayout`、`TextureAtlas` 在 `bevy::prelude`；`UVec2` 亦在 prelude。
  【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html>
- 用图集生成 sprite：`Sprite::from_atlas_image(Handle<Image>, TextureAtlas { layout, index })`【已验证】
  <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Sprite.html>、官方用例 <https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html>
- 索引算式（本文所有图集通用）：

```
index(row, col) = row * COLUMNS + col        // 0-based
```

### 3.3 图集清单

| 图集 | 文件 | 网格（列×行） | 总尺寸（像素） | 索引语义 |
|---|---|---|---|---|
| 角色 | `assets/sprites/player/cat_sheet.png` | `6 × 4` | `192 × 128` | 见 §5 |
| 瓦片 | `assets/sprites/tiles/tileset.png` | `TILESET_COLUMNS × N`（§10） | `(32·TILESET_COLUMNS) × (32·N)` | 由 `03` legend 固定 |
| 装饰品 | `assets/sprites/decorations/<id>.png` | `1 × 1` | `32 × 32` | 静态单帧 |
| UI | `assets/sprites/ui/ui_atlas.png` | 由 §9 决定 | 同上规则 | UI 专用 |

验证整除：`192 = 32×6`，`128 = 32×4` ✓。

---

## 4. 锚点 / 枢轴约定

**路径证据**：`Anchor` **不在** `bevy::prelude`（`https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Anchor.html` → HTTP 404），必须用 `bevy::sprite::Anchor`。【已验证】
<https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html>【已验证】

`Anchor(pub Vec2)` 是归一化锚点（0=左/上，0.5=中，1=右/下），常量含 `BOTTOM_CENTER` 等。【已验证】（同上 URL，常量列表）

| 对象 | Anchor | 对齐基准 |
|---|---|---|
| 角色本体 sprite | `Anchor::BOTTOM_CENTER` | **脚底中点**对齐角色 AABB 的**底边中点** |
| 装饰品 sprite | `Anchor::BOTTOM_CENTER` | 同角色基准 + 每装饰品偏移（见下） |
| 瓦片 sprite | `Anchor::BOTTOM_CENTER` | 瓦片 AABB 的**底边中点**（全局统一约定，见 `07` §7.8） |
| 拾取物 / 敌人 | `Anchor::BOTTOM_CENTER` | 各自 AABB 底边中点 |
| UI `ImageNode` | 不适用（用 `Node` flex 布局） | — |

**规则**：

1. 所有世界空间 sprite 的锚点必须**显式指定**，禁止依赖 `Anchor` 默认值。
2. 锚点是「视觉对齐」；**逻辑真相永远是 `Aabb`**（`00` §0.3）。sprite 位置由 AABB 反推，不由 `Transform` 反推。
3. **脚底对齐公式**（角色）：

```
player_anchor_world = Vec2::new(aabb.center().x, aabb.min.y)   // 脚底中点（世界像素）
sprite_anchor_xy    = player_anchor_world                       // Anchor::BOTTOM_CENTER
```

4. **装饰品图层偏移公式**（§6 的每 id 偏移）：

```
decor_world = player_anchor_world + offset(id)     // offset(id): Vec2，单位像素，来自 §6 表
```

5. **镜像一致性**：若角色 sprite 设 `flip_x = true`，则其所有已装备装饰品 sprite 必须同样设 `flip_x = true`（否则左右不对称）。镜像时 `Anchor::BOTTOM_CENTER` 不改变对齐，但装饰品的 `offset(id).x` 需随之取反（见 §6 公式）。

---

## 5. 角色动画帧约定

### 5.1 结论：`bevy_animation` **不支持** sprite 帧动画【已验证】

**必须自写 `AnimationTimer + AnimationIndices + TextureAtlas.index` 系统。**

1. `bevy::animation` 模块索引中**没有任何** sprite / atlas / frame 相关类型；提供的是 `AnimationPlayer`、`AnimationClip`、`AnimationGraph`、`AnimationGraphHandle`、`AnimationTargetId`、`AnimatedBy`、`VariableCurve`、`AnimationTransitions` 及系统 `advance_animations` / `animate_targets`。`AnimationPlayer::play(&mut self, NodeIndex)` 播放的是 `AnimationGraph` 节点，**与 sprite 图集无关**。【已验证】<https://docs.rs/bevy/0.20.0/bevy/animation/index.html>、<https://docs.rs/bevy/0.20.0/bevy/animation/struct.AnimationPlayer.html>
2. `TextureAtlasLayout` 文档只说该类型「**可以**用于逐帧动画」，不提供任何现成动画组件。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextureAtlasLayout.html>
3. 官方 0.20 示例 `examples/2d/sprite_sheet.rs` 自己写了组件 + 每帧系统；`examples/2d/sprite_animation.rs` 亦然。【已验证】
   <https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html>、<https://docs.rs/bevy/0.20.0/src/sprite_animation/sprite_animation.rs.html>

### 5.2 图集排列（`cat_sheet.png`，6 列 × 4 行）

| 行 `row` | 动画 | 帧数 | 列范围 `col` | 图集索引区间 `[first, last]` | 播放 |
|---|---|---|---|---|---|
| 0 | `idle` | 4 | 0..=3 | `0..=3` | 循环 |
| 1 | `run` | 6 | 0..=5 | `6..=11` | 循环 |
| 2 | `jump` | 2 | 0..=1 | `12..=13` | 单次（上升段） |
| 3 | `fall` | 2 | 0..=1 | `18..=19` | 单次（下落段） |

- 每行**左对齐**填充；该行未用到的列（如 `jump` 的 2..=5）必须为**全透明像素**，不得留白花纹。
- 索引一律由 `index(row, col) = row * 6 + col` 得出，禁止硬编码到图集最后一格。
- 帧率随动画（每帧时长）：

| 动画 | 帧时长（秒/帧） | 环行为 |
|---|---|---|
| `idle` | `0.20` | 循环 |
| `run` | `0.08` | 循环 |
| `jump` | `0.10` | 停在最后一帧 |
| `fall` | `0.12` | 停在最后一帧 |

> 帧时长是本主题的**动画常量**，定义在此。动画**选择**（何时播 `jump` vs `fall`）由 `06` 的 `Grounded` / 速度符号驱动；本文不定义物理常量。

### 5.3 可抄写实现（自研 `sprite_anim` 模块）

```rust
use bevy::prelude::*;
use bevy::sprite::Anchor;

// —— 组件 ——
#[derive(Component, Clone, Copy)]
pub struct AnimationIndices { pub first: usize, pub last: usize }

#[derive(Component, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer);

// —— 每帧推进系统（与官方 sprite_sheet.rs 等价）——
fn animate_sprite(
    time: Res<Time>,
    mut q: Query<(&AnimationIndices, &mut AnimationTimer, &mut Sprite)>,
) {
    for (indices, mut timer, mut sprite) in &mut q {
        timer.tick(time.delta());
        if timer.just_finished()
            && let Some(atlas) = &mut sprite.texture_atlas
        {
            atlas.index = if atlas.index >= indices.last {
                indices.first
            } else {
                atlas.index + 1
            };
        }
    }
}

// —— 按动画行切到新动画（值见 §5.2 表）——
pub fn set_animation(
    indices: &mut AnimationIndices,
    timer: &mut AnimationTimer,
    sprite: &mut Sprite,
    anim: Anim,
) {
    let (row, frames, secs) = match anim {
        Anim::Idle => (0usize, 4usize, 0.20f32),
        Anim::Run  => (1, 6, 0.08),
        Anim::Jump => (2, 2, 0.10),
        Anim::Fall => (3, 2, 0.12),
    };
    let first = row * PLAYER_SHEET_COLUMNS as usize;
    let last  = first + frames - 1;
    if indices.first != first {
        indices.first = first;
        indices.last  = last;
        if let Some(atlas) = &mut sprite.texture_atlas { atlas.index = first; }
    }
    timer.0.set_duration(core::time::Duration::from_secs_f32(secs));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Anim { Idle, Run, Jump, Fall }

// —— spawn 角色（节选；AABB 来自 06）——
fn spawn_player(mut commands: Commands, assets: Res<AssetServer>,
                mut layouts: ResMut<Assets<TextureAtlasLayout>>) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(TILE_PX), PLAYER_SHEET_COLUMNS, PLAYER_SHEET_ROWS, None, None));
    commands.spawn((
        Sprite::from_atlas_image(
            assets.load("sprites/player/cat_sheet.png"),
            TextureAtlas { layout, index: 0 },     // index 0 = idle 第一帧
        ),
        Anchor::BOTTOM_CENTER,
        AnimationIndices { first: 0, last: 3 },
        AnimationTimer(Timer::from_seconds(0.20, TimerMode::Repeating)),
        // Transform 的 z 由 07 的角色本体 Z 决定（00 §0.4）
    ));
}
```

- `Timer`/`TimerMode`、`Timer::from_seconds`、`just_finished`、`Deref`/`DerefMut`（`#[derive(Deref, DerefMut)]` 包 `Timer`）均为 0.20 现行 API。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Timer.html>、官方示例 <https://docs.rs/bevy/0.20.0/src/sprite_sheet/sprite_sheet.rs.html>
- 上面 `animate_sprite` 与官方 `examples/2d/sprite_sheet.rs` 逐行等价（官方用 `atlas.index == indices.last`；此处用 `>=` 以容忍外部改动索引）。
- 收进本项目的 `sprite_anim` 插件（归属见 `05`），**不**依赖 `bevy_animation` 做角色帧动画。

---

### 5.5 动画数据外置（`assets/anim/player.anim.ron（v2：每个状态自带 `sheet`/`columns`）`）

图集只描述"像素在哪"，**播放规则**（每个状态用哪些帧、每帧多久、是否循环）必须外置，才能被工具编辑。

```ron
(
    version: 1,
    sheet: "sprites/player/cat_sheet.png",
    columns: 6,
    states: [
        (name: "idle", frames: [0, 1, 2, 3],    frame_secs: 0.20, looping: true),
        (name: "run",  frames: [6, 7, 8, 9, 10, 11], frame_secs: 0.08, looping: true),
        (name: "jump", frames: [12, 13],        frame_secs: 0.10, looping: false),
        (name: "fall", frames: [18, 19],        frame_secs: 0.12, looping: false),
    ],
)
```

| 规则 | 说明 |
|---|---|
| `frames` | **图集索引**列表（行主序），长度 ≥ 1；空列表 = 该状态不可用（校验失败） |
| 状态集合 | 首批固定 4 个（`idle/run/jump/fall`），与 `06` 的选行规则一一对应；增删状态属破坏性变更 |
| `frame_secs` | > 0；范围 `0.02..=2.0` |
| `looping` | 循环 or 停在最后一帧 |
| 热重载 | 保存即生效（与关卡同一套 `AssetEvent` 机制） |
| 缺文件 | 用内置默认值（等价当前常量），只打一条 warn |
| 校验 | 索引必须落在 `columns × rows` 内（图集尺寸由 `columns` 与图片高度推出） |

### 5.6 贴图文件的外部改动（热重载）

贴图可能在编辑器/游戏运行期间被外部软件（Aseprite、Photoshop…）重画。所有**同步读盘**的贴图统一按
「路径 + 文件 `mtime`」判定变化，变化后重建使用它的实体；不重启、不写盘、不改 `doc`/`dirty`/undo。

| 贴图 | 检测与重建 | 生效范围 |
|---|---|---|
| 关卡瓦片图集（`level.tileset`） | `game::render::tiles::watch_level_tileset` 轮询 mtime → 置位 `TilesetReloadRequest` → `spawn_level_tiles` 全量重建 | 发布 bin 的瓦片、编辑器试玩 |
| 编辑器投影 / 左栏缩略图 | 缓存键加入 `game::assets::texture_stamp`；`editor::view::watch_doc_tileset` 变化时销毁全部 `EditorProjectionRoom` ⇒ 全量重建 | 编辑态画布、地块调色板缩略图 |
| 角色各状态图集（`player.anim.ron` 的 `sheet`） | `player::spawn::rebuild_player_sheets` 把每个状态图集的 mtime 纳入重建键 | 试玩与发布 bin 的角色贴图 |
| 动画面板预览 | `prepare_textures` 的图集缓存键加入 mtime | 编辑器动画面板 |
| 背景层贴图 | `game::render::parallax` 的贴图身份键 = 路径 + mtime，变化即重新解码并用新尺寸重建 | 编辑器画布、试玩、发布 bin |
| 内置图集 / 装饰品（走 `AssetServer`） | Bevy `file_watcher`（`dev` feature） | 同上 |

规则：

- 只用 `metadata().modified()` 轮询，不引入 `notify` 等新依赖；与 `anim_data::watch_anim_file` 同一套机制。
- 解码失败保留旧贴图并打 `error!`；文件被写一半时，下一次 mtime 变化会再次触发。
- 重画只允许覆盖 PNG 字节；`TILE_PX`、图集行列数、帧序列（§3、§5）不变。
- mtime 精度不足的文件系统（如 1 s 粒度）可能在同秒内漏掉一次改动；保存后隔一秒再观察即可。

## 6. 装饰品图层

### 6.1 图层顺序（从最靠近角色到最外层）

Z 值**唯一来源**见 `00` §0.4（本文不复制该公式）；本文只定义**槽位顺序**与**层数上限**。

| `layer_index` | 槽位名 | 语义（放什么） |
|---|---|---|
| 0 | `body` | 身体贴花（毛色斑纹、贴纸） |
| 1 | `feet` | 脚部（鞋、靴） |
| 2 | `torso` | 躯干（背带、马甲、围裙） |
| 3 | `neck` | 颈部（围巾、项圈、领结） |
| 4 | `face` | 面部（眼镜、面具、眼罩） |
| 5 | `head` | 头顶（帽子、头饰、耳朵挂件） |
| 6 | `outer` | 最外层（披风、尾饰、天线） |

- **最大图层数 `DECORATION_LAYERS = 7`**（`layer_index ∈ 0..=6`）。该上限由 `00` §0.4 给出的装饰品图层 Z 区间跨度决定，**与 `00` 冲突时以 `00` 为准**。
- 同一角色同一 `layer_index` 处**至多一个**装饰品；重复装备则**替换**（不是叠加）。
- 由于 `00` §0.4 规定装饰品整体绘制在角色本体**之前（上层）**，本项目**不提供**「角色背后」槽位；需要背向遮挡的部件（如真正的披风后摆）应改走前景瓦片层（见 `07`），不得私自改 Z。

### 6.2 装饰品元数据表（`config.rs` 中的常量）

每个装饰品 id 在本模块登记：`layer_index` + 与角色画布对齐的像素偏移。偏移基准 = §4 的 `player_anchor_world`（脚底中点）。

```
offset(id) = Vec2::new(dx, dy)      // 像素；dy 向上为正（Bevy +Y 向上）
decor_world = player_anchor_world + offset(id)          // 未镜像
decor_world = flip(player_anchor_world) + Vec2::new(-dx, dy)   // 角色 flip_x 时 x 取反
```

```rust
pub struct DecorationSpec { pub id: &'static str, pub layer: u8, pub offset: Vec2 }

// 示例登记（真实 id 清单归 01；此处只演示字段）
pub const DECORATIONS: &[DecorationSpec] = &[
    // DecorationSpec { id: "straw_hat", layer: 5, offset: Vec2::new(0.0, 2.0) },
    // DecorationSpec { id: "scarf",     layer: 3, offset: Vec2::new(0.0, 0.0) },
];
```

- 装饰品贴图 = 单帧 32×32（§3.3）。`offset` 是纯像素整数（`fract() == 0`），避免破坏像素吸附（见 `07`）。
- 装饰品**不参与**物理/碰撞，纯外观（`00` §0.1 D8）。

### 6.3 未装备时的行为

| 状态 | 行为 |
|---|---|
| 某槽位未装备 | **不 spawn** 该层的 sprite 实体（严禁 spawn 后设 `Visibility::Hidden` 占位） |
| 首次装备 | 创建 sprite：`Sprite::from_atlas_image(<id>.png …)` + `Anchor::BOTTOM_CENTER`，`flip_x` 跟随角色 |
| 替换装备 | 复用同一实体，仅替换 `Sprite.image`（`Handle<Image>`）与 `offset` |
| 卸下 | despawn 该层实体 |
| 存档缺失该 id | 视为未装备（不报错、不生成占位） |

- 实现建议：用 `DecorationSlot { layer: u8, id: DecorationId }` 组件标记「角色 + 槽位」；查询 `(DecorationSpec, )` 决定 Z（Z 值取自 `00` §0.4）。
- 每帧（或装备变更时）按 §4 公式重算所有 decorate sprite 的世界位置与 `flip_x`。

---

## 6.9 音效资产

| 项 | 约定 |
|---|---|
| 路径 | `assets/audio/<cue>.wav`；`<cue>` 为 `snake_case`（`00` §0.5） |
| 格式 | **WAV / PCM 16-bit / 单声道 / 44.1 kHz**（`bevy` 的 `wav` 特性；`audio` 特性只带 vorbis，`09` §9.4） |
| 时长 | 单个音效 ≤ `0.30 s`（短促、不叠成噪音） |
| 占位生成 | `animj-dev gen-placeholder-assets` 程序化生成（确定性方波/正弦 + 线性衰减），与图片资产同一条 `--check` 逐字节校验（§7） |
| 缺失 | 运行时缺文件只打一条 warn（`bevy_asset` 行为），**不得** panic |

### 6.9.1 首批 cue（与 `05` §5.6 的事件一一对应）

| cue | 触发 |
|---|---|
| `jump` | 玩家起跳（`StepOutcome.jumped`） |
| `land` | 落地（`StepOutcome.landed`） |
| `bump` | 撞头（`StepOutcome.head_bumped`） |
| `pickup` | `CollectiblePicked` |
| `clear` | `RoomCleared` |
| `decoration` | `DecorationGranted` |
| `death` | `PlayerDied` |

新增 cue 必须同时：本表一行、`assets/audio/` 一个文件、生成器一条 job（`08` §8.10.3）。

### 7.1.1 真美术替换流程（交给美术/用户）

1. **只覆盖字节**：路径、文件名、像素尺寸、图集网格、帧序全部不变（§7.1 的四条硬约束）。
2. 逐文件替换，**每次替换后**跑：

   ```bash
   cargo game -- --frames 90 --screenshot target/art
   ```

   期望 `VERIFY: PASS`（11 项自检不依赖美术内容）与 `target/art-canvas.png` / `target/art-window.png` 肉眼一致。
   **`animj-dev gen-placeholder-assets --check` 只用于占位阶段**：换成真美术后它必然失败（生成器还原不出真图），属预期，不要拿它当验收。
3. 清单（尺寸 / 网格 / 语义，均以本文件为准）：

| 文件 | 像素尺寸 | 网格与帧语义 |
|---|---|---|
| `cat_sheet.png` | `192 × 128` | `6 × 4`；行 = `idle/run/jump/fall`；有效帧 `4/6/2/2`，其余格**全透明** |
| `tileset.png` | `256 × 32` | `8 × 1`；格 `0..3` = `solid/oneway/spike/decor`（顺序不可变）；其余格全透明 |
| `ui_atlas.png` | `128 × 64` | `4 × 2` |
| `decorations/<id>.png` | `32 × 32` | 单帧；`<id>` = 关卡 `reward` 并集（当前 `item_01`…`item_30`） |
| `audio/<cue>.wav` | ≤ 0.3 s | PCM 16-bit / 单声道 / 44.1 kHz；cue 见 §6.9.1 |
| `fonts/<name>.ttf` | — | 可选，入库即启用（当前未入库，文本只能 ASCII，见 §9） |

4. **命名逐字节一致**（大小写也算）：Windows 不敏感、CI/Linux 敏感。
5. 交付前用编辑器复核：`cargo editor -- --frames 90` 打开，确认 30 件装饰品与全部瓦片显示正常。

## 7. 占位资产策略

**目标（`00` §0.1 D9）**：暂用程序化占位资产，后置替换真美术，且替换时**零 Rust 代码改动**。

### 7.1 约束：换真美术 = 只覆盖 PNG 字节

零代码改动成立的前提（**硬约束**，真美术必须遵守）：

1. 文件**路径与文件名**与 §1/§2 完全相同（含大小写）。
2. 图集**网格（列×行）、`padding=0`、`offset=0`、总尺寸被 `TILE_PX` 整除**与 §3.3/§5.2/§10 完全相同。
3. 每格内的**帧语义、排列顺序、朝向**与 §5.2 相同（行 = 动画，列 = 帧序，不可重排）。
4. 单张贴图的**像素尺寸**与 §3.3 相同。

> 只要满足以上四点，替换真美术**不需要改任何** `.rs` 文件、常量或图集布局调用。

### 7.2 工具：`animj-dev gen-placeholder-assets`

- 命令归属：bin `animj-dev`（见 `00` §0.6，crate `crates/editor/`）。
- 用法：

```
animj-dev gen-placeholder-assets [--out assets] [--force]
```

- 行为：
  - 若目标文件已存在且未传 `--force`，则**跳过**并打印 `skip`（保护手工替换过的真美术）。
  - 写出**确定性** PNG（同一版本同参数 → 逐字节相同的输出），便于 diff。
  - 使用 §7.4 调色板，按主题给不同区域上色并标注帧号（帧号用调色板中的高对比色渲染在角落，仅占位用）。
  - 输出**必须**满足 §7.1 的四条约束。

### 7.3 输出清单

| 输出文件 | 网格（列×行） | 像素尺寸 | 帧数 | 内容 |
|---|---|---|---|---|
| `assets/sprites/player/cat_sheet.png` | `6 × 4` | `192 × 128` | 24 格（有效 14 帧，其余透明） | 行 = `idle/run/jump/fall`，帧序见 §5.2。**占位身体 = `PLAYER_SIZE`（20×28）且底对齐**：看到的就是撞到的；满 32×32 的身体会让 6 px / 4 px 的视觉外沿被误认为穿模 |
| `assets/sprites/tiles/tileset.png` | `TILESET_COLUMNS × N` | `(32·TILESET_COLUMNS) × (32·N)` | `TILESET_COLUMNS × N` | 每格带索引数字；`N` 由 `03` §3.3 的**可渲染**条目数决定（算法见 §10）。**轮廓必须表达物理语义**（`06` §6.4.5）：`#` 实心 = 满格纯色；`=` 单向平台 = 顶部 8 px 薄板（下方全透明）；`^` 尖刺 = 锯齿；装饰 = 内缩小块。历史上四类都画成满格方块，导致玩家把单向平台当成实心块、误以为"跳跃穿模" |
| `assets/sprites/decorations/<id>.png` | `1 × 1` | `32 × 32` | 1 | 每个 id 一张：**背景全透明**，只在角色画布坐标上画识别图形（居中色块 + 指纹刻度 + 索引角标）；`<id>` 为所有关卡 `reward` 字段的并集（`03` §3.2）。**不得整格不透明**：装饰品是叠加层（§6.1），全不透明会遮住角色本体 |
| `assets/sprites/ui/ui_atlas.png` | 见 §9 | 同规则 | — | UI 图标占位 |
| `assets/fonts/<font_name>.ttf` | — | — | — | **不生成**；字体为随仓库提交的静态资产 |

- `tileset.png` 的行数 `N` 与索引**语义**由 `03` 的 legend 唯一决定；生成器**读取 legend**（或由 `03` 提供的等价表）后输出，**不得**内置一套独立映射（§10）。
- 装饰品 id 清单**唯一来源是所有关卡 `.level.ron` 中 `reward` 字段的并集**（`03` §3.2 / §3.7 V11）；生成器不内置 id 列表，必须扫描 `assets/levels/` 得到。

### 7.4 占位调色板（8 色）

| # | 名称 | 十六进制 | 用途 |
|---|---|---|---|
| 0 | `void` | `#000000` | 透明占位填充（实际写 `alpha=0`） |
| 1 | `bg` | `#1a1c2c` | 底色 |
| 2 | `primary` | `#5d275d` | 主体 |
| 3 | `secondary` | `#b13e53` | 次体 |
| 4 | `accent` | `#ef7d57` | 高亮 |
| 5 | `skin` | `#ffcd75` | 角色/装饰品基色 |
| 6 | `marker` | `#38b764` | 帧号/索引标注 |
| 7 | `outline` | `#f4f4f4` | 轮廓/文字 |

- 调色板**仅供占位**使用；真美术不强制遵循。
- 所有占位 PNG 使用**纯色块**（不做抗锯齿、不做半透明渐变），保证 0/255 透明度符合 §8。

---

## 8. 导入设置

| 项 | 规格 | 依据 |
|---|---|---|
| 格式 | PNG，**8-bit / 每通道**，RGBA（带 alpha），非交错 | 本文 |
| 颜色空间 | 图像以 **sRGB** 存储；Bevy 侧纹理格式为 sRGB（`Rgba8UnormSrgb`） | 【INFERENCE】（Bevy 默认把 PNG 按 sRGB 处理；精确格式由 `07` 决定） |
| 位深 | **8 bit**（禁止 16-bit / HDR） | 本文 |
| 采样 | **全局 nearest**（`ImagePlugin::default_nearest()`） | **见 `07`**（本文不重复渲染配置） |
| 平滑/滤波 | 源图像**不得**烘焙抗锯齿/双线性；像素艺术用最近邻原图 | 本文 |
| Alpha | **直通（straight / non-premultiplied）alpha**；像素艺术通常只用 `0` 或 `255` | 【INFERENCE】（Bevy 期望 straight alpha；半透明会把颜色当预乘处理导致偏色） |
| 边缘 | 图集单元之间不得留半透明缝隙（`padding=0` 已保证）；单元边缘像素不得半透明 | 本文 §3.1 |
| 命名/大小写 | 见 §2；路径在代码中以**相对 `assets/`** 的斜杠路径书写（`sprites/player/cat_sheet.png`） | `00` §0.6 |

注意事项：

1. **不要**把 sRGB 图像当线性处理（或在导出时做 gamma 变换）；否则颜色会系统性偏亮/偏暗。
2. **不要**为像素艺术烘焙"最近邻放大"后的图；保持原始小尺寸，交给 `07` 的整数倍缩放。
3. 需要**逐资源**覆盖采样器时（例如线性缩放的 UI 图集），走 `ImageLoaderSettings`（`07` 记录写法）；本文默认全局 nearest。
4. 半透明装饰品（如玻璃、气泡）允许 `0<alpha<255`，但必须保证是**直通 alpha**；与角色合成时若出现描边渗色，改为 `alpha∈{0,255}` 或用 `SpriteAlphaMode`（见 `07` / `10`）。

---

## 9. 字体与 UI 资源

| 资源 | 路径 | 命名规则 | 说明 |
|---|---|---|---|
| UI/世界文字字体 | `assets/fonts/<font_name>.ttf` | `snake_case`，如 `pixel_sans.ttf` | 随仓库提交的静态资产；占位生成器**不**生成字体 |
| UI 图标图集 | `assets/sprites/ui/ui_atlas.png` | 固定文件名；网格固定 **`4 × 2`**（=128×64 像素），单元 `TILE_PX`，padding/offset = 0 | 8 个格位按行主序；占位生成器（§7.2）即按此输出 |
| UI 单图 | `assets/sprites/ui/<name>.png` | `snake_case` | 未打包成图集的独立图标/底图 |

> ⚠️ **在自带 TTF 入库之前，所有 UI / `Text2d` 文本只能是 ASCII。**
> Bevy 的 `default_font` 特性只内嵌 **ASCII** 字形（feature 表原文："Include a default font, containing only ASCII characters"），
> 且**遇到第一个缺字形后会把该行后续文本一起丢掉**。实测（M0，bevy 0.20.0）：
> `"animj · M0 骨架\n状态 …"` 在内层画布上只渲染出 `animj` 五个字母，`·` 之后的全部丢失 ——
> 这不是布局问题，是字体缺字形，看截图会误判成「文字节点没接线」。
> 因此：新增非 ASCII 文本前，必须先把字体文件入库并在 `TextFont.font` 里指定它。

- 字体**唯一来源单一字体文件**（避免 UI 文字在不同机器渲染不一致）；`TextFont.font` 用 `FontSource`（`Handle<Font>` 可 `into()`），字号须显式用 `FontSize::Px(..)`（0.20 默认字号变为 `FontSize::Rem(1.)`）。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.TextFont.html>、<https://docs.rs/bevy/0.20.0/bevy/prelude/enum.FontSource.html>
- UI 图片用 `ImageNode`（或 `ImageNode::from_atlas_image` 走图集），**不是** `Sprite`。【已验证】<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.ImageNode.html>
- UI 尺寸、布局、样式常量归 UI 相关文档（`05`/`07`）；本文只规定路径与文件规格。
- 字体/UI 资源同样受 §7.1 零代码改动约束：替换文件 = 同名同路径覆盖。

---

## 10. 瓦片图集与 legend 的边界

**职责划分**：

| 内容 | 所有者 |
|---|---|
| legend 字符 → `TileKind` 的映射、`tileset.png` 中每个索引代表什么瓦片、`Solid`/`OneWay`/`Hazard` 语义 | **`03-level-format.md`**（本文**只引用不重定义**） |
| `tileset.png` 的**物理布局**（网格、padding/offset、总尺寸、索引顺序） | **本文** |

`tileset.png` 物理布局规格：

| 项 | 值 |
|---|---|
| 文件 | 占位：`assets/sprites/tiles/tileset.png`；真实美术可由关卡 `tileset` 指向独立文件（当前示例 `tileset_n.png`，6 列） |
| 单元 | `TILE_PX × TILE_PX`（见 `00` §0.3） |
| 列数 | 占位固定 **`TILESET_COLUMNS = 8`**；真实美术由关卡的 `tileset_columns` 决定（当前示例为 6） |
| 行数 | `N = ceil(可渲染条目数 / 列数)`（随 `03` §3.3 中可渲染条目增长而增长；占位当前 4 条 ⇒ `N = 1`） |
| `padding` / `offset` | `0` / `0`（调用处 `None`/`None`） |
| 总尺寸 | `(32 × 8) × (32 × N)` = `256 × 32N`，宽高均可被 `TILE_PX` 整除 |
| 索引顺序 | 行主序；`tile_index = row * TILESET_COLUMNS + col` |
| 索引 → `TileKind` | **由 `03` 的 legend 唯一决定**；本文不列举任何字符/瓦片 |

调用写法（构造一次，全局复用）：

```rust
pub const TILESET_COLUMNS: u32 = 8;

fn tileset_layout(layouts: &mut Assets<TextureAtlasLayout>, rows: u32) -> Handle<TextureAtlasLayout> {
    layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(TILE_PX),   // 32×32
        TILESET_COLUMNS,         // 8
        rows,                    // N，由 03 的 legend 长度算出
        None,                    // padding = 0
        None,                    // offset  = 0
    ))
}
```

**规则**：

- 瓦片图集中**索引顺序与图集行列严格一一对应**；`03` §3.3 定义 legend 时，**表中有图集索引的条目**按其在表内的次序 == 图集行主序索引。二者**必须保持同步**：改 legend 顺序 = 必须重新导出 `tileset.png`。
- **索引 0 不是"空瓦片"**：`Air`（`.`）**不占用图集格**（不生成任何 sprite）。索引 0 是第一个**可渲染**瓦片的贴图（即 `03` §3.3 表中的 `Solid`）。
- 因此图集**行数** `N = ceil(可渲染条目数 / TILESET_COLUMNS)`。当前 `03` §3.3 有 4 个可渲染条目（`Solid`/`OneWay`/`Spike`/`Decor`），故 `N = ceil(4 / 8) = 1`，图集总尺寸 `256 × 32`。legend 增长到 9 个可渲染条目时 `N` 变 2。
- 生成器**不得**内置任何字符 → 索引的映射；必须从 `03` 的等价表取得（`crates/level_format` 的 legend 映射即为该唯一来源）。
- 本文**不**列举任何 legend 字符或 `TileKind` 名称；需要时见 `03` 或直接读 `level_format` 的类型定义。

---

## 附：本文定义的常量汇总

| 常量 | 值 | 归属 |
|---|---|---|
| `PLAYER_SHEET_COLUMNS` | `6` | 本文 §3.2 / §5.2 |
| `PLAYER_SHEET_ROWS` | `4` | 本文 §3.2 / §5.2 |
| 角色动画帧数 | `idle 4 / run 6 / jump 2 / fall 2` | 本文 §5.2 |
| 角色帧时长 | `idle 0.20 / run 0.08 / jump 0.10 / fall 0.12`（秒/帧） | 本文 §5.2 |
| `DECORATION_LAYERS` | `7`（`layer_index 0..=6`） | 本文 §6.1 |
| 装饰品贴图尺寸 | `32 × 32`（=`TILE_PX`） | 本文 §3.3 / §6 |
| `TILESET_COLUMNS` | `8` | 本文 §10 |
| 占位调色板 | 见 §7.4 | 本文 |

> 以上均为**资源格式常量**，与 `06`（物理/手感）、`07`（渲染）互不重叠。凡涉及 Z 值、`TILE_PX`、房间尺寸的数值，一律以 `00` 为准。
