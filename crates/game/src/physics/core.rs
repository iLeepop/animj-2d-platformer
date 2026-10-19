//! 物理内核：**纯函数**，不依赖 Bevy ECS。所有者文档：`docs/06-physics-and-movement.md` §6.1.4、§6.3.2、§6.4。
//!
//! 允许依赖：`f32`、`std`、`bevy::math::Vec2`（纯数学类型）。禁止：`bevy_ecs`、时钟、`rand`、实体遍历顺序。

use bevy::math::Vec2;
use level_format::TILE_PX;

use super::aabb::Aabb;
use super::tile::{TileKind, TileWorld};
use crate::config::physics::{COYOTE_STEPS, PLAYER_SIZE, PhysicsConfig};

/// 瓦片边长（px）的 `f32` 形式。数值权威来源是 `00` §0.3。
pub const TILE: f32 = TILE_PX as f32;

/// 角色物理状态。**逻辑真相**（`06` §6.8.1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub aabb: Aabb,
    pub vel: Vec2,
    pub grounded: bool,
    /// `0` = 接地；`>0` = 已离地的固定步数（`06` §6.3.1）。
    pub airborne_since: u32,
    /// 距最近一次「跳跃键按下」的固定步数；`u32::MAX` = 已消费 / 无。
    pub buffer_age: u32,
}

impl Body {
    /// 出生 / 房间重置用的干净状态（`06` §6.5.3）：
    /// 土狼时间**用尽**，因此重置后不得凭空获得一次空中起跳。
    pub fn at_center(center: Vec2) -> Self {
        Self {
            aabb: Aabb::from_center_size(center, PLAYER_SIZE),
            vel: Vec2::ZERO,
            grounded: false,
            airborne_since: COYOTE_STEPS + 1,
            buffer_age: u32::MAX,
        }
    }
}

/// 单个固定步的输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepInput {
    /// `-1` / `0` / `+1`。
    pub move_x: i8,
    /// 本步**刚**按下跳跃（`just_pressed` 语义，只在按下那一步为真）。
    pub jump_pressed: bool,
    /// 本步仍按住跳跃。
    pub jump_held: bool,
}

/// 单个固定步的结果（供表现层与测试使用，语义见 `06` §6.1.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepOutcome {
    pub jumped: bool,
    pub landed: bool,
    pub head_bumped: bool,
    /// `+1` / `-1` = 本步水平分离的方向，`0` = 无。
    pub hit_wall_x: i32,
    pub walked_off: bool,
}

/// 推进一个固定步（玩家之间**不**碰撞）。等价于 [`step_with_players`] 的空障碍列表。
pub fn step(
    cfg: &PhysicsConfig,
    world: &dyn TileWorld,
    body: &mut Body,
    input: StepInput,
    out: &mut StepOutcome,
) {
    step_with_players(cfg, world, body, input, &[], out);
}

/// 推进一个固定步，并把 `others`（其他玩家的 AABB）当作**实心**障碍。
///
/// **唯一**的状态转移函数。`others` 的顺序不影响结果：X/Y 分离总是取最近的面。
/// 语义见 `06` §6.4.10：玩家互相当作可站立、可撞墙的实心盒，不做推挤。
pub fn step_with_players(
    cfg: &PhysicsConfig,
    world: &dyn TileWorld,
    body: &mut Body,
    input: StepInput,
    others: &[Aabb],
    out: &mut StepOutcome,
) {
    *out = StepOutcome::default();
    let was_grounded = body.grounded;

    // ---- 0) 尺寸不变量（R1–R3）----
    // 反向 / 变形的 AABB 会让 `tile_range` 返回空区间（`tx0 > tx1`），等于**关闭全部碰撞**：
    // 角色会穿地板、掉出相机，还表现为"撞墙后慢慢卡出去"。正常路径永不触发；
    // 这里修复的是外部给了畸形 `Body`（例如关卡把重生点塞进实心格）。
    if !aabb_intact(&body.aabb, cfg.player_size) {
        body.aabb = Aabb::from_center_size(body.aabb.center(), cfg.player_size);
    }

    // ---- 1) 计时器（§6.3.2 步骤 1）----
    if body.grounded {
        body.airborne_since = 0;
    } else {
        body.airborne_since = body.airborne_since.saturating_add(1);
    }
    if input.jump_pressed {
        body.buffer_age = 0;
    } else {
        body.buffer_age = body.buffer_age.saturating_add(1);
    }

    // ---- 2) 起跳意图（§6.3.2 步骤 2）----
    let coyote_ok = body.airborne_since <= cfg.coyote_steps;
    let buffer_ok = body.buffer_age <= cfg.jump_buffer_steps;
    if buffer_ok && (body.grounded || coyote_ok) {
        body.vel.y = cfg.jump_velocity;
        body.grounded = false;
        // 土狼时间用尽 ⇒ 禁止空中二段跳
        body.airborne_since = cfg.coyote_steps + 1;
        body.buffer_age = u32::MAX;
        out.jumped = true;
    }

    // ---- 3) 加速度（§6.2.5 水平；垂直按 grounded / 速度符号选段）----
    // 注意：`grounded` 取步骤 2 **之后**的值（起跳当步即走空中分支，见 §6.2.5）
    let target_vx = input.move_x as f32 * cfg.max_run_speed;
    let a_limit = if body.grounded {
        if input.move_x != 0 {
            cfg.ground_accel
        } else {
            cfg.ground_friction
        }
    } else if input.move_x != 0 {
        cfg.air_accel
    } else {
        cfg.air_drag
    };
    let a_x = ((target_vx - body.vel.x) / cfg.fixed_dt).clamp(-a_limit, a_limit);
    let a_y = if body.grounded {
        0.0
    } else if body.vel.y > 0.0 {
        -cfg.gravity
    } else {
        -cfg.fall_gravity
    };

    // ---- 4) 速度积分与钳制（§6.2.3 梯形法；顺序不可交换）----
    let vel_before = body.vel;
    body.vel.x += a_x * cfg.fixed_dt;
    body.vel.y += a_y * cfg.fixed_dt;
    body.vel.y = body.vel.y.max(-cfg.max_fall_speed);
    // 松手截断必须在位移积分之前，否则松手当步会多上升一整步的旧速度
    if !input.jump_held && !body.grounded && body.vel.y > cfg.jump_cut_velocity {
        body.vel.y = cfg.jump_cut_velocity;
    }
    body.vel.x = body.vel.x.clamp(-cfg.max_run_speed, cfg.max_run_speed);
    let vel_after = body.vel;
    debug_assert!(body.vel.is_finite(), "速度必须有限（§6.8.3）");

    // ---- 5) 位移积分 + 子步进（§6.4.1）----
    let displacement = 0.5 * (vel_before + vel_after) * cfg.fixed_dt;
    let n = substep_count(cfg, displacement);
    let sub = displacement / n as f32;
    for _ in 0..n {
        if sub.x != 0.0 {
            move_x(cfg, world, &mut body.aabb, sub.x, others, out);
        }
        if sub.y != 0.0 {
            // `prev_foot` 在每个子步开始前刷新：单向平台的三条件依赖它（§6.4.1 / §6.4.5）
            let prev_foot = body.aabb.min.y;
            move_y(cfg, world, &mut body.aabb, sub.y, prev_foot, others, out);
        }
    }

    // ---- 6) 应用碰撞结果 ----
    if out.hit_wall_x != 0 {
        body.vel.x = 0.0; // 撞墙吸收水平速度（不粘滞，见 §6.2.5）
    }
    if out.head_bumped {
        // 撞头吸收上升速度（`06` §6.3.4 边界表）：否则会在低天花板下"吸附"到剩余上升时间结束
        body.vel.y = body.vel.y.min(0.0);
    }
    // 支撑探测：只有**不在上升**时才可能被支撑。
    // 若不加 `vel.y <= 0`，上升穿过单向平台时脚底一进入容差带就会被判为接地，
    // 该步 `a_y` 变成 0（丢掉一步重力 ⇒ 跳跃高度偏高且上升被"吸住"）。
    // 这是 T9 抓到的、06 §6.3.2 步骤 6 未覆盖的相互作用。
    match nearest_standable_top_below(cfg, world, &body.aabb, others) {
        Some(top) if body.vel.y <= 0.0 => {
            body.aabb.min.y = top; // 吸附到顶面：零 skin，面精确贴合
            body.aabb.max.y = top + cfg.player_size.y; // 保持尺寸（§6.4.9）
            body.grounded = true;
        }
        _ => body.grounded = false,
    }
    if body.grounded && body.vel.y < 0.0 {
        body.vel.y = 0.0;
    }
    if was_grounded && !body.grounded {
        out.walked_off = true;
    }
}

/// 传感器查询：把 `aabb` 按 `expand` 调整后，是否与 `kind` 类别的瓦片相交。
///
/// `expand > 0` = **外扩**（拾取判定，手感慷慨）；`expand < 0` = **内缩**（Hazard 判定，擦边不算死）。
/// 见 `06` §6.5.3 / §6.5.4。
pub fn overlaps_tile_kind(world: &dyn TileWorld, aabb: Aabb, expand: f32, kind: TileKind) -> bool {
    let probe = aabb.inset(-expand);
    let (tx0, tx1, ty0, ty1) = tile_range(GEOM_EPS_LOCAL, &probe);
    tx0 <= tx1 && ty0 <= ty1 && world.any_kind_in(tx0, ty0, tx1, ty1, kind)
}

/// `overlaps_tile_kind` 只做几何相交，容差与 `PhysicsConfig` 无关，因此这里用常量（= `GEOM_EPS`）。
const GEOM_EPS_LOCAL: f32 = crate::config::physics::GEOM_EPS;

/// 子步数：`n = max(1, ceil(max(|d.x|, |d.y|) / SUBSTEP_MAX_DISTANCE))`（`06` §6.4.1 B / §6.4.6）。
///
/// 超过常量表硬约束时 `debug_assert` 触发（说明有人把速度常量改大到不再安全）。
pub(crate) fn substep_count(cfg: &PhysicsConfig, displacement: Vec2) -> u32 {
    let max_axis = displacement.x.abs().max(displacement.y.abs());
    debug_assert!(
        max_axis <= cfg.substep_max_distance * cfg.substep_hard_cap as f32 + 1e-3,
        "单步位移 {max_axis} px 超过常量表硬约束（`06` §6.4.6）：请复核速度常量"
    );
    let n = (max_axis / cfg.substep_max_distance).ceil() as u32;
    n.max(1)
}

/// 盒子的尺寸与朝向是否仍是出生时的不变量（`06` §6.4.9）。
fn aabb_intact(aabb: &Aabb, size: Vec2) -> bool {
    let s = aabb.size();
    aabb.min.x < aabb.max.x
        && aabb.min.y < aabb.max.y
        && (s.x - size.x).abs() <= 1e-3
        && (s.y - size.y).abs() <= 1e-3
}

/// 瓦片范围查询的**唯一入口**（`06` §6.4.2）。
///
/// 上界减 `GEOM_EPS`：AABB 与瓦片面**精确贴合**时，贴合面上那一格不算重叠（零 skin 策略）。
pub(crate) fn tile_range(eps: f32, aabb: &Aabb) -> (i32, i32, i32, i32) {
    (
        (aabb.min.x / TILE).floor() as i32,
        ((aabb.max.x - eps) / TILE).floor() as i32,
        (aabb.min.y / TILE).floor() as i32,
        ((aabb.max.y - eps) / TILE).floor() as i32,
    )
}

/// X 轴分离（先解 X，见 `06` §6.4.3）。
///
/// **尺寸不变量**（`06` §6.4.9）：夹取只改行进侧的那一面，另一面按 `player_size` 反推，
/// 即「把盒子整体推出」而不是「压扁盒子」。同时只认**行进方向前方**的面
/// （`face` 不早于出发时的对应边），否则身体与之重叠的格子（典型：埋在地板里）会用
/// 自己的面把盒子夹反。
fn move_x(
    cfg: &PhysicsConfig,
    world: &dyn TileWorld,
    aabb: &mut Aabb,
    dx: f32,
    others: &[Aabb],
    out: &mut StepOutcome,
) {
    let width = cfg.player_size.x;
    let prev = *aabb;
    let new = aabb.translate(Vec2::new(dx, 0.0));
    let (tx0, tx1, ty0, ty1) = tile_range(cfg.geom_eps, &new);
    let mut limit: Option<f32> = None;
    let mut push = |face: f32| {
        // 只认前方的面：身体已经越过它的（重叠态）不参与夹取
        let ahead = if dx > 0.0 {
            face >= prev.max.x - cfg.geom_eps
        } else {
            face <= prev.min.x + cfg.geom_eps
        };
        if !ahead {
            return;
        }
        limit = Some(match limit {
            None => face,
            Some(l) if dx > 0.0 => l.min(face),
            Some(l) => l.max(face),
        });
    };
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            if !world.tile(tx, ty).is_solid() {
                continue;
            }
            push(if dx > 0.0 {
                tx as f32 * TILE // 墙的左面
            } else {
                (tx + 1) as f32 * TILE // 墙的右面
            });
        }
    }
    // 其他玩家 = 实心盒（`06` §6.4.10）。竖直方向必须真的重叠才算挡路。
    for other in others {
        let vertical_overlap =
            new.max.y > other.min.y + cfg.geom_eps && new.min.y < other.max.y - cfg.geom_eps;
        if !vertical_overlap {
            continue;
        }
        if dx > 0.0 && new.max.x > other.min.x + cfg.geom_eps {
            push(other.min.x);
        } else if dx < 0.0 && new.min.x < other.max.x - cfg.geom_eps {
            push(other.max.x);
        }
    }

    let mut result = new;
    if let Some(l) = limit {
        if dx > 0.0 {
            result.max.x = l;
            result.min.x = l - width;
        } else {
            result.min.x = l;
            result.max.x = l + width;
        }
        out.hit_wall_x = if dx > 0.0 { 1 } else { -1 };
    }
    *aabb = result;
}

/// Y 轴分离 + 落地吸附（`06` §6.4.4 / §6.4.5）。
///
/// 与 [`move_x`] 同样遵守**尺寸不变量**（`06` §6.4.9）：夹取后按 `player_size` 恢复另一面。
fn move_y(
    cfg: &PhysicsConfig,
    world: &dyn TileWorld,
    aabb: &mut Aabb,
    dy: f32,
    prev_foot: f32,
    others: &[Aabb],
    out: &mut StepOutcome,
) {
    let height = cfg.player_size.y;
    let prev = *aabb;
    let new = aabb.translate(Vec2::new(0.0, dy));
    let (tx0, tx1, ty0, ty1) = tile_range(cfg.geom_eps, &new);

    // 向下要「最高的顶面」，向上要「最低的底面」；显式取极值，不依赖遍历顺序。
    let mut land_top: Option<f32> = None;
    let mut head_bottom: Option<f32> = None;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let top = (ty + 1) as f32 * TILE;
            let bottom = ty as f32 * TILE;
            match world.tile(tx, ty) {
                TileKind::Solid => {
                    if dy < 0.0 {
                        land_top = Some(land_top.map_or(top, |t: f32| t.max(top)));
                    } else {
                        head_bottom = Some(head_bottom.map_or(bottom, |b: f32| b.min(bottom)));
                    }
                }
                // 单向平台：仅三条件同时成立时在 Y 轴阻挡（§6.4.5）
                TileKind::OneWay => {
                    if dy < 0.0 && prev_foot >= top - cfg.geom_eps && new.min.y < top {
                        land_top = Some(land_top.map_or(top, |t: f32| t.max(top)));
                    }
                }
                TileKind::Empty | TileKind::Hazard => {}
            }
        }
    }

    // 其他玩家 = 实心盒（`06` §6.4.10）：可以站在头上，也会撞到脚底。
    for other in others {
        let horizontal_overlap =
            new.max.x > other.min.x + cfg.geom_eps && new.min.x < other.max.x - cfg.geom_eps;
        if !horizontal_overlap {
            continue;
        }
        if dy < 0.0 {
            let top = other.max.y;
            if prev_foot >= top - cfg.geom_eps && new.min.y < top {
                land_top = Some(land_top.map_or(top, |t: f32| t.max(top)));
            }
        } else if dy > 0.0 {
            let bottom = other.min.y;
            if prev.max.y <= bottom + cfg.geom_eps && new.max.y > bottom {
                head_bottom = Some(head_bottom.map_or(bottom, |b: f32| b.min(bottom)));
            }
        }
    }

    let mut result = new;
    let mut resolved = false;
    if let Some(top) = land_top {
        result.min.y = top;
        result.max.y = top + height;
        out.landed = true;
        resolved = true;
    }
    if let Some(bottom) = head_bottom {
        result.max.y = bottom;
        result.min.y = bottom - height;
        out.head_bumped = true;
        resolved = true;
    }
    // 落地吸附：本子步没有任何分离时，检查脚底是否已在容差带内（浮点缝隙的唯一兜底）
    if !resolved
        && dy < 0.0
        && let Some(top) = nearest_standable_top_below(cfg, world, &result, others)
    {
        result.min.y = top;
        result.max.y = top + height;
        out.landed = true;
    }
    *aabb = result;
}

/// 脚底下方容差带内、**最高**的可站立（Solid / OneWay）瓦片顶面；不存在返回 `None`。
///
/// 同时用于「落地吸附」与每步的「支撑探测」（§6.3.2 步骤 6）：两者必须给出同一个答案，
/// 否则会出现「落地当步 grounded、下一当步 airborne」的两拍抖动。
pub(crate) fn nearest_standable_top_below(
    cfg: &PhysicsConfig,
    world: &dyn TileWorld,
    aabb: &Aabb,
    others: &[Aabb],
) -> Option<f32> {
    let eps = cfg.geom_eps;
    let probe_min_x = aabb.min.x + eps;
    let probe_max_x = aabb.max.x - eps;
    if probe_max_x < probe_min_x {
        return None; // 退化：AABB 比 2ε 还窄
    }
    let tx0 = (probe_min_x / TILE).floor() as i32;
    let tx1 = (probe_max_x / TILE).floor() as i32;
    let ty0 = ((aabb.min.y - cfg.contact_tolerance) / TILE).floor() as i32;
    let ty1 = ((aabb.min.y - eps) / TILE).floor() as i32;

    let mut best: Option<f32> = None;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            if !world.tile(tx, ty).is_standable() {
                continue;
            }
            let top = (ty + 1) as f32 * TILE;
            // 排除「已穿透当前行」的情形：只认可不高于脚底的顶面
            if top <= aabb.min.y + eps {
                best = Some(best.map_or(top, |b: f32| b.max(top)));
            }
        }
    }
    // 站在其他玩家头上（`06` §6.4.10）：与瓦片同一条容差带。
    for other in others {
        let horizontal_overlap = probe_max_x >= other.min.x && probe_min_x <= other.max.x;
        if !horizontal_overlap {
            continue;
        }
        let top = other.max.y;
        if top <= aabb.min.y + eps && top >= aabb.min.y - cfg.contact_tolerance {
            best = Some(best.map_or(top, |b: f32| b.max(top)));
        }
    }
    best
}
