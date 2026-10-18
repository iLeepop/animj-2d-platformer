# 06 — 物理与移动（自研 AABB）

> **常量所有权**：本文是**所有物理 / 手感 / 碰撞常量**的唯一所有者（见 `00` §0.2）。
> 其它文档引用本文常量时写「见 `06` §…」，**不得复制数值**。
> 本文不拥有：瓦片语义与关卡字段（`03`）、模块与调度装配（`05`）、渲染常量与像素吸附（`07`）、
> 房间拓扑与相机（`02`）、目标条件语义（`01`）、`TILE_PX` / `ROOM_PX` / 坐标转换函数（`00`）。

---

## 6.1 设计约束

### 6.1.1 三条硬约束

| # | 约束 | 来源 | 落地要求 |
|---|---|---|---|
| C1 | 固定步长 **60 Hz**，物理逻辑**只在 `FixedUpdate` 内**推进 | `00` D11 | 所有速度 / 加速度常量的单位都是 **每秒**，积分时乘 `FIXED_DT` |
| C2 | 物理内核是**纯函数**，不依赖 Bevy ECS | `00` D12 | 内核可脱离 `App` / `World` / `Window` 直接单测 |
| C3 | 位置/速度用 `f32`；渲染前才取整 | `00` §0.3、`07` | 内核不出现任何取整操作 |

### 6.1.2 固定步长的精确语义（Bevy 0.20）【已验证】

- 时钟配置：`app.insert_resource(Time::<Fixed>::from_hz(60.0))`。
  0.20 存在 `Time::<Fixed>::from_hz(hz: f64) -> Time<Fixed>`、`from_seconds`、`from_duration`、`set_timestep_hz`。
  证据：<https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Time.html>
- 物理系统加在 `FixedUpdate` schedule；`FixedUpdate` 在 `PreUpdate` 与 `Update` 之间执行，
  **一帧内可能运行 0 次、1 次或多次**（累积 `overstep` 驱动）。
  证据：<https://docs.rs/bevy/0.20.0/bevy/time/struct.Fixed.html>
- `Time<Virtual>::pause()` 会**停止** `FixedUpdate` 运行 ⇒ 暂停/恢复直接用虚拟时钟，物理侧无需额外开关。
  证据：同上（"If the virtual clock is paused, the `FixedUpdate` schedule will not run."）
- Bevy 默认固定步长是 **64 Hz（15625 µs）**，官方选择理由是「60 Hz 与显示器刷新率可能产生病态交替（一帧跑两次/零次）」。
  **本项目按 `00` D11 显式覆盖为 60 Hz**；因此本约束被重新引入：**所有物理逻辑必须与「一帧内运行几步」无关**——
  禁止在任何系统里假设「每渲染帧恰好一步」。
  证据：<https://docs.rs/bevy/0.20.0/bevy/time/struct.Fixed.html>

### 6.1.3 为什么 dt 用常量而不是读时钟

`Time<Fixed>` 的 `timestep()` 是 `Duration`，而 `Duration` 只有**纳秒整数精度**；
`1/60 s = 16 666 666.67 ns` 无法整除 ⇒ `Time<Fixed>::delta_secs() != 1.0f32 / 60.0`。
【已验证：`Duration` 精度行为】<https://docs.rs/bevy/0.20.0/bevy/time/struct.Fixed.html>（"converted to a Duration which has fixed nanosecond resolution"）
【INFERENCE：上述不等式】两项独立事实的直接推论。

**规则**：物理积分一律用常量 `FIXED_DT = 1.0 / 60.0`（`f32`），
`Time<Fixed>::delta_secs()` / `overstep_fraction()` **只允许用于诊断与日志**，禁止参与积分。
理由：常量保证同输入序列的逐位可复现（见 §6.9 测试 T17）。

### 6.1.4 纯函数内核的边界

内核 `crates/game/src/physics/core.rs`：

- **允许**依赖：`f32` 运算、`std`、`bevy::math::Vec2`（纯数学类型，来自 `glam`，无 ECS 依赖）。
- **禁止**依赖：`bevy_ecs`、`World`、`Query`、`SystemParam`、`bevy_time`、任何 `App`/`Plugin`。
- **禁止**：读时钟、读 `Transform`、用 `rand`、依赖实体遍历顺序。

签名思想（不是逐字实现要求，但接口形状是契约）：

```rust
pub struct Body { pub aabb: Aabb, pub vel: Vec2, pub grounded: bool,
                  pub airborne_since: u32, pub buffer_age: u32 }

pub struct StepInput { pub move_x: i8, pub jump_pressed: bool, pub jump_held: bool }

pub struct StepOutcome { pub jumped: bool, pub landed: bool, pub head_bumped: bool,
                         pub hit_wall_x: i32 /* -1|0|+1 */, pub walked_off: bool }

pub trait TileWorld {                    // 只读
    fn tile(&self, tx: i32, ty: i32) -> TileKind;
}

pub fn step(cfg: &PhysicsConfig, world: &dyn TileWorld, body: &mut Body,
            input: StepInput, out: &mut StepOutcome);

/// 传感器查询（Hazard / 拾取 / 区域）。
/// `expand`：**正数 = 外扩**、**负数 = 内缩**、`0 = 原样`。
pub fn overlaps_tile_kind(world: &dyn TileWorld, aabb: Aabb,
                          expand: f32, kind: TileKind) -> bool;
```

`StepOutcome` 字段语义（供适配层与测试使用）：

| 字段 | 置位时刻 | 用途 |
|---|---|---|
| `jumped` | §6.3.2 步骤 2 接受了起跳意图 | 触发跳跃音效/挤压动画（表现层，见 `07`） |
| `landed` | 本步任一次 Y 轴向下分离或落地吸附命中 | 落地音效/尘土粒子、重置 `airborne_since` 的证据 |
| `head_bumped` | 本步 Y 轴向上分离命中 | 撞头音效；可选的撞头减速（首批不做） |
| `hit_wall_x` | `+1`/`-1` 为本步水平分离的方向，`0` 为无 | 清零 `vel.x`（§6.3.2 步骤 6） |
| `walked_off` | 上一步 `grounded == true` 且本步支撑探测失败 | 触发"踏空"表现（无输入缓冲时启动土狼时间窗口的观测点在 §6.3.1） |

`step` 是**唯一**推进物理的状态转移函数；适配层只负责「组装输入 → 调 `step` → 写回组件」。

### 6.1.5 ECS 适配层

| 层 | 位置 | 职责 |
|---|---|---|
| 数据 | `crates/game/src/physics/{aabb.rs,tile.rs,core.rs}` | `Aabb`、`TileKind`、`TileWorld`、`step` |
| 常量 | `crates/game/src/config.rs` 的 `pub mod physics`（见 `00` §0.5） | §6.2 / §6.3 / §6.4 / §6.5 全部常量 + `PhysicsConfig` |
| 适配 | `crates/game/src/player/mod.rs` | `PlayerMotion(Body)` 组件、输入采样、调用 `step`、写回 |
| 传感器 | `crates/game/src/player/mod.rs` | Hazard / 拾取物 / 目标区域判定（§6.5） |
| 投影 | `crates/game/src/render/`（见 `07`） | `Aabb → Transform`（取整），单向 |

调度顺序要求（`05` 负责按此装配插件）：

| schedule | 系统（按序） | 说明 |
|---|---|---|
| `PreUpdate` | `sample_player_input` | `ButtonInput<KeyCode>` → `PlayerInput`（`00` D16），跳跃按下**锁存**（见下）。**必须 `.after(bevy::input::InputSystems)`**：`ButtonInput` 由 Bevy 的输入系统在同一 schedule 内更新，不排序会读到上一帧（表现为"按键被吞"） |
| `FixedUpdate` | `apply_player_physics` | 调 `physics::step`（.after 输入采样） |
| `FixedUpdate` | `check_hazards` | 死亡传感器（.after 物理） |
| `FixedUpdate` | `check_pickups_and_zones` | 拾取与目标区域（.after 物理） |
| `PostUpdate` | `sync_player_transform` | `Aabb → Transform` 取整；排在该帧 transform 传播系统集 `TransformSystems::Propagate` **之前**（【已验证：官方示例要求相关系统与 `TransformSystems::Propagate` 显式排序】<https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html>）|

**键位**（定义在 `player/input.rs`，`00` D16 说按键映射归 input 模块）：

| 动作 | 按键 |
|---|---|
| 左移 | `ArrowLeft` / `A` |
| 右移 | `ArrowRight` / `D` |
| 跳跃 | `Space` / `ArrowUp` / `Z` |
| 交互 / 设置存档点 | `E` |

`jump_pressed` 是**锁存**的"刚按下"：`sample_input` 在 `just_pressed` 时**置位**，由 `apply_player_physics`
在消费后**清零** ⇒ 一个按下恰好被**一个**固定步看到。**不能**每帧写 `just_pressed()`：

- `just_pressed()` 只活一帧，而 `FixedUpdate` 每帧可能跑 **0 次**（渲染帧与固定步不同频）⇒ 按下被静默吞掉
  （实机表现为"跳跃键很不灵敏、落地后要等一会"）。
- 同帧跑 2 步时，写成 `just_pressed` 会让第 2 步再次刷新跳跃缓冲（虚假输入）。

**锁存不等于按住连跳**：持续量是 `jump_held`，而"落地时仍按住"不会触发起跳（§6.3.4 边界表）。
测试：`player::input::tests::jump_press_is_latched_until_consumed`。

`interact_pressed` 与 `jump_pressed` 同一锁存语义：`E` 的 `just_pressed` 置位，由
`checkpoint::activate_checkpoint` 在一个固定步内消费并清零（`02` §2.5）。
测试：`player::input::tests::interact_press_is_latched_until_consumed`。

---

## 6.2 手感参数

### 6.2.1 先定设计意图（数值推导的输入）

| 意图 | 取值 | 理由 |
|---|---|---|
| 满跳上升高度 `h` | **2.5 瓦片** | 关卡设计上限 = 可攀 2 格台阶（留 0.5 格余量），且不足以直接跳过 3 格墙 |
| 到顶点时间 `t` | **0.40 s** | 平台跳跃的"脆"手感区间（0.3–0.45 s）；太慢发飘，太快像弹球 |
| 下落是否更快 | 是，倍率 **1.35** | 平台跳跃惯例：下落快于上升，减少"滞空感" |
| 单位换算 | `TILE_PX`（见 `00` §基础尺寸 = 32 px） | `h = 2.5 × TILE_PX = 80 px` |

### 6.2.2 推导公式（解析）

设起跳初速 `JUMP_VELOCITY = V`、上升重力 `GRAVITY = G`、顶点时刻 `t = V/G`、顶点高度 `h = V²/(2G)`。
由 `h = V·t/2` 与 `t = V/G` 反解：

$$\boxed{\;GRAVITY = \frac{2h}{t^2}\;}\qquad \boxed{\;JUMP_VELOCITY = \frac{2h}{t}\;}$$

代入 `h = 80 px`、`t = 0.40 s`：

```
GRAVITY       = 2 × 80 / 0.40² = 160 / 0.16 = 1000.0  px/s²
JUMP_VELOCITY = 2 × 80 / 0.40  = 160 / 0.40 = 400.0   px/s
校验：V²/(2G) = 160000/2000 = 80 px ✓   顶点时刻 V/G = 0.40 s = 24 个固定步 ✓
```

下落重力与终端速度：

```
FALL_GRAVITY   = GRAVITY × FALL_GRAVITY_MULT = 1000 × 1.35 = 1350.0 px/s²
t_down         = √(2h / FALL_GRAVITY) = √(160 / 1350) = 0.3443 s
总滞空         = t + t_down = 0.7443 s
达到 MAX_FALL_SPEED 所需下落距离 = V_max²/(2·FALL_GRAVITY) = 560²/2700 = 116.1 px = 3.63 瓦片
```

### 6.2.3 积分方案（**必须**按此实现，否则上表公式与实现不符）

用**精确常数加速度步（梯形法）**，而不是朴素欧拉：

```
a  = 本步净加速度 (px/s²)          // 水平见 §6.2.5；垂直（重力选段）见 §6.3.2 步骤 3
v' = v + a·dt                      // 再按需钳制（MAX_FALL_SPEED / 松手截断 / MAX_RUN_SPEED）
d  = 0.5·(v + v')·dt               // ← 关键：用旧速与新速的平均值积分位移
```

对**常加速度**，梯形法在每步上给出解析精确解，因此 §6.2.2 的 `GRAVITY = 2h/t²` / `JUMP_VELOCITY = 2h/t`
与实现**逐位一致**：

- 「先速度后位置」的朴素欧拉 → 顶点高度欠冲 `V·dt/2 = 3.33 px`（−4.2%）。
- 「先位置后速度」的朴素欧拉 → 顶点高度过冲 `+3.33 px`（+4.2%）。
- 梯形法 → 顶点高度 80.0 px（24 步；`Σ` 的代数恒等式见 §6.9 T11）。

**禁止**为水平轴单独改用其他积分次序；两轴共用 §6.2.3。

### 6.2.4 常量表（本文拥有）

单位约定：位置 px，速度 px/s，加速度 px/s²，时间 s（步数按 `FIXED_DT` 换算）。坐标系 = 世界像素，**+Y 向上**（见 `00` §0.3）。

| 常量 | 值 | 单位 | 含义 |
|---|---|---|---|
| `FIXED_HZ` | `60.0` | Hz | 固定步长频率（= `00` D11） |
| `FIXED_DT` | `1.0 / FIXED_HZ` ≈ `0.0166667` | s | 物理积分步长 |
| `JUMP_HEIGHT` | `2.5` | 瓦片（= 80 px） | 设计意图：满跳上升高度 |
| `JUMP_APEX_TIME` | `0.40` | s | 设计意图：起跳→顶点耗时 |
| `GRAVITY` | `1000.0` | px/s² | 上升段重力，`= 2·h/t²` |
| `JUMP_VELOCITY` | `400.0` | px/s | 起跳初速，`= 2·h/t` |
| `FALL_GRAVITY_MULT` | `1.35` | × | 下落重力倍率 |
| `FALL_GRAVITY` | `1350.0` | px/s² | `= GRAVITY × FALL_GRAVITY_MULT` |
| `MAX_FALL_SPEED` | `560.0` | px/s | 终端下落速度（钳制 `vy ≥ −MAX_FALL_SPEED`） |
| `MAX_RUN_SPEED` | `200.0` | px/s | 水平最大速度（水平轴的唯一上限） |
| `GROUND_ACCEL` | `1200.0` | px/s² | 地面朝输入方向的加速度 |
| `GROUND_FRICTION` | `1600.0` | px/s² | 地面无输入时的减速度 |
| `AIR_ACCEL` | `900.0` | px/s² | 空中朝输入方向的加速度 |
| `AIR_DRAG` | `400.0` | px/s² | 空中无输入时的减速度 |
| `JUMP_CUT_VELOCITY` | `280.0` | px/s | 松手后上升速度的上限（可变跳跃高度） |
| `COYOTE_STEPS` | `6` | 固定步 | 离地后可起跳的步数（= 100 ms） |
| `JUMP_BUFFER_STEPS` | `6` | 固定步 | 落地前跳跃按下的记忆步数（= 100 ms） |
| `CONTACT_TOLERANCE` | `4.0` | px | 垂直接触/落地容差 |
| `GEOM_EPS` | `0.001` | px | 分离余量 & 瓦片查询偏置（同一个常量，见 §6.4.2） |
| `SUBSTEP_MAX_DISTANCE` | `16.0` | px | 单子步最大位移（推导见 §6.4.6） |
| `SUBSTEP_HARD_CAP` | `8` | 个 | 单步最大子步数 |
| `MAX_STEP_DISPLACEMENT` | `SUBSTEP_MAX_DISTANCE × SUBSTEP_HARD_CAP = 128.0` | px | 常量表硬约束上限（§6.4.6） |
| `PLAYER_SIZE` | `(20.0, 28.0)` | px | 角色 AABB 宽 × 高 |
| `HAZARD_INSET` | `2.0` | px | Hazard 判定的角色 AABB 内缩量 |
| `PICKUP_INFLATE` | `2.0` | px | 拾取判定的角色 AABB 外扩量 |
| `PICKUP_SIZE` | `12.0` | px | 拾取物 AABB 边长 |

代码投影（`config.rs::physics`）：上表逐条写为 `pub const`；同时提供

```rust
pub struct PhysicsConfig { pub gravity: f32, pub jump_velocity: f32, /* …同表… */ }
pub const DEFAULT_PHYSICS: PhysicsConfig = /* 取上表常量 */;
```

`step()` 接收 `&PhysicsConfig`（而非直接读 `const`），使测试能构造覆盖版本（§6.9 T10、T10b）。

### 6.2.5 水平速度求解（统一公式）

```
target_vx = move_x as f32 × MAX_RUN_SPEED                  // move_x ∈ {-1, 0, +1}
a_limit   = if grounded { if move_x != 0 { GROUND_ACCEL } else { GROUND_FRICTION } }
            else        { if move_x != 0 { AIR_ACCEL    } else { AIR_DRAG } }
a_x       = clamp((target_vx − vel.x) / FIXED_DT, −a_limit, +a_limit)
```

- `move_x = 0` 时 `target_vx = 0`，`a_x` 使速度以不超过 `a_limit` 的速率趋向 0 ⇒ 摩擦/阻力由同一式子给出。
- `|target_vx − vel.x| / dt < a_limit` 时一步精确到位，**不可能超速**（因此 `MAX_RUN_SPEED` 是硬上限）。
- 撞墙清零 `vel.x` 后，只要仍按住方向键，下一步立即从 0 重新加速 ⇒ 天然"贴墙不粘滞"（§6.9 T5）。
- `grounded` 取**本步步骤 2 之后**的值：起跳当步 `grounded` 已置为 `false`，因此起跳当步就走
  `AIR_ACCEL`/`AIR_DRAG` 分支（脚已离地，符合直觉），且 `a_y = −GRAVITY` 而不是 0。

派生时间：`0 → MAX_RUN_SPEED` 需 `200/1200 = 0.167 s`（10 步）；`MAX_RUN_SPEED → 0` 需 `200/1600 = 0.125 s`（7.5 步）。

### 6.2.6 派生设计预算（供 `01` / `02` 的关卡设计引用）

| 派生量 | 公式 | 值 |
|---|---|---|
| 满跳上升高度 | `h` | 80.0 px = **2.50 瓦片** |
| 最小跳跃上升高度（解析） | `JUMP_CUT_VELOCITY² / (2·GRAVITY)` | 39.2 px = 1.22 瓦片 |
| 最小跳跃上升高度（实现最坏情况：起跳后第一个固定步边界即松手） | `0.5·(JUMP_VELOCITY+JUMP_CUT_VELOCITY)·FIXED_DT + JUMP_CUT_VELOCITY²/(2·GRAVITY)` | 5.67 + 39.2 = **44.9 px = 1.40 瓦片** |
| 总滞空时间 | `t + √(2h/FALL_GRAVITY)` | 0.744 s |
| 满跳水平位移 | `总滞空 × MAX_RUN_SPEED` | 148.9 px = **4.65 瓦片** |
| 单步最大位移 | `max(MAX_RUN_SPEED, MAX_FALL_SPEED) × FIXED_DT` | 9.333 px |
| 可攀爬最大台阶高度 | `h` 向下取整到瓦片 | **2 瓦片** |
| 可跨越最大水平缺口 | `满跳水平位移` 向下取整到瓦片，再减 0.5 安全量 | **4 瓦片** |
| 轻点必过台阶 | 最小跳跃（44.9 px）> `TILE_PX` | ✅ 1 瓦片 |
| 满跳所需净空（顶部） | `PLAYER_SIZE.y + h` | 108 px = 3.375 瓦片 |

> 结论：房间内**任何需要通过的竖直落差 ≤ 2 瓦片、水平缺口 ≤ 4 瓦片**；超过则关卡无解。
> 反例防线：跳跃高度 2.5 瓦片 < 3 瓦片 ⇒ 3 格高墙是有效阻挡。

### 6.2.7 调参指引

| 想要的效果 | 改哪个 | 方向 | 连带影响 |
|---|---|---|---|
| 跳更高 | `JUMP_HEIGHT` ↑ | — | **必须**按 §6.2.2 重算 `GRAVITY` 与 `JUMP_VELOCITY`（两者联动，不可单独改） |
| 跳更"脆"（时间更短） | `JUMP_APEX_TIME` ↓ | 同时重算 `GRAVITY`（`2h/t²`）、`JUMP_VELOCITY`（`2h/t`） | `FALL_GRAVITY` 随 `GRAVITY` 自动变 |
| 落得更快 | `FALL_GRAVITY_MULT` ↑ | 1.2–1.6 | 缩短下落段、减少滞空 |
| 下落"封顶"更早 | `MAX_FALL_SPEED` ↓ | — | 需保证 `MAX_FALL_SPEED·dt < SUBSTEP_MAX_DISTANCE`（§6.4.6） |
| 跑更快 | `MAX_RUN_SPEED` ↑ | — | 满跳水平位移按比例增加，须复核 §6.2.6 的 4 瓦片缺口预算 |
| 起步更跟手 | `GROUND_ACCEL` ↑ | — | 手感变化曲线 = `1/(GROUND_ACCEL/MAX_RUN_SPEED)` 秒到顶 |
| 松手更急停 | `GROUND_FRICTION` ↑ | — | 冰面感反向：↓ 变小 |
| 短跳更矮 | `JUMP_CUT_VELOCITY` ↓ | ≥ 1 瓦片对应速度（`√(2·G·TILE_PX)` = 253 px/s）| 必须保持最小跳跃 > 1 瓦片（§6.2.6） |
| 土狼/缓冲更宽 | `COYOTE_STEPS` / `JUMP_BUFFER_STEPS` ↑ | 建议 ≤ 12 步（200 ms） | 太宽会让"已经掉落"仍能起跳，破坏可预测性 |

**改动流程**：只改 `config.rs::physics` → 重跑 `cargo test -p game`（§6.9 全部断言）→ 更新本文表格。

---

## 6.3 宽容机制（状态机）

### 6.3.1 状态与计时器

两个状态：`Grounded` / `Airborne`（由 `Body.grounded` 派生）。两个计时器用**步数**而非秒（确定性）：

| 字段 | 类型 | 语义 |
|---|---|---|
| `airborne_since` | `u32` | `0` = 接地；`>0` = 已离地的固定步数 |
| `buffer_age` | `u32` | 距最近一次「跳跃键按下」经过的固定步数；`u32::MAX` = 已消费/无 |

派生条件（**精确无 off-by-one**）：

```
coyote_ok = airborne_since <= COYOTE_STEPS                     // 离地后第 1..6 步仍可起跳 = 100 ms
buffer_ok = buffer_age    <= JUMP_BUFFER_STEPS                 // 按下当步算第 0 步，之后 6 步内有效 = 100 ms
want_jump = buffer_ok && (grounded || coyote_ok)
```

### 6.3.2 每固定步的**精确**判定顺序

```
1) 计时器
   if grounded               { airborne_since = 0 }
   else                      { airborne_since = airborne_since.saturating_add(1) }
   if input.jump_pressed     { buffer_age = 0 }
   else                      { buffer_age = buffer_age.saturating_add(1) }

2) 起跳意图
   if want_jump {
       vel.y         = JUMP_VELOCITY;
       grounded      = false;                              // 立即离地，防止下一步重置 coyote
       airborne_since = COYOTE_STEPS + 1;                  // 土狼时间用尽 ⇒ 禁止空中二段跳
       buffer_age    = u32::MAX;                           // 缓冲已消费
       out.jumped    = true;
   }

3) 加速度（§6.2.5 水平；垂直：）
   a_y = if grounded { 0 }                                 // 接地时不积分重力
         else if vel.y > 0 { −GRAVITY }                    // 上升段
         else { −FALL_GRAVITY }                            // 顶点与下落段

4) 速度积分与钳制（§6.2.3）
   vel = vel + (a_x, a_y)·FIXED_DT
   vel.y = max(vel.y, −MAX_FALL_SPEED)
   if !input.jump_held && !grounded && vel.y > JUMP_CUT_VELOCITY { vel.y = JUMP_CUT_VELOCITY }   // 松手截断
   vel.x = clamp(vel.x, −MAX_RUN_SPEED, +MAX_RUN_SPEED)     // 防御性钳制（§6.2.5 已保证不越界）

5) 位移积分 + 子步进（§6.4）

6) 应用碰撞结果
   if out.hit_wall_x != 0 { vel.x = 0 }                    // 撞墙吸收水平速度（不粘滞，见 §6.2.5）
   支撑探测 → grounded（**仅当 vel.y <= 0**）；若 grounded && vel.y < 0 { vel.y = 0 }

7) 输出 StepOutcome（jumped / landed / head_bumped / hit_wall_x / walked_off）
```

> ⚠️ **支撑探测必须排除上升段**：`Some(top)` 只在 `vel.y <= 0` 时才算接地。
> 否则「上升穿过单向平台」时，脚底一进入 `CONTACT_TOLERANCE` 带就会被判为接地，
> 该步 `a_y` 变成 0（**丢掉一步重力**：实测竖直速度比解析值偏高 `GRAVITY·FIXED_DT` = 16.67 px/s），
> 上升被"吸住"。这条由 T9 实测发现并已在实现中落地（`physics/core.rs` 的步骤 6）。

> 顺序不可交换：**松手截断必须在位移积分之前**（否则松手当步会多上升一整步的旧速度）；
> **起跳必须在重力之前**（否则起跳当步被先扣一次重力，实际高度少 0.28 px）。

### 6.3.3 松手截断（可变跳跃高度）语义

- 判定为「松手」= `jump_held == false`（持续按住则恒为 `false` 之外）。
- 生效范围：**仅当 `vel.y > 0`（上升段）**；作用是把上升速度**上限**压到 `JUMP_CUT_VELOCITY`。
- 因为它是「钳到上限」而非「乘以系数」，**松手时机不影响同一次跳跃的最小高度**（可预测，§6.9 T12）。
- 上升高度谱：满按 80.0 px（2.50 瓦片）→ 立刻松手 44.9 px（1.40 瓦片），其余取值连续落在区间内。

### 6.3.4 宽容机制边界表

| 场景 | 期望行为 | 依据 |
|---|---|---|
| 走出平台边缘后第 6 步按跳跃 | **起跳成功** | `airborne_since = 6 <= COYOTE_STEPS` |
| 走出平台边缘后第 7 步按跳跃 | 不起跳（buffer 记住，等落地） | `airborne_since = 7 > 6` |
| 空中按跳跃（`airborne_since > COYOTE_STEPS`） | 不起跳，`buffer_age = 0` 开始计时 | — |
| 按下跳跃后第 6 步落地 | **该步起跳** | `buffer_age = 6 <= JUMP_BUFFER_STEPS` |
| 按下跳跃后第 7 步落地 | 不起跳（缓冲已过期） | `buffer_age = 7 > 6` |
| 落地时仍按住跳跃键（非新按下） | **不起跳**（`jump_pressed` 只在按下那一步为真） | 输入语义（`00` D16） |
| 起跳后立刻再次按下跳跃 | 不起跳（`airborne_since = COYOTE_STEPS+1` 已封死土狼） | §6.3.2 步骤 2 |
| 上升中撞头 | `vel.y = 0`，从顶点停止上升（不额外扣高度） | §6.4.4 |
| 落地瞬间 | `vel.y = 0` 且 `airborne_since = 0` | §6.4.4 |
| 撞头/撞墙时按下的跳跃 | 缓冲照常保留，落地后生效 | 缓冲只受步数与消费影响 |

---

## 6.4 AABB 碰撞算法

### 6.4.1 每固定步流程

```
A. 取 d = 0.5·(vel_old + vel_new)·FIXED_DT                // 本步总位移（第 4 步已算好速度）
B. n = max(1, ceil( max(|d.x|, |d.y|) / SUBSTEP_MAX_DISTANCE ))
   debug_assert!(n <= SUBSTEP_HARD_CAP);                   // 由 §6.4.6 的常量约束保证
C. for _ in 0..n {
       let s = d / n as f32;                               // 每个子步做一次完整的两轴求解
       if s.x != 0.0 { move_x(&mut aabb, s.x, world, &mut out); }
       if s.y != 0.0 { move_y(&mut aabb, s.y, world, prev_foot, &mut out); }
   }
D. support_probe(&aabb, world) → grounded                  // = §6.3.2 步骤 6 的支撑探测，实现见 §6.4.4
```

`prev_foot` 在每个子步开始前刷新为 `aabb.min.y`（单向平台需要它）。

### 6.4.2 瓦片范围查询（唯一入口）

世界瓦片坐标：`tx = floor(x / TILE_PX)`、`ty = floor(y / TILE_PX)`（原点 = 地图左下角，+Y 上，与 `00` §0.3 一致）。

```
tx0 = floor(min.x / TILE_PX)
tx1 = floor((max.x − GEOM_EPS) / TILE_PX)
ty0 = floor(min.y / TILE_PX)
ty1 = floor((max.y − GEOM_EPS) / TILE_PX)
```

- 减 `GEOM_EPS` 的目的：AABB 与瓦片面**精确贴合**时（分离器的结果），贴合面上的那格**不算重叠**。
  这就是「零 skin」策略：不需要给 AABB 加 0.01 px 的偏移量，像素风下取整后的渲染位置与碰撞面完全对齐。
- 迭代顺序：`ty` 外层、`tx` 内层，**遍历全部重叠格**再取极值（不能"命中即返回"，否则 2 格高的墙会漏解）。
- 角色单次查询覆盖格数 ≤ `(⌈20/32⌉+1) × (⌈28/32⌉+1) = 2 × 2 = 4` 格（`PLAYER_SIZE` 见 §6.2.4）。

### 6.4.3 X 轴分离（先解 X）

```
fn move_x(aabb, dx, world, out) {
    let prev = aabb;
    let mut new = aabb.translate(vec2(dx, 0));
    let (tx0,tx1,ty0,ty1) = tile_range(new);
    let mut limit: Option<f32> = None;
    for ty in ty0..=ty1 { for tx in tx0..=tx1 {
        if !world.tile(tx, ty).is_solid() { continue; }
        let face = if dx > 0.0 { tx as f32 * TILE_PX } else { (tx+1) as f32 * TILE_PX };
        // ★ 只认**前方**的面：身体已经越过的（重叠态）不参与夹取
        let ahead = if dx > 0.0 { face >= prev.max.x - GEOM_EPS }
                    else        { face <= prev.min.x + GEOM_EPS };
        if !ahead { continue; }
        limit = Some(if dx > 0.0 { min_by(limit, face) } else { max_by(limit, face) });
    }}
    if let Some(l) = limit {
        // ★ 尺寸不变量：只夹行进侧，另一侧按 player_size 反推（整体推出，而不是压扁）
        if dx > 0.0 { new.max.x = l; new.min.x = l - player_size.x; }
        else        { new.min.x = l; new.max.x = l + player_size.x; }
        out.hit_wall_x = if dx > 0.0 { +1 } else { -1 };    // §6.3.2 步骤 6 据此清零 vel.x
    }
    aabb = new;
}
```

**为什么先解 X**：Y 轴决定落地与撞头（手感关键），先解 X 使 Y 的瓦片查询基于**最终**水平位置，
避免斜向移动时在墙角被水平重叠"顶"到错误高度（角落吸附 / 抖动的经典来源）。

**为什么有两处 ★**（实机 bug 回归，`§6.4.9`）：若夹取只改 `max.x` / `min.x` 一侧，而身体**已经**
与某格重叠（典型：重生点让脚底埋进地板），那个格子会用自己的面把盒子**压扁甚至压反**；
反向盒子的 `tile_range` 返回空区间 ⇒ 碰撞静默关闭。

### 6.4.4 Y 轴分离 + 落地吸附

```
fn move_y(aabb, dy, world, prev_foot, out) {
    let mut new = aabb.translate(vec2(0, dy));
    let mut resolved = false;
    for ty in ty0..=ty1 { for tx in tx0..=tx1 {
        match world.tile(tx, ty) {
            Solid => {
                let top = (ty+1) as f32 * TILE_PX;  let bottom = ty as f32 * TILE_PX;
                // ★ 尺寸不变量：夹行进侧，另一侧反推（§6.4.9）
                if dy < 0.0 { new.min.y = top; new.max.y = top + player_size.y; out.landed = true; }
                else        { new.max.y = bottom; new.min.y = bottom - player_size.y; out.head_bumped = true; }
                resolved = true;
            }
            OneWay => { /* 见 §6.4.5：命中则 new.min.y = top、out.landed = true、resolved = true；
                           不满足三条件时完全忽略 */ }
            _ => {}
        }
    }}
    // 落地吸附（CONTACT_TOLERANCE 的正业）：本子步没有任何分离时，检查脚底是否已在容差带内
    if !resolved {                                                            // 进入本分支即 dy < 0（dy == 0 不调用 move_y）
        if let Some(top) = nearest_standable_top_below(new, world) {
            new.min.y = top; new.max.y = top + player_size.y; out.landed = true;
        }
    }
    aabb = new;
}

/// 返回脚底下方容差带内、最高的可站立（Solid / OneWay）瓦片顶面；不存在返回 None。
fn nearest_standable_top_below(aabb, world) -> Option<f32> {
    // 探针带（两侧都收紧 GEOM_EPS，避免把紧贴的侧墙当成支撑）：
    //   x ∈ [aabb.min.x + GEOM_EPS, aabb.max.x − GEOM_EPS]
    //   y ∈ [aabb.min.y − CONTACT_TOLERANCE, aabb.min.y]            // 距离 ∈ [0, CONTACT_TOLERANCE]
    //   ⇒ 瓦片行 ty ∈ [floor((min.y − CONTACT_TOLERANCE) / TILE_PX), floor((min.y − GEOM_EPS) / TILE_PX)]
    // 对每行的可站立瓦片取 top = (ty + 1) as f32 * TILE_PX，过滤 top ≤ aabb.min.y + GEOM_EPS
    // （排除"已穿透当前行"的情形），答案为剩下的 top 之最大值。
}
```

`support_probe`（§6.4.1 步骤 D）就是同一函数：

```
match (nearest_standable_top_below(aabb, world), vel.y) {
    (Some(top), vy) if vy <= 0.0 => {                                       // 吸附到顶面：零 skin，面精确贴合
        aabb.min.y = top; aabb.max.y = top + player_size.y; grounded = true; // ★ 保持尺寸（§6.4.9）
    }
    _                            => { grounded = false; }                   // 含"在上升"⇒ 一律不接地
}
```

**为什么必须带 `vel.y <= 0`**：探测本身是纯几何的，不知道运动方向。上升穿过单向平台时脚底会短暂落在
平台顶面下方 `CONTACT_TOLERANCE` 之内，若不加这个条件就会把"正在上升"判成"站在平台上"，
当步 `a_y = 0`（丢掉一步重力），跳跃高度被抬高且上升被"吸住"。见 T9 与 §6.3.2 步骤 6 的说明。

之所以用**同一条容差带**做「落地吸附」与「站立探测」：两者必须给出同一个答案，否则会出现
「落地当步判定为 grounded、下一当步又判定为 airborne」的两拍抖动（会连带污染土狼时间与输入缓冲）。

`CONTACT_TOLERANCE` 的三项职责（这是本文对「落地穿透容差」的精确定义）：

1. **吸掉浮点缝隙**：脚底与平台顶面距离在 `[0, 4] px` 内一律视为接触并吸附到顶面 ⇒ 站立状态不会在
   grounded / airborne 之间抖动。
2. **落地宽容**：高速度下落时，若某子步的分离结果让脚底停在面外 ≤ 4 px，仍判定为"成功落地"。
3. **站立稳定性**：`support_probe` 用的下探距离就是这个值，因此浮点误差不会把站立的角色判为悬空。

真实穿透（AABB 与 Solid 重叠）**无论多深都由分离器直接推出到面贴合**，不依赖容差；
穿透深度的硬上限由 §6.4.6 的子步进几何保证（= `SUBSTEP_MAX_DISTANCE`）。

### 6.4.5 单向平台（OneWay）

> **设计用途（`01` §1.8）**：单向平台是**隐藏通道**元素，数量应远少于实心块（`#`）。
> 普通地面/平台一律用 `#`（四面阻挡）；`=` 只出现在"从下方跳上去发现秘道"的少数点位。

**可站条件（三条同时满足）**：

```
1. dy < 0                              // 本子步向下移动（下落中/站在上面时被判定为接触）
2. prev_foot >= tile_top − GEOM_EPS    // 移动前脚底在平台顶面之上（含贴合）
3. new.min.y <  tile_top               // 移动后脚底会进入平台

动作：new.min.y = tile_top; out.landed = true;
```

| 情形 | 结果 | 原因 |
|---|---|---|
| 从平台上方下落 | **站住** | 三条件满足 |
| 站在平台上（`dy = 0`） | 由 `support_probe` 判定接触 | 条件 1 不要求 `move_y`；支撑探测把 OneWay 计入可站类别 |
| 从平台下方跳起（`dy > 0`） | **穿过，不阻挡** | 条件 1 不满足 |
| 上升穿过平台顶面的一瞬间 | 穿过 | 那一刻 `dy > 0` |
| 上升到顶面之上后开始下落 | **站住** | `prev_foot` 已 ≥ 顶面 |
| 水平跑动经过平台边缘 | 不阻挡（X 轴分离只看 `Solid`） | §6.4.3 |
| 在平台下面贴着走 | 不阻挡 | 同上 |

**明确不支持「按住 ↓ 主动下穿」**：需求未提；引入后 `prev_foot` 的语义要按"是否处于下穿状态"分支，
并与土狼时间、单向平台站立状态互相干扰，收益极低。

### 6.4.6 子步进推导

目标不变量：**单子步位移 < 最小实体厚度**。最小实体厚度 = 1 瓦片 = `TILE_PX`（见 `00`）= 32 px。

```
(1) 单步最大位移
    D_max = max(MAX_RUN_SPEED, MAX_FALL_SPEED) × FIXED_DT
          = max(200, 560) / 60
          = 9.333 px

(2) 安全系数 2（几何余量：即使边界恰好对齐也不越格）
    SUBSTEP_MAX_DISTANCE = TILE_PX / 2 = 16.0 px
    硬约束：TILE_PX / 2 < TILE_PX  ⇔  16 < 32 ✓

(3) 子步数
    n = max(1, ceil(D_max / SUBSTEP_MAX_DISTANCE)) = ceil(9.333 / 16) = 1

(4) 常量表硬约束（改动任一速度常量后必须复核）
    D_max ≤ SUBSTEP_MAX_DISTANCE × SUBSTEP_HARD_CAP = 16 × 8 = 128 px
    ⇔ 任一轴速度 ≤ 128 × 60 = 7680 px/s
```

结论：**当前常量下 `n ≡ 1`**，子步进是安全网而非常态开销（§6.9 T10 用覆盖配置强制 `n > 1` 验证正确性）。
水平方向的额外位移来源只有移动平台承载（首批不实现，见 §6.7）；若未来加入，必须把承载位移并入 `D_max` 复核 (4)。

### 6.4.7 明确不支持：斜坡

**不支持。** 理由：

1. **需求未提**：`00` D2/D3 只要求瓦片网格 AABB；`01` 的首批目标类型是 `ReachExit` + `Collect`，无斜坡需求。
2. **成本显著抬高**：瓦片网格斜坡需要额外的斜面几何数据（每个瓦片内的三角/高度函数）、
   通用的"推离斜面法线"分离求解、沿坡滑动的受力模型、以及坡上接地判定与土狼时间的交互规则——
   这套东西的复杂度**超过本文其余全部内容之和**，而本文的轴分离算法会整体失效（轴分离对斜面本就不成立）。
3. **美术风格不需要**：角色是小动物像素形象，阶梯化的台阶观感更贴合；网格地形用「阶梯化」表达的斜坡在
   32 px 瓦片下依然可读。
4. **替代方案**：需要"缓坡"手感时，用**阶梯化斜坡**（连续 1 格高的小台阶）；`6.2.6` 保证 1 格台阶轻点即过，
   且 2 格可攀，因此阶梯化斜坡在现有手感参数下完全通行。

### 6.4.8 复杂度预算

单固定步、单角色：

| 项 | 次数 |
|---|---|
| 瓦片范围查询 | ≤ 4 格/次 |
| 轴分离 | 2 轴 × `n` 子步 = 2 次 |
| 支撑探测 | 1 次（≤ 6 格） |
| 传感器查询 | 3 次（Hazard / 拾取 / 区域，各 ≤ 4 格） |
| **合计** | ≤ 约 30 次瓦片读取 / 步，60 Hz 下 ≈ 1800 次/s（无分配） |

`step()` 与全部查询函数**零堆分配**（`Aabb`/`Vec2` 皆 `Copy`，无 `Vec`）。

---

### 6.4.9 尺寸不变量（AABB 永不压扁 / 反向）

**规则**：`Body.aabb.size()` 必须恒等于 `player_size`，且 `min < max`（两轴）。这是硬约束，
不是风格问题——违反后**不会有任何报错**，只会静默失去碰撞：

| 变形 | 直接后果 |
|---|---|
| `max.x < min.x` 或 `max.y < min.y` | `tile_range()` 的 `tx0 > tx1` ⇒ 范围为空 ⇒ **一张瓦片都不查**：穿地板、穿墙、掉出相机 |
| 高度被压到 14（旧 bug） | 只有 sprite 下半身参与碰撞，"半格没碰撞"的直观感受 |
| 宽度被夹成负数 | 地图外框墙也拦不住 ⇒ 贴边"慢慢卡出去" |

**保证手段**（三层，任一被动都不该发生）：

1. **分离时保持尺寸**（§6.4.3 / §6.4.4）：只夹行进侧，另一侧按 `player_size` 反推。
2. **只认前方的面**（§6.4.3）：已经与身体重叠的格子不参与本次夹取（避免把重叠解成反向）。
3. **步首自检修复**（`step` 步骤 0）：发现尺寸不对或反向时，以**中心**重建为 `player_size` 并继续。

**配套**：重生点/关卡 `spawn` 的值是 **AABB 中心**，不是脚底（`02` §2.5）。脚底贴地板的中心
`= 地板顶面 − player_size.y/2`；写成地板顶面本身就会踩中上表的第 2 行。

## 6.5 碰撞体与图层

### 6.5.1 角色 AABB

| 项 | 值 / 约定 |
|---|---|
| 尺寸 | `PLAYER_SIZE = (20, 28)` px（见 §6.2.4） |
| 表示 | `Aabb { min: Vec2, max: Vec2 }`，世界像素 |
| 关卡文件 `at` | 该 AABB 的**中心**（房间局部坐标，+Y 向下），见 `00` §0.3 |
| 初始化 | `center = room_local_to_world(col, row, grid_rows, at)`（`00` 的唯一转换函数）；`aabb = Aabb::from_center_size(center, PLAYER_SIZE)` |
| 水平余量 | 20 px 宽 ⇒ 1 瓦片宽（32 px）的竖直通道两侧各余 6 px（不卡通道） |
| 竖直余量 | 28 px 高 ⇒ 1 瓦片高通道余 4 px |
| Sprite 对齐 | sprite 32×32 + `Anchor::BOTTOM_CENTER`（【已验证】`bevy::sprite::Anchor::BOTTOM_CENTER` 存在，<https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html>）；渲染位置 = `AABB` 底边中点取整（§6.8） |
| 尺寸变化的允许范围 | 若未来改 `PLAYER_SIZE`，必须重跑 §6.9 全部断言 |

### 6.5.2 瓦片语义 → 交互矩阵

瓦片语义 `TileKind`（取值与图集索引映射见 `03`）：`Empty` / `Solid` / `OneWay` / `Hazard`。

| `TileKind` | X 轴分离 | Y 轴分离 | 可站立 | 传感器触发 |
|---|---|---|---|---|
| `Empty` | 忽略 | 忽略 | 否 | 否 |
| `Solid` | **阻挡** | **阻挡** | **是** | 否 |
| `OneWay` | 忽略 | 仅 §6.4.5 三条件成立时阻挡 | 是 | 否 |
| `Hazard` | 忽略 | 忽略 | 否 | **是**（§6.5.3） |

### 6.5.3 Hazard（传感器 → 死亡/重置）

- **不阻挡**：`Hazard` 不参与任何轴分离。
- **判定**：把角色 AABB **内缩** `HAZARD_INSET`（= 2 px，四边各内缩）后，与 `Hazard` 瓦片矩形求相交；
  任一相交即触发 ⇒ 即 `overlaps_tile_kind(world, aabb, −HAZARD_INSET, Hazard)`。
  内缩理由：角色擦到尖刺贴图的最外一列像素不应致死，减少"擦边即死"的挫败感。
- **时机**：`FixedUpdate` 的 `check_hazards`，物理步**之后**执行（用本步最终 AABB）。
- **输出**：写一条 buffered 消息 `PlayerDied`（0.20 的 buffered 消息类型是 `Message`，
  **不是** `Event`；`MessageWriter<T>::write(..)`【已验证】<https://docs.rs/bevy/0.20.0/bevy/ecs/message/index.html>）。
- **同一固定步只写一次**：`check_hazards` 内部按实体去重（单角色场景天然满足）。
- **重置行为**（把角色放回**已激活的存档点**；没有存档点时回退到**关卡出生点**；清速度、清计时器）
  由 `player::death` 在收到 `PlayerDied` 后执行（`02` §2.5）；
  物理侧只负责"发出信号" + 提供一个干净的 `Body` 构造函数：

```
Body::at_center(c) = Body {
    aabb: Aabb::from_center_size(c, PLAYER_SIZE),
    vel: Vec2::ZERO,
    grounded: false,
    airborne_since: COYOTE_STEPS + 1,   // 土狼时间「用尽」：出生/重置后不得凭空获得一次空中起跳
    buffer_age: u32::MAX,               // 无待生效的跳跃缓冲
}
```

### 6.5.4 可拾取物（果实）

| 项 | 约定 |
|---|---|
| 数据 | 关卡实体列表中的一项（字段见 `03`），`at` = AABB 中心（房间局部坐标） |
| AABB | `Aabb::from_center_size(world_center, Vec2::splat(PICKUP_SIZE))`，`PICKUP_SIZE = 12` px |
| 判定 | 角色 AABB **外扩** `PICKUP_INFLATE`（= 2 px）后与拾取物 AABB 相交 ⇒ 已拾取（即 `overlaps_tile_kind(.., +PICKUP_INFLATE, ..)` 的正向外扩语义） |
| 时机 | `FixedUpdate` 的 `check_pickups_and_zones`，物理步之后 |
| 输出 | `MessageWriter<CollectiblePicked>`（消息名以 `05` 的命名为准），随后销毁该实体 |
| 为什么外扩而 Hazard 内缩 | 拾取要"手感慷慨"（差一点也算吃到）；危险要"手感宽容"（擦边不算死） |

### 6.5.5 目标区域（`ReachExit` 及通用区域）

| 项 | 约定 |
|---|---|
| 表示 | 矩形区域（`at` = 中心、`size` = 宽高，字段见 `03`）；`ReachExit` 是其中一种用法（语义见 `01`） |
| 判定 | 角色 AABB 与区域 AABB **相交**（不做内缩/外扩） |
| 附加条件 | 首批**无**附加条件（不要求 grounded）；`01` 若要新增条件（如"必须站在地面"），由 `01` 定义条件语义、由 `05` 装配判定系统，物理侧不再新增常量 |
| 时机 | `FixedUpdate` 的 `check_pickups_and_zones`，物理步之后 |
| 幂等 | 区域判定每步都可能为真，**由"是否已达成"的状态位去重**（状态位归 `01`/`02`），不由物理侧处理 |

---

## 6.6 瓦片查询接口

### 6.6.1 接口（只读）

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileKind { Empty, Solid, OneWay, Hazard }

impl TileKind {
    pub fn is_solid(self) -> bool;          // == Solid
    pub fn is_standable(self) -> bool;      // == Solid || OneWay
}

/// 只读瓦片视图；由 ECS 适配层实现（把房间数据包成这个接口）
pub trait TileWorld {
    /// 全局瓦片坐标（原点 = 地图左下角，+Y 上）处的语义。
    fn tile(&self, tx: i32, ty: i32) -> TileKind;

    /// 矩形范围内是否存在指定类别。默认朴素实现即可（≤4 格），
    /// 但允许房间实现做"边界预裁剪"优化。
    fn any_kind_in(&self, tx0: i32, ty0: i32, tx1: i32, ty1: i32, kind: TileKind) -> bool {
        (ty0..=ty1).any(|ty| (tx0..=tx1).any(|tx| self.tile(tx, ty) == kind))
    }
}
```

设计要点：

- **只有 `tile(tx, ty)` 是必须实现的**；其余方法都有默认实现 ⇒ 测试用 `TestWorld`（`Vec<TileKind>` + 宽高）只需实现一个方法。
- 接口**不含**任何 `&mut`、无 `Commands`、无 `Query` ⇒ 可在纯单测里构造，满足 C2。
- `tiles_in(aabb)` 这种"返回迭代器"的写法**不进入接口**：会造成 `impl Trait` 与借用复杂度，
  而调用方只需要"范围 + 类别存在性"这一件事（`any_kind_in`），收益为零。

### 6.6.2 房间激活范围

- 同时处于激活态（瓦片数据可用）的房间：**当前房间 + 与当前房间共享一条边的相邻房间**（最多 5 个）。
  理由：角色 AABB 最大仅 28 px，跨边界后物理最多在相邻房间内行动；再多加载无意义。
- 激活范围的**判定与生命周期归 `02`**；物理侧只要求：`TileWorld::tile` 在任何时刻对被激活房间返回真实数据。

### 6.6.3 跨房间 / 越界行为

| 查询位置 | 返回 | 理由 |
|---|---|---|
| 当前房间内 | 真实瓦片 | — |
| 落在相邻已激活房间 | **映射到该房间的局部瓦片后返回真实瓦片**（映射公式见 §6.6.4） | 房间无缝衔接（`00` D4）；若返回 `Empty`，角色会在门口踏空 |
| 落在 `grid` 范围之外 | **`Solid`** | 地图外必须是硬墙，防止角色走出地图后无限下落 |
| 落在 `grid` 内但未被激活的房间 | **`Solid`** 且 `debug` 下 `warn!` | 该情形不应发生（未激活房间与当前房间至少隔 1 个房间，而后者边界为 `Solid`）；一旦发生说明房间激活逻辑有 bug，保守返回 `Solid` 以免穿模 |
| 门洞 | 由 `03` 的数据决定（门洞位置瓦片为 `Empty`） | 物理不特殊处理房间边界；边界墙就是普通 `Solid` 瓦片 |

### 6.6.4 坐标转换（唯一路径）

物理内部只用**全局瓦片坐标**；需要读房间数据时才转换：

```
col   = tx.div_euclid(ROOM_TILES)
lty   = ty.rem_euclid(ROOM_TILES)
lrow  = ty.div_euclid(ROOM_TILES)                        // 从地图底部数的房间行
row   = grid_rows - 1 - lrow
ltx   = tx.rem_euclid(ROOM_TILES)
file_row = ROOM_TILES - 1 - lty                          // 关卡文件行号（文件首行 = 房间最上一行）
```

**禁止**在物理代码里写像素级的房间算术——像素级转换一律调用 `00` 的
`room_local_to_world` / `world_to_room`；上面的整数公式只是它在瓦片粒度上的等价形式（`00` §0.3 已固定符号约定）。

---

## 6.7 移动平台

### 6.7.1 结论：**首批不实现**

理由（按权重排序）：

1. **需求缺席**：`00` 的 D1–D17 与 `01` 的玩法范围（`ReachExit` + `Collect`）均未要求移动平台；
   §6.2.6 的静态跳跃预算已足以表达全部关卡意图（可攀 2 格、可跨 4 格缺口）。
2. **承载语义会引入一整类难以测试的时序 bug**：位置差传递、单向平台上的传递、玩家被夹在平台与墙之间
   的挤压处理、以及"平台先动 → 再搬运玩家 → 再解玩家自身碰撞"的严格顺序。这些是物理侧最容易出
   隐藏 bug 的地方，而单测成本高（需要多体时序用例）。
3. **数据格式尚未预留**（`03` 首批无移动平台字段）⇒ 首批实现会同时改动 `03`、`01`、物理三处，超出本文范围。
4. **不做接口预留**（YAGNI）：`TileWorld` 首批**不提供**平台查询方法；未来实现时按下节签名扩展，
   代价是一次性小改，而非现在为假想需求维护一个空实现。

### 6.7.2 若未来实现：承载算法（位置差传递）

以下为**完整算法规范**，供未来决策时直接落地（首批不在实现清单内）。

新增数据：

```rust
pub struct MovingPlatform { pub aabb: Aabb, pub vel: Vec2, pub prev_aabb: Aabb }
// TileWorld 扩展：
fn platforms(&self) -> &[MovingPlatform];
// Body 扩展：
pub struct Body { /* … */ pub ground_platform: Option<usize> }
```

每固定步的顺序（**不可重排**）：

```
1. 平台推进：prev_aabb = aabb; aabb = aabb.translate(vel · FIXED_DT)
2. 搬运：对每个 body，若 body.ground_platform == Some(i) {
       let delta = platforms[i].aabb.min − platforms[i].prev_aabb.min;    // 位置差，含竖直与水平分量
       body.aabb = body.aabb.translate(delta);
       body.vel  += platforms[i].vel;                                     // 可选：把平台速度并入玩家速度（"甩出去"手感）
   }
3. 解碰撞（本文 §6.4 的两轴分离）：把 body 从被推入的墙里推出来。
   若无法分离（推向墙时被墙挡住 ⇒ 挤压）：
       放弃本步搬运（body.aabb 回退到搬运前的位置），平台穿过 body；
       debug 下 warn!，不触发死亡（挤压不是玩法机制）。
4. 随后才是玩家自身的速度积分与位移（§6.4.1）。
5. 支撑探测刷新 body.ground_platform（脚下最近的非空瓦片或平台）。
```

新增常量（未来实现时才在 §6.2.4 登记，属于本文所有权）：

| 常量 | 建议值 | 含义 |
|---|---|---|
| `PLATFORM_CARRY_SPEED_LIMIT` | `−1`（无限制） | 平台速度并入玩家速度的上限；`−1` = 不限制 |
| `CRUSH_ESCAPE_EPS` | `GEOM_EPS` | 挤压脱离的判定余量 |

同时 §6.4.6 的 `D_max` 必须并入 `|platform.vel|·FIXED_DT` 后复核。

---

## 6.8 浮点与像素的边界

### 6.8.1 单向数据流（本文的核心不变式）

```
PlayerInput ──► physics::step ──► Body { aabb, vel, … } ──►（单向投影）──► Transform
                     ▲                                                        │
                     └──────────── 禁止任何反向读取 ◄──────────────────────────┘
```

- **逻辑真相是 `Aabb`，不是 `Transform`**（`00` §0.3 已定契约）。碰撞、传感器、区域判定一律读 `Aabb`。
- `Transform` 是 `Aabb` 的**渲染投影**，每个 `PostUpdate` 重算；任何系统读 `Transform` 去改物理状态都是违规。
- 因此浮点误差**不会**通过 `Transform` 回流污染物理；`Transform` 取整也不影响物理。

### 6.8.2 渲染投影的取整

```
render_pos = (aabb.center().x.round(), aabb.min.y.round())     // f32::round，std
transform.translation = (render_pos.x, render_pos.y, Z)        // Z 由 07 给定
```

- **位置语义 = `AABB` 底边中点（脚底中点）**，配合全局统一的 `Anchor::BOTTOM_CENTER`（`04` §4）。
  这样 sprite 的视觉脚底与碰撞体底边严格重合，玩家能从画面上准确读出落脚点。
- 取整用 `f32::round()`（标准库），**不**对 `Aabb` 本体取整。
- 取整后的像素位移序列（如 3.33 px/步 → 3/4/3 px 交替）是像素风游戏的正常表现，**不做子像素补偿**。
- **不做**固定步长的渲染插值（官方 `physics_in_fixed_timestep` 示例的 `overstep_fraction` 插值）。
  理由：像素完美要求整像素吸附（`07`），插值出的亚像素位置最终仍要取整，收益仅在极少数帧出现，
  却增加一个跨模块状态。`overstep_fraction()` 只用于诊断。
- 取整只发生在 `07` 拥有的 `sync_player_transform` 里；物理模块内**不存在** `round`/`floor` 的位置写入。

### 6.8.3 浮点规则

| 规则 | 说明 |
|---|---|
| 全部物理量用 `f32` | 位置、速度、加速度、容差 |
| 不使用 `f64` | 世界坐标量级 ≤ 10⁴ px，`f32` 精度（2⁻²³ × 10⁴ ≈ 0.001 px）远优于 `GEOM_EPS` |
| 比较一律带容差 | 禁止 `aabb.min.y == tile_top` 这类精确比较；用 `abs(..) <= GEOM_EPS` |
| 不依赖 NaN 传播 | 速度钳制（§6.3.2 步骤 4）是唯一防线；`debug_assert!(vel.is_finite())` |
| 不用 `EPSILON` 魔数 | 只用 `GEOM_EPS` 与 `CONTACT_TOLERANCE` 两个常量 |

---

## 6.9 测试清单

**全部可在无 Bevy 的情况下单测**：`crates/game/src/physics/tests.rs`，用
`TestWorld { grid: Vec<TileKind>, cols: i32, rows: i32 }` 实现 `TileWorld`（只需 `tile()`），
`PhysicsConfig` 用 `DEFAULT_PHYSICS` 或覆盖版本。运行方式：`cargo test -p game`（不启动 `App`、不需要窗口）。
每条断言都是**不变量**，可直接写成 `assert!` / `assert!(approx_eq!(..))`。

### R1–R5 尺寸不变量（实机 bug 回归，`§6.4.9`）

- **R1** 身体中心处在地板厚度中间（旧默认重生点）：每步后 `aabb.size()` 必须仍等于 `player_size`，
  且 `min < max`；最终 `aabb.min.y == 地板顶面` 且 `grounded`。
- **R2** 同一埋入状态下 **按住右移**：每步尺寸不变（旧实现宽度被压成负数）。
- **R3** 出生在墙体内部、向右推：每步尺寸不变（分离必须整体推出，不得反向）。
- **R4** **出厂关卡的每个房间**：用该房间的重生点放置角色、静止 180 步后必须 `grounded`，
  碰撞盒不变形，且重生点不与 `Hazard` 重叠（否则死亡→重生→再死的循环）。
- **R5** 贴地图左边界持续左推 600 步：`min.x >= 0`，每步尺寸不变（旧实现会"慢慢卡出去"）。
- **R6** 物理网格与坐标/渲染网格指向同一格：对出厂关卡逐瓦片比对
  `tile_at_world(中心) == tile(floor(world/TILE))` 与 `RoomTiles::get`，三条路径必须一致
  （防"视觉站在砖上、实际踩空"的半格错位）。

### H1–H2 向上碰撞（实机"跳跃穿模"排查补上的用例）

- **H1** 朝单层**实心**天花板以 `JUMP_VELOCITY` 上升：每一固定步 `aabb.max.y <= 天花板底面`，
  且至少一次 `head_bumped == true`；撞头后 `vel.y <= 0`（`§6.3.4` 边界表的"撞头停止上升"）。
- **H2** 同高度换成**单向平台**：上升期脚底可以越过平台顶面（穿过去），下落时停在顶面并 `grounded`。
  ⇒ "跳跃时穿过网格地块"在 `=` 上是**设计行为**；`#` 上由 H1 保证不可能。

### T1 垂直不穿透
以 `vy = −MAX_FALL_SPEED`、起始位置距单层 1 瓦片厚 `Solid` 地面 200 px 下落 60 步：
**每一步后** `aabb` 与任何 `Solid` 瓦片的重叠深度（`min(overlap_x, overlap_y)`）都必须 `<= GEOM_EPS`，且最终 `aabb.min.y == tile_top`（容差 `GEOM_EPS`）。

### T2 水平不穿透
以 `vx = +MAX_RUN_SPEED` 朝单格宽的 `Solid` 墙移动 30 步：**每一步后** `aabb.max.x <= wall_min_x + GEOM_EPS`，且撞墙后 `vel.x == 0`。

### T3 对角不穿透（角落）
以 `v = (MAX_RUN_SPEED, −MAX_FALL_SPEED)` 朝单瓦片平台的上右角运动 30 步：任何时候 `aabb` 与 `Solid` 的重叠深度 `<= GEOM_EPS`，且最终 `aabb.min.y == platform_top ∨ aabb.max.x == platform_left`。

### T4 静止时不下陷
角色站在 `Solid` 顶面上、输入全零，连续 600 步：每一步 `abs(aabb.min.y − tile_top) <= GEOM_EPS` 且 `grounded == true`、`vel == Vec2::ZERO`。

### T5 撞墙不粘滞
角色贴墙下落、每步 `move_x = +1`，连续 60 步：`vel.x == 0` 恒成立（被墙钳制），同时 `vel.y` 单调趋向 `−MAX_FALL_SPEED` 并在第 `ceil(MAX_FALL_SPEED/(FALL_GRAVITY·FIXED_DT))` 步内到达；把墙移除后一步内 `vel.x > 0`（重新加速，证明未被"粘住"）。

### T6 coyote 边界
角色水平走出平台边缘，`airborne_since` 记步：在第 `COYOTE_STEPS`（= 6）步按下跳跃 ⇒ `out.jumped == true` 且 `vel.y == JUMP_VELOCITY`；在第 `COYOTE_STEPS + 1`（= 7）步按下 ⇒ `out.jumped == false`。

### T7 jump buffer 边界

**精确语义**：落地只能在**位移之后**被观测到（§6.3.2 的顺序不可交换），因此缓冲起跳落在**落地后的下一步**。
即「在土狼窗口之外按下跳跃，并在其后的第 `k` 步落地」时：

- `k <= JUMP_BUFFER_STEPS - 1`（= 5）⇒ **起跳**，且起跳发生在落地后那一步（= 按下后第 6 步）；
- `k >= JUMP_BUFFER_STEPS`（= 6）⇒ 不起跳（缓冲已过期）。

这与 §6.3.1 的「按下当步算第 0 步、之后 6 步内有效」完全等价（实现见 `physics/tests.rs::t7`）。

### T8 禁止空中二段跳
接地起跳后，在第 1/2/3…/20 步各按一次跳跃键：除起跳那次外 `out.jumped` 恒为 `false`，且 `vel.y` 的最大值 == `JUMP_VELOCITY`。

### T9 单向平台双向行为
（a）从平台下方 `1 瓦片` 处**直接以 `vel.y = +JUMP_VELOCITY` 上升**（不走按键：`at_center` 后土狼时间已用尽，按键在纯空中不会起跳）且 `jump_held = true`（否则松手截断会把速度压到 `JUMP_CUT_VELOCITY`）：上升期间 `vel.y` 与该单侧重力下的解析值一致（不受平台影响），且 `aabb.min.y` 可以超过 `tile_top`；
（b）越过 `tile_top` 后下落：在 `aabb.min.y == tile_top` 停住、`grounded == true`、`vel.y == 0`；
（c）在平台上水平跑 60 步：`abs(aabb.min.y − tile_top) <= GEOM_EPS`（不被平台边缘卡住、不抖动）。

### T10 子步进防穿透（覆盖配置）
用 `PhysicsConfig { max_fall_speed: 4000.0, ..DEFAULT_PHYSICS }` 构造（`D_max = 66.7 px ⇒ n = 5`）重复 T1：**仍然**每一步重叠 `<= GEOM_EPS` 且终态 `aabb.min.y == tile_top`。

### T10b 子步数公式
对 `D_max ∈ {0, 1, 16, 16.1, 32, 128}` 断言 `n = max(1, ceil(D_max / SUBSTEP_MAX_DISTANCE))`，且 `D_max > MAX_STEP_DISPLACEMENT` 时 `debug_assert` 触发（`#[should_panic]` 测试）。

### T11 跳跃高度与公式一致
零输入平地起跳、全程按住跳跃键：从起跳前位置到最高点，`aabb.min.y` 的上升量 `== JUMP_HEIGHT × TILE_PX`（= 80.0 px，容差 0.1 px），且到达顶点所用步数 `== JUMP_APEX_TIME × FIXED_HZ`（= 24 步）。

### T12 松手截断的高度谱与可预测性
（a）全程按住：峰值上升 == 80.0 px（容差 0.1 px）；
（b）起跳后第 1 个固定步即松手：峰值上升 == 44.9 px（容差 0.3 px，见 §6.2.6 最坏情况公式）；
（c）峰值上升恒 ≥ `TILE_PX`（保证轻点必过 1 格台阶）；
（d）在上升段的**任意**步松手，峰值上升都落在 `[44.9, 80.0] px` 区间内。

### T13 终端速度
空中零输入下落 600 步：`vel.y == −MAX_FALL_SPEED` 恒成立（不为更负），且 `abs(vel.y) <= MAX_FALL_SPEED` 在每一步都成立。

### T14 摩擦与加速时间
（a）地面以 `vx = MAX_RUN_SPEED`、输入置零：在 `ceil(MAX_RUN_SPEED / (GROUND_FRICTION · FIXED_DT))`（= 8）步内 `vel.x == 0`，其后不再变化；
（b）地面静止、`move_x = +1`：在 `ceil(MAX_RUN_SPEED / (GROUND_ACCEL · FIXED_DT))`（= 10）步内 `vel.x == MAX_RUN_SPEED` 且永不越界。

### T15 Hazard 传感器
角色 AABB 只与 `Hazard` 瓦片边缘重叠 `HAZARD_INSET` 之内：`overlaps_tile_kind(.., −HAZARD_INSET, Hazard) == false`，且本步 `aabb` 与无 Hazard 时**完全相同**（不阻挡）；
重叠超过 `HAZARD_INSET` 时返回 `true`，且 `aabb` 仍与无 Hazard 时完全相同。

### T16 拾取外扩
角色 AABB 与拾取物 AABB 相距 `PICKUP_INFLATE / 2` 时 `overlaps_tile_kind(.., +PICKUP_INFLATE, ..) == true`；相距 `PICKUP_INFLATE × 4` 时为 `false`。

> 实现说明：**拾取物是实体不是瓦片**，因此 T16 实测的是 `overlaps_tile_kind` 的**外扩/内缩语义**
> （用瓦片作探针，几何等价）；真正的拾取判定归 M4（`check_pickups_and_zones`）。

### T17 确定性
同一初始 `Body`、同一 300 步输入序列、同一 `TestWorld`，连续跑两次：每步 `aabb.min.x.to_bits()/aabb.min.y.to_bits()/vel.x.to_bits()/vel.y.to_bits()` 完全相同。

### T18 面贴合不算重叠
`aabb` 的底面与 `Solid` 顶面精确相等时：`tile_range` 不包含该 `Solid` 格；因此该步不发生任何分离、`aabb` 不被推动（`aabb.min.y` 前后相等）。

### T19 边角擦过不产生水平推挤
`aabb.max.x` 与 `Solid` 的 `min.x` 精确相等、同时向下移动时：不产生 `hit_wall_x`，`aabb.min.x` 前后相等。

### T20 跨房间 / 越界查询
在房间右边界外侧 1 格查询：返回相邻房间映射后的真实 `TileKind`（与直接读该房间数据一致）；在 `grid` 范围外查询：恒返回 `Solid`。

> 实现位置：T20 测的是 ECS 适配层的 `TileWorld` 实现，因此在 `crates/game/src/tiles.rs` 的测试里
> （那里才有真实的 `LevelTiles`），其余 T1–T19、T21 在 `physics/tests.rs`。

### T21 房间重置提供干净状态
`Body::at_center(c)` 产出的 `Body` 满足 `vel == Vec2::ZERO && grounded == false && buffer_age == u32::MAX && airborne_since == COYOTE_STEPS + 1`；
并且紧接其后的第一次 `step` **不会**触发 `out.jumped`（即使 `jump_pressed == true`），证明出生/重置后没有凭空获得土狼跳跃。

---

## 6.10 证据索引

| 断言 | 标记 | 证据 |
|---|---|---|
| `Time::<Fixed>::from_hz/from_seconds/from_duration/set_timestep_hz`、`overstep_fraction` 存在 | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Time.html> |
| 默认固定步长 64 Hz（15625 µs）及其理由；`FixedUpdate` 一帧可跑 0/1/多次；`FixedUpdate` 在 `PreUpdate` 与 `Update` 之间；虚拟时钟暂停则 `FixedUpdate` 不跑 | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/time/struct.Fixed.html> |
| `Time<Fixed>` 的 timestep 以 `Duration`（纳秒整数精度）存储 | 【已验证】 | 同上（`set_timestep_hz` 小节） |
| `Duration` 纳秒精度 ⇒ `delta_secs() != 1.0/60.0` | 【INFERENCE】 | 由上一行 + 算术直接推论 |
| `Time` 在 `FixedUpdate` 内即 `Time<Fixed>` | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/prelude/struct.Time.html> |
| 0.20 buffered 消息 = `Message`、`MessageWriter<T>::write`；`Event` 是 observer 专用 | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/ecs/message/index.html> |
| `FixedUpdate` 是内建 schedule；`SystemSet` 与 `.chain()/.after()` | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/ecs/schedule/index.html> |
| `bevy::sprite::Anchor` 及 `BOTTOM_CENTER` 常量存在（不在 prelude） | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html> |
| `TransformSystems::Propagate` 与系统排序要求 | 【已验证】 | <https://docs.rs/bevy/0.20.0/src/pixel_grid_snap/pixel_grid_snap.rs.html> |
| `Vec2`/`glam` 为纯数学类型（无 ECS 依赖）；bevy 0.20 使用 glam 0.33 | 【已验证】 | <https://docs.rs/bevy/0.20.0/bevy/sprite/struct.Anchor.html>（`Deref<Target = Vec2>` 链接到 `glam/0.33.12` 源码页） |
| 依赖策略：不引入物理引擎 | 【已验证】 | `00` D3/D15；`09` 的生态矩阵（avian2d 无 0.20 版） |

> 本文不含**【UNVERIFIED】**条目：所有 Bevy API 断言均有 0.20.0 文档/示例源码直证，
> 其余全部是本项目自研的数值推导与几何论证（可被 §6.9 的测试逐条检验）。
