//! `docs/06-physics-and-movement.md` §6.9 的 T1–T21 不变量测试。
//!
//! 全部**不需要 Bevy App / 窗口 / GPU**：`TestWorld` 实现 [`TileWorld`] 只需要一个方法。
//! T20 的房间映射部分在 `tiles.rs` 的测试里（那里才有真实的 `LevelTiles`）。

use bevy::math::Vec2;

use super::aabb::Aabb;
use super::core::{TILE, overlaps_tile_kind, step, step_with_players, substep_count};
use super::tile::{TileKind, TileWorld};
use super::{Body, StepInput, StepOutcome};
use crate::config::physics::*;

/// 极简瓦片世界：`grid[ty * cols + tx]`，`ty = 0` 是最下面一行（与全局瓦片坐标一致）。
struct TestWorld {
    grid: Vec<TileKind>,
    cols: i32,
    rows: i32,
}

impl TestWorld {
    fn new(cols: i32, rows: i32) -> Self {
        Self {
            grid: vec![TileKind::Empty; (cols * rows) as usize],
            cols,
            rows,
        }
    }

    fn set(&mut self, tx: i32, ty: i32, kind: TileKind) {
        self.grid[(ty * self.cols + tx) as usize] = kind;
    }

    fn fill_row(&mut self, ty: i32, kind: TileKind) {
        for tx in 0..self.cols {
            self.set(tx, ty, kind);
        }
    }

    fn fill_col(&mut self, tx: i32, ty0: i32, ty1: i32, kind: TileKind) {
        for ty in ty0..=ty1 {
            self.set(tx, ty, kind);
        }
    }
}

impl TileWorld for TestWorld {
    fn tile(&self, tx: i32, ty: i32) -> TileKind {
        // 地图外 = 硬墙（`06` §6.6.3）
        if tx < 0 || ty < 0 || tx >= self.cols || ty >= self.rows {
            return TileKind::Solid;
        }
        self.grid[(ty * self.cols + tx) as usize]
    }
}

// ---------- R1–R3：分离必须保持 AABB 尺寸（实机 bug 回归）----------
//
// 实机现象：重生点让身体埋进地板 → 分离只改盒子的**一侧** → 盒子被压扁/压反
// → `tile_range` 对反向盒子返回空区间 → 从此穿一切瓦片、掉出相机。

fn assert_box_invariant(body: &Body, context: &str) {
    let size = body.aabb.size();
    assert!(
        (size.x - PLAYER_SIZE.x).abs() < 1e-3,
        "{context}：宽度变形 {}",
        size.x
    );
    assert!(
        (size.y - PLAYER_SIZE.y).abs() < 1e-3,
        "{context}：高度变形 {}",
        size.y
    );
    assert!(
        body.aabb.min.x < body.aabb.max.x && body.aabb.min.y < body.aabb.max.y,
        "{context}：AABB 反向 {:?}",
        body.aabb
    );
}

/// R1：身体中心位于地板厚度中间（旧默认重生点就是这样），一步后必须站上地板，
/// **尺寸不变**。旧实现把 `min.y` 顶到地板顶面却不移 `max.y` ⇒ 高度从 28 变 14。
#[test]
fn r1_embedded_in_floor_keeps_box_size() {
    let world = flat_world(); // 底行实心，顶面 y = TILE
    let cfg = DEFAULT_PHYSICS;
    let mut body = Body::at_center(Vec2::new(100.0, TILE * 0.5));
    let mut out = StepOutcome::default();
    for i in 0..3 {
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert_box_invariant(&body, &format!("第 {i} 步"));
    }
    assert!(body.grounded);
    assert!((body.aabb.min.y - TILE).abs() < 1e-3, "脚底必须在地板顶面");
}

/// R2：埋在地板里**同时向右走**——地板瓦片与身体在 Y 上重叠，旧实现用它的左面
/// 去夹 `max.x`，会把盒子压扁甚至压反。
#[test]
fn r2_embedded_and_walking_keeps_box_size() {
    let world = flat_world();
    let cfg = DEFAULT_PHYSICS;
    let mut body = Body::at_center(Vec2::new(100.0, TILE * 0.5));
    let mut out = StepOutcome::default();
    let input = StepInput {
        move_x: 1,
        jump_pressed: false,
        jump_held: false,
    };
    for i in 0..30 {
        step(&cfg, &world, &mut body, input, &mut out);
        assert_box_invariant(&body, &format!("第 {i} 步"));
    }
    assert!(body.grounded);
    assert!((body.aabb.min.y - TILE).abs() < 1e-3);
}

/// R3：出生在墙体内部时，分离必须把盒子整体推出（尺寸优先），不得留下反向盒子。
#[test]
fn r3_embedded_in_wall_is_depenetrated_without_deform() {
    let mut world = flat_world();
    world.fill_col(5, 1, 6, TileKind::Solid);
    let cfg = DEFAULT_PHYSICS;
    let mut body = Body::at_center(Vec2::new(TILE * 5.5, TILE * 2.0));
    let mut out = StepOutcome::default();
    for i in 0..10 {
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 1,
                jump_pressed: false,
                jump_held: false,
            },
            &mut out,
        );
        assert_box_invariant(&body, &format!("第 {i} 步"));
    }
}

/// R5：贴着地图左边界持续向左推 600 步——不得越界，盒子始终不变形。
/// （旧实现里盒子一旦被压反，`tile_range` 返回空区间，连地图外框墙都拦不住。）
#[test]
fn r5_pushing_map_edge_never_escapes() {
    let world = flat_world();
    let cfg = DEFAULT_PHYSICS;
    let mut body = standing_on(TILE * 2.0, TILE);
    let mut out = StepOutcome::default();
    let input = StepInput {
        move_x: -1,
        jump_pressed: false,
        jump_held: false,
    };
    for i in 0..600 {
        step(&cfg, &world, &mut body, input, &mut out);
        assert_box_invariant(&body, &format!("第 {i} 步"));
        assert!(
            body.aabb.min.x >= -1e-2,
            "第 {i} 步穿出地图左边界：min.x={}",
            body.aabb.min.x
        );
    }
}

/// H1：向上撞**实心**天花板必须被挡住（旧测试清单里没有撞头用例）。
#[test]
fn h1_head_bump_blocks_solid_ceiling() {
    let mut world = flat_world();
    // 天花板：ty = 4（底面 y = 128，顶面 160）
    world.fill_row(4, TileKind::Solid);
    let cfg = DEFAULT_PHYSICS;
    let mut body = standing_on(TILE * 3.0, TILE);
    body.vel.y = JUMP_VELOCITY;
    body.grounded = false;
    let mut out = StepOutcome::default();
    let mut bumped = false;
    for _ in 0..90 {
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 0,
                jump_pressed: false,
                jump_held: true,
            },
            &mut out,
        );
        assert!(
            body.aabb.max.y <= TILE * 4.0 + 1e-2,
            "头部穿过实心天花板：max.y={}",
            body.aabb.max.y
        );
        bumped |= out.head_bumped;
    }
    assert!(bumped, "从未报告 head_bumped");
}

/// H2：单向平台——从下方穿过、从上方站住（`06` §6.4.5）。
/// 实机"跳跃时忽略网格地块"就是这条**设计行为**（`=` 不是实心块）。
#[test]
fn h2_oneway_is_passable_from_below() {
    let mut world = flat_world();
    // 平台放在够得着的高度：顶面 y = 96（跳跃脚底峰值 112）
    world.fill_row(2, TileKind::OneWay);
    let cfg = DEFAULT_PHYSICS;
    let mut body = standing_on(TILE * 3.0, TILE);
    body.vel.y = JUMP_VELOCITY;
    body.grounded = false;
    let mut out = StepOutcome::default();
    let mut peak = 0.0f32;
    for _ in 0..90 {
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 0,
                jump_pressed: false,
                jump_held: true,
            },
            &mut out,
        );
        peak = peak.max(body.aabb.min.y);
    }
    assert!(peak > TILE * 3.0, "应当能穿过单向平台：peak={peak}");
    assert!(body.grounded, "落回后应站住");
    assert!(
        (body.aabb.min.y - TILE * 3.0).abs() < 1e-2,
        "应站在平台顶面：{}",
        body.aabb.min.y
    );
}

// ---------- 辅助 ----------

/// 平地世界：底行实心。
fn flat_world() -> TestWorld {
    let mut world = TestWorld::new(16, 16);
    world.fill_row(0, TileKind::Solid);
    world
}

/// 站在 `floor_top` 上的干净状态（脚底精确贴合顶面）。
fn standing_on(x: f32, floor_top: f32) -> Body {
    let mut body = Body::at_center(Vec2::new(x, floor_top + PLAYER_SIZE.y * 0.5));
    body.grounded = true;
    body.airborne_since = 0;
    body
}

/// `aabb` 与任何 `Solid` 瓦片的最大重叠深度（`min(overlap_x, overlap_y)`）。
fn overlap_depth(world: &TestWorld, aabb: &Aabb) -> f32 {
    let (tx0, tx1, ty0, ty1) = super::core::tile_range(GEOM_EPS, aabb);
    let mut worst: f32 = 0.0;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            if !world.tile(tx, ty).is_solid() {
                continue;
            }
            let t_min = Vec2::new(tx as f32 * TILE, ty as f32 * TILE);
            let t_max = t_min + Vec2::splat(TILE);
            let ox = aabb.max.x.min(t_max.x) - aabb.min.x.max(t_min.x);
            let oy = aabb.max.y.min(t_max.y) - aabb.min.y.max(t_min.y);
            worst = worst.max(ox.min(oy));
        }
    }
    worst.max(0.0)
}

/// 起跳一次并测量峰值上升高度与到达峰值的步数。
///
/// `release_at`：第几步开始松手（`None` = 全程按住；`Some(1)` = 起跳当步就松手）。
fn measure_jump(world: &TestWorld, start: &Body, release_at: Option<u32>) -> (f32, u32) {
    let cfg = DEFAULT_PHYSICS;
    let mut body = *start;
    let base = body.aabb.min.y;
    let mut peak = 0.0f32;
    let mut apex_step = 0u32;
    for i in 1..=200 {
        let held = release_at.is_none_or(|release| i < release);
        let mut out = StepOutcome::default();
        step(
            &cfg,
            world,
            &mut body,
            StepInput {
                jump_pressed: i == 1,
                jump_held: held,
                ..Default::default()
            },
            &mut out,
        );
        let rise = body.aabb.min.y - base;
        if rise > peak {
            peak = rise;
            apex_step = i;
        }
        if peak > 0.0 && body.vel.y <= 0.0 {
            break; // 到达顶点
        }
    }
    (peak, apex_step)
}

/// 从 `start` 自由下落，返回第几步落地。
fn steps_until_landing(world: &TestWorld, start: &Body) -> u32 {
    let cfg = DEFAULT_PHYSICS;
    let mut body = *start;
    for i in 1..=1000 {
        let mut out = StepOutcome::default();
        step(&cfg, world, &mut body, StepInput::default(), &mut out);
        if out.landed {
            return i;
        }
    }
    panic!("1000 步内没有落地");
}

// ---------- T1..T11 ----------

/// T1 垂直不穿透：以终端速度下落 60 步，每步重叠深度 ≤ `GEOM_EPS`，终态贴合地面。
#[test]
fn t1_vertical_no_tunneling() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    let floor_top = TILE;
    let mut body = Body::at_center(Vec2::new(100.0, floor_top + 200.0 + PLAYER_SIZE.y * 0.5));
    body.vel = Vec2::new(0.0, -MAX_FALL_SPEED);
    body.airborne_since = 7;

    for i in 0..60 {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert!(
            overlap_depth(&world, &body.aabb) <= cfg.geom_eps + 1e-3,
            "第 {i} 步穿透：{:?}",
            body.aabb
        );
    }
    assert!((body.aabb.min.y - floor_top).abs() <= 1e-3);
    assert!(body.grounded);
}

/// T2 水平不穿透：以最大跑速撞 1 格宽实心墙，撞后 `vel.x == 0`。
#[test]
fn t2_horizontal_no_tunneling() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = flat_world();
    world.fill_col(6, 0, 3, TileKind::Solid);
    let wall_left = 6.0 * TILE;

    let mut body = standing_on(100.0, TILE);
    body.vel.x = MAX_RUN_SPEED;
    for i in 0..30 {
        let mut out = StepOutcome::default();
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 1,
                ..Default::default()
            },
            &mut out,
        );
        assert!(
            body.aabb.max.x <= wall_left + 1e-3,
            "第 {i} 步穿墙：{}",
            body.aabb.max.x
        );
    }
    assert_eq!(body.vel.x, 0.0);
}

/// T3 对角不穿透（角落）：终态要么在平台顶面，要么被挡在侧面。
#[test]
fn t3_diagonal_no_tunneling() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = TestWorld::new(16, 16);
    for ty in 0..=1 {
        for tx in 3..=5 {
            world.set(tx, ty, TileKind::Solid);
        }
    }
    let block_top = 2.0 * TILE;
    let block_right = 6.0 * TILE;

    // 起点：右边缘在方块右面外侧 8 px、脚底在方块顶面上方 20 px ⇒ 两个方向的到达时刻接近，
    // 正是「擦角」场景（水平 3.33 px/步、垂直 ~9.3 px/步 ⇒ 2.4 步 vs 2.1 步）
    let start_max_x = block_right + 8.0;
    let start_min_y = block_top + 20.0;
    let mut body = Body::at_center(Vec2::new(
        start_max_x - PLAYER_SIZE.x * 0.5,
        start_min_y + PLAYER_SIZE.y * 0.5,
    ));
    body.vel = Vec2::new(-MAX_RUN_SPEED, -MAX_FALL_SPEED);
    body.airborne_since = 7;

    for i in 0..10 {
        let mut out = StepOutcome::default();
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: -1,
                ..Default::default()
            },
            &mut out,
        );
        assert!(
            overlap_depth(&world, &body.aabb) <= cfg.geom_eps + 1e-3,
            "第 {i} 步穿透"
        );
    }
    let on_top = (body.aabb.min.y - block_top).abs() <= 1e-3;
    let on_side = (body.aabb.max.x - block_right).abs() <= 1e-3;
    assert!(on_top || on_side, "终态位置异常：{:?}", body.aabb);
}

/// T4 静止时不下陷：站 600 步，位置与速度不得有任何变化。
#[test]
fn t4_standing_is_stable() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    let mut body = standing_on(100.0, TILE);

    for i in 0..600 {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert!(
            (body.aabb.min.y - TILE).abs() <= cfg.geom_eps + 1e-3,
            "第 {i} 步下陷：{}",
            body.aabb.min.y
        );
        assert!(body.grounded, "第 {i} 步丢了 grounded");
        assert_eq!(body.vel, Vec2::ZERO, "第 {i} 步速度非零");
    }
}

/// T5 撞墙不粘滞：贴墙下落时 `vel.x == 0`；到终端速度；墙移除后一步内重新加速。
#[test]
fn t5_wall_does_not_stick() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = TestWorld::new(16, 60);
    world.fill_col(8, 0, 59, TileKind::Solid);
    let wall_left = 8.0 * TILE;

    let mut body = Body::at_center(Vec2::new(wall_left - PLAYER_SIZE.x * 0.5, 1500.0));
    body.airborne_since = 7;

    let input = StepInput {
        move_x: 1,
        ..Default::default()
    };
    for i in 0..60 {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, input, &mut out);
        assert_eq!(body.vel.x, 0.0, "第 {i} 步未清零水平速度");
    }
    assert!(
        (body.vel.y + MAX_FALL_SPEED).abs() <= 1e-3,
        "第 60 步应到终端速度，实际 {}",
        body.vel.y
    );

    world.fill_col(8, 0, 59, TileKind::Empty);
    let mut out = StepOutcome::default();
    step(&cfg, &world, &mut body, input, &mut out);
    assert!(body.vel.x > 0.0, "墙移除后未重新加速");
}

/// T6 coyote 边界：离地后第 `COYOTE_STEPS` 个空中步仍可起跳，第 +1 步不可。
#[test]
fn t6_coyote_boundary() {
    let cfg = DEFAULT_PHYSICS;
    // 平台抬高到 ty = 20，右侧是空的：踏空后不会在观察窗内落地
    let mut world = TestWorld::new(16, 32);
    for tx in 0..=4 {
        world.set(tx, 20, TileKind::Solid);
    }
    let platform_top = 21.0 * TILE;

    for (airborne_step, expect_jump) in [(COYOTE_STEPS, true), (COYOTE_STEPS + 1, false)] {
        let mut body = standing_on(100.0, platform_top);
        let mut airborne_seen = 0u32;
        let mut jumped = false;
        for _ in 0..200 {
            // 计数器与 `Body::airborne_since` 的关系：第 W+n 个空中步的**起始**计数器为 n、
            // 起始 `airborne_since` 为 n-1（判定前自增到 n），因此按下条件用 `==`。
            let press = !body.grounded && airborne_seen == airborne_step;
            let mut out = StepOutcome::default();
            step(
                &cfg,
                &world,
                &mut body,
                StepInput {
                    move_x: 1,
                    jump_pressed: press,
                    jump_held: press,
                },
                &mut out,
            );
            jumped |= out.jumped;
            if !body.grounded {
                airborne_seen += 1;
            }
            if airborne_seen > airborne_step {
                break;
            }
        }
        assert_eq!(jumped, expect_jump, "空中第 {airborne_step} 步按下跳跃");
    }
}

/// T7 jump buffer 边界。
///
/// 精确语义（由 §6.3.2 的顺序决定）：落地只能在**位移之后**被观测到，因此缓冲起跳落在
/// 落地后的**下一步**。故「落地发生在按下之后第 `k` 步」时：`k <= JUMP_BUFFER_STEPS - 1` 起跳，
/// 否则不起跳（等价于「按下后 6 步内有效」，见 §6.3.4）。
#[test]
fn t7_jump_buffer_boundary() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    let floor_top = TILE;

    let mut start = Body::at_center(Vec2::new(100.0, floor_top + 300.0));
    start.airborne_since = 60; // 土狼窗口早已用尽 ⇒ 只考察缓冲
    let landing = steps_until_landing(&world, &start);
    assert!(landing > 8, "需要一个足够长的下落过程，实际 {landing} 步");

    for (steps_before, expect_jump) in [(JUMP_BUFFER_STEPS - 1, true), (JUMP_BUFFER_STEPS, false)] {
        let press_at = landing - steps_before;
        let mut body = start;
        let mut jumped_at = None;
        for i in 1..=landing + 3 {
            let press = i == press_at;
            let mut out = StepOutcome::default();
            step(
                &cfg,
                &world,
                &mut body,
                StepInput {
                    jump_pressed: press,
                    jump_held: press,
                    ..Default::default()
                },
                &mut out,
            );
            if out.jumped {
                jumped_at = Some(i);
            }
        }
        assert_eq!(
            jumped_at.is_some(),
            expect_jump,
            "落地前 {steps_before} 步按下跳跃"
        );
        if let Some(at) = jumped_at {
            assert_eq!(at, landing + 1, "缓冲起跳应发生在落地后的下一步");
        }
    }
}

/// T8 禁止空中二段跳：起跳后任意时刻再按都不得起跳，且 `vel.y` 不得超过 `JUMP_VELOCITY`。
#[test]
fn t8_no_double_jump() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    let mut body = standing_on(100.0, TILE);
    let mut max_vel_y = f32::MIN;

    for i in 1..=20u32 {
        let mut out = StepOutcome::default();
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                jump_pressed: true, // 每一步都"刚按下"
                jump_held: true,
                ..Default::default()
            },
            &mut out,
        );
        if i == 1 {
            assert!(out.jumped, "第一步应起跳");
        } else {
            assert!(!out.jumped, "第 {i} 步不得起跳（空中二段跳）");
        }
        assert!(!body.grounded, "第 {i} 步仍应在空中（起跳 44 步后才落地）");
        max_vel_y = max_vel_y.max(body.vel.y);
    }
    assert!(
        max_vel_y <= JUMP_VELOCITY + 1e-3,
        "竖直速度越过起跳初速：{max_vel_y}"
    );
    // 起跳当步的观测量 = 起跳初速扣掉一步重力
    let expected = JUMP_VELOCITY - GRAVITY * FIXED_DT;
    assert!(
        (max_vel_y - expected).abs() <= 1e-3,
        "起跳当步应为 {expected}，实际 {max_vel_y}"
    );
}

/// T9 单向平台：下方起跳可穿过、越过后下落能站住、站上后水平跑动不抖动。
#[test]
fn t9_one_way_platform() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = TestWorld::new(16, 16);
    world.fill_row(2, TileKind::OneWay);
    let platform_top = 3.0 * TILE;

    // (a) 从平台下方 1 瓦片处以 `vy = +JUMP_VELOCITY` 上升：不受阻挡，且速度与解析曲线一致
    let mut body = Body::at_center(Vec2::new(100.0, platform_top - TILE + PLAYER_SIZE.y * 0.5));
    body.vel.y = JUMP_VELOCITY;
    body.airborne_since = 7;
    let mut passed_above = false;
    for i in 1..=40u32 {
        let mut out = StepOutcome::default();
        // 起跳场景隐含"按住跳跃"：否则松手截断会把上升速度压到 JUMP_CUT_VELOCITY（§6.3.3）
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                jump_held: true,
                ..Default::default()
            },
            &mut out,
        );
        let expected_vel_y = JUMP_VELOCITY - GRAVITY * (i as f32 * FIXED_DT);
        if body.vel.y > 0.0 {
            assert!(
                (body.vel.y - expected_vel_y).abs() <= 1e-3,
                "第 {i} 步竖直速度 {} 与解析值 {expected_vel_y} 不符（被单向平台影响？）",
                body.vel.y
            );
        }
        if body.aabb.min.y > platform_top + 1.0 {
            passed_above = true;
        }
        if passed_above && body.grounded {
            break;
        }
    }
    assert!(passed_above, "上升期被单向平台挡住了");

    // (b) 落回后必须站在平台顶面
    assert!(body.grounded, "越过平台后应能站住");
    assert!((body.aabb.min.y - platform_top).abs() <= 1e-3);
    assert_eq!(body.vel.y, 0.0);

    // (c) 在平台上水平跑 60 步：不得被边缘卡住或抖动
    for i in 0..60 {
        let mut out = StepOutcome::default();
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: if i % 20 < 10 { 1 } else { -1 },
                ..Default::default()
            },
            &mut out,
        );
        assert!(
            (body.aabb.min.y - platform_top).abs() <= cfg.geom_eps + 1e-3,
            "第 {i} 步离开平台顶面：{}",
            body.aabb.min.y
        );
    }
}

/// T10 子步进防穿透（覆盖配置）：`max_fall_speed = 4000` ⇒ `n = 5`，仍不得穿模。
#[test]
fn t10_substepping_prevents_tunneling() {
    let cfg = PhysicsConfig {
        max_fall_speed: 4000.0,
        ..DEFAULT_PHYSICS
    };
    let world = flat_world();
    let floor_top = TILE;
    let mut body = Body::at_center(Vec2::new(100.0, floor_top + 200.0 + PLAYER_SIZE.y * 0.5));
    body.vel = Vec2::new(0.0, -cfg.max_fall_speed);
    body.airborne_since = 7;

    let displacement = cfg.max_fall_speed * cfg.fixed_dt;
    assert!(
        substep_count(&cfg, Vec2::new(0.0, displacement)) > 1,
        "覆盖配置应强制多子步"
    );

    for i in 0..40 {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert!(
            overlap_depth(&world, &body.aabb) <= cfg.geom_eps + 1e-3,
            "第 {i} 步穿透（子步进失效）"
        );
        if body.grounded {
            break;
        }
    }
    assert!((body.aabb.min.y - floor_top).abs() <= 1e-3);
}

/// T10b 子步数公式：`n = max(1, ceil(D_max / SUBSTEP_MAX_DISTANCE))`。
#[test]
fn t10b_substep_count_formula() {
    let cfg = DEFAULT_PHYSICS;
    for (distance, expect) in [
        (0.0, 1),
        (1.0, 1),
        (16.0, 1),
        (16.1, 2),
        (32.0, 2),
        (128.0, 8),
    ] {
        assert_eq!(
            substep_count(&cfg, Vec2::new(distance, 0.0)),
            expect,
            "D = {distance}"
        );
        assert_eq!(
            substep_count(&cfg, Vec2::new(0.0, -distance)),
            expect,
            "D = -{distance}"
        );
    }
}

/// T10b 的超限分支：位移超过常量表硬约束时 `debug_assert` 触发。
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "超过常量表硬约束")]
fn t10b_over_hard_cap_panics() {
    let cfg = DEFAULT_PHYSICS;
    let _ = substep_count(&cfg, Vec2::new(MAX_STEP_DISPLACEMENT + 1.0, 0.0));
}

/// T11 跳跃高度与公式一致：满跳 80.0 px、24 步到顶点。
#[test]
fn t11_jump_height_matches_formula() {
    let world = flat_world();
    let start = standing_on(100.0, TILE);
    let (peak, apex_step) = measure_jump(&world, &start, None);
    assert!(
        (peak - JUMP_HEIGHT_PX).abs() <= 0.1,
        "峰值上升 {peak}，期望 {JUMP_HEIGHT_PX}"
    );
    assert_eq!(
        apex_step,
        (JUMP_APEX_TIME as f64 * FIXED_HZ).round() as u32,
        "到达顶点所用步数"
    );
}

/// T12 松手截断的高度谱：全程按住 80.0；起跳当步松手 44.9；任意时机松手都落在区间内且不矮于 1 瓦片。
#[test]
fn t12_jump_cut_spectrum() {
    let world = flat_world();
    let start = standing_on(100.0, TILE);

    let (full, _) = measure_jump(&world, &start, None);
    assert!((full - JUMP_HEIGHT_PX).abs() <= 0.1, "满跳 = {full}");

    let (shortest, _) = measure_jump(&world, &start, Some(1));
    assert!(
        (shortest - 44.9).abs() <= 0.3,
        "最坏情况最小跳跃 = {shortest}，期望 44.9"
    );

    let mut previous = 0.0;
    for release in 1..=28 {
        let (peak, _) = measure_jump(&world, &start, Some(release));
        assert!(
            (44.8..=JUMP_HEIGHT_PX + 0.1).contains(&peak),
            "松手于第 {release} 步时峰值 {peak} 越界"
        );
        assert!(peak >= TILE, "轻点必过 1 格台阶（峰值 {peak}）");
        assert!(
            peak >= previous - 0.01,
            "松手越晚峰值不应变矮（第 {release} 步：{peak} < {previous}）"
        );
        previous = peak;
    }
}

// ---------- T13..T21 ----------

/// T13 终端速度：下落中 `vel.y` 不得低于 `-MAX_FALL_SPEED`，且 25 步内到达。
#[test]
fn t13_terminal_velocity() {
    let cfg = DEFAULT_PHYSICS;
    let world = TestWorld::new(8, 64);
    let mut body = Body::at_center(Vec2::new(64.0, 1000.0));
    body.airborne_since = 7;

    let mut reached_at = None;
    for i in 1..=200 {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert!(
            body.vel.y >= -MAX_FALL_SPEED - 1e-3,
            "第 {i} 步速度超过终端速度：{}",
            body.vel.y
        );
        if (body.vel.y + MAX_FALL_SPEED).abs() <= 1e-3 && reached_at.is_none() {
            reached_at = Some(i);
        }
        if body.grounded {
            break;
        }
    }
    let reached_at = reached_at.expect("应到达终端速度");
    let expected = (MAX_FALL_SPEED / (FALL_GRAVITY * FIXED_DT)).ceil() as u32;
    assert!(
        reached_at <= expected,
        "{reached_at} 步才到终端速度，期望 ≤ {expected}"
    );
}

/// T14 摩擦与加速时间：地面从满速到 0 需 ≤ 8 步；从静止到满速需 ≤ 10 步且不越界。
#[test]
fn t14_friction_and_accel() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();

    let mut body = standing_on(100.0, TILE);
    body.vel.x = MAX_RUN_SPEED;
    let stop_steps = (MAX_RUN_SPEED / (GROUND_FRICTION * FIXED_DT)).ceil() as u32;
    assert_eq!(stop_steps, 8);
    for i in 0..stop_steps {
        let mut out = StepOutcome::default();
        step(&cfg, &world, &mut body, StepInput::default(), &mut out);
        assert!(i < stop_steps - 1 || body.vel.x == 0.0, "第 {i} 步未停下");
    }
    assert_eq!(body.vel.x, 0.0);
    let mut out = StepOutcome::default();
    step(&cfg, &world, &mut body, StepInput::default(), &mut out);
    assert_eq!(body.vel.x, 0.0, "停下后不得再被摩擦推动");

    let mut body = standing_on(100.0, TILE);
    let accel_steps = (MAX_RUN_SPEED / (GROUND_ACCEL * FIXED_DT)).ceil() as u32;
    assert_eq!(accel_steps, 10);
    for i in 0..accel_steps {
        let mut out = StepOutcome::default();
        step(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 1,
                ..Default::default()
            },
            &mut out,
        );
        assert!(body.vel.x <= MAX_RUN_SPEED + 1e-3, "第 {i} 步超速");
    }
    assert!((body.vel.x - MAX_RUN_SPEED).abs() <= 1e-3);
}

/// T15 Hazard 传感器：内缩 `HAZARD_INSET` 之内不触发；超过则触发；两种情况都不阻挡。
#[test]
fn t15_hazard_sensor() {
    let cfg = DEFAULT_PHYSICS;
    let hazard_left = 6.0 * TILE;

    // 角色右边缘与尖刺左边缘重叠 1 px（< HAZARD_INSET ⇒ 不该死）
    let graze = Aabb::from_center_size(
        Vec2::new(hazard_left + 1.0 - PLAYER_SIZE.x * 0.5, 100.0),
        PLAYER_SIZE,
    );
    // 重叠 HAZARD_INSET + 1 px（⇒ 该死）
    let deep = Aabb::from_center_size(
        Vec2::new(
            hazard_left + HAZARD_INSET + 1.0 - PLAYER_SIZE.x * 0.5,
            100.0,
        ),
        PLAYER_SIZE,
    );

    let mut world = TestWorld::new(16, 16);
    assert!(
        !overlaps_tile_kind(&world, graze, -HAZARD_INSET, TileKind::Hazard),
        "无尖刺时不该触发"
    );
    world.set(6, 3, TileKind::Hazard);
    assert!(
        !overlaps_tile_kind(&world, graze, -HAZARD_INSET, TileKind::Hazard),
        "擦边 {HAZARD_INSET} 之内不该触发"
    );
    assert!(
        overlaps_tile_kind(&world, deep, -HAZARD_INSET, TileKind::Hazard),
        "深度重叠必须触发"
    );

    // 不阻挡：同一步下有无 Hazard，aabb 结果逐位相同
    let without = TestWorld::new(16, 16);
    let mut with = TestWorld::new(16, 16);
    with.set(6, 3, TileKind::Hazard);
    let mut a = Body::at_center(Vec2::new(hazard_left + 40.0, 120.0));
    let mut b = a;
    a.airborne_since = 7;
    b.airborne_since = 7;
    for _ in 0..30 {
        let mut out_a = StepOutcome::default();
        let mut out_b = StepOutcome::default();
        step(
            &cfg,
            &without,
            &mut a,
            StepInput {
                move_x: -1,
                ..Default::default()
            },
            &mut out_a,
        );
        step(
            &cfg,
            &with,
            &mut b,
            StepInput {
                move_x: -1,
                ..Default::default()
            },
            &mut out_b,
        );
    }
    assert_eq!(a.aabb, b.aabb, "Hazard 不得参与任何轴分离");
}

/// T16 外扩 / 内缩语义：`expand > 0` 更慷慨，`expand < 0` 更保守。
///
/// 几何：瓦片格 32 px 宽，因此「相距 gap」指角色右边缘到瓦片左面的距离。
#[test]
fn t16_expand_semantics() {
    let tile_left = 4.0 * TILE; // tx = 4 的瓦片左面
    let body_y = 100.0; // 落在 ty = 3 这一行（y ∈ [96,128)）内

    // (a) gap = PICKUP_INFLATE/2：原样不相交，外扩后相交
    let gap = PICKUP_INFLATE / 2.0;
    let mut near = TestWorld::new(16, 16);
    near.set(4, 3, TileKind::Hazard);
    let body = Aabb::from_center_size(
        Vec2::new(tile_left - gap - PLAYER_SIZE.x * 0.5, body_y),
        PLAYER_SIZE,
    );
    assert!(
        !overlaps_tile_kind(&near, body, 0.0, TileKind::Hazard),
        "相距 {gap} px 时原样不该相交"
    );
    assert!(
        overlaps_tile_kind(&near, body, PICKUP_INFLATE, TileKind::Hazard),
        "外扩 {PICKUP_INFLATE} px 后应相交（差一点也算吃到）"
    );

    // (b) gap = PICKUP_INFLATE × 4：即使外扩也不相交
    let far_gap = PICKUP_INFLATE * 4.0;
    let mut far = TestWorld::new(16, 16);
    far.set(4, 3, TileKind::Hazard);
    let body_far = Aabb::from_center_size(
        Vec2::new(tile_left - far_gap - PLAYER_SIZE.x * 0.5, body_y),
        PLAYER_SIZE,
    );
    assert!(
        !overlaps_tile_kind(&far, body_far, PICKUP_INFLATE, TileKind::Hazard),
        "相距 {far_gap} px 时外扩也不该相交"
    );

    // (c) 内缩必须比原样更保守，外扩必须比原样更宽松
    let mut touching = TestWorld::new(16, 16);
    touching.set(4, 3, TileKind::Hazard);
    let body_touch = Aabb::from_center_size(
        Vec2::new(tile_left + 1.0 - PLAYER_SIZE.x * 0.5, body_y),
        PLAYER_SIZE,
    );
    let raw = overlaps_tile_kind(&touching, body_touch, 0.0, TileKind::Hazard);
    assert!(raw, "重叠 1 px 时原样应相交");
    assert!(
        overlaps_tile_kind(&touching, body_touch, PICKUP_INFLATE, TileKind::Hazard),
        "外扩不得比原样更保守"
    );
    assert!(
        !overlaps_tile_kind(&touching, body_touch, -PICKUP_INFLATE, TileKind::Hazard),
        "内缩不得比原样更宽松"
    );
}

/// T17 确定性：同一初始状态 + 同一 300 步输入，两次运行的位模式完全相同。
#[test]
fn t17_deterministic() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = TestWorld::new(16, 16);
    world.fill_row(0, TileKind::Solid);
    world.fill_col(6, 0, 3, TileKind::Solid);
    world.fill_row(8, TileKind::OneWay);

    let start = Body::at_center(Vec2::new(80.0, TILE + PLAYER_SIZE.y * 0.5));
    let inputs: Vec<StepInput> = (0..300)
        .map(|i| StepInput {
            move_x: ((i / 17) % 3) as i8 - 1,
            jump_pressed: i % 43 == 0,
            jump_held: (i % 43) < 20,
        })
        .collect();

    let trace = |inputs: &[StepInput]| {
        let mut body = start;
        let mut bits = Vec::new();
        for input in inputs {
            let mut out = StepOutcome::default();
            step(&cfg, &world, &mut body, *input, &mut out);
            bits.push((
                body.aabb.min.x.to_bits(),
                body.aabb.min.y.to_bits(),
                body.vel.x.to_bits(),
                body.vel.y.to_bits(),
            ));
        }
        bits
    };
    assert_eq!(
        trace(&inputs),
        trace(&inputs),
        "两次运行的位模式必须完全相同"
    );
}

/// T18 面贴合不算重叠：脚底与顶面精确相等时不产生任何分离。
#[test]
fn t18_touching_is_not_overlap() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    // 脚底精确贴合：min.y == 32，速度为零
    let mut body = standing_on(100.0, TILE);
    let before = body.aabb.min.y;
    let mut out = StepOutcome::default();
    step(&cfg, &world, &mut body, StepInput::default(), &mut out);
    assert_eq!(body.aabb.min.y, before, "贴合面不得被推动");
    assert!(!out.head_bumped);
}

/// T19 边角擦过不产生水平推挤：`max.x` 与墙的左面精确相等时向下移动不得触墙。
#[test]
fn t19_corner_graze_no_horizontal_push() {
    let cfg = DEFAULT_PHYSICS;
    let mut world = TestWorld::new(16, 32);
    world.fill_col(8, 0, 31, TileKind::Solid);
    let wall_left = 8.0 * TILE;

    let mut body = Body::at_center(Vec2::new(wall_left - PLAYER_SIZE.x * 0.5, 400.0));
    body.vel = Vec2::new(0.0, -MAX_FALL_SPEED);
    body.airborne_since = 7;
    let before_min_x = body.aabb.min.x;
    let mut out = StepOutcome::default();
    step(&cfg, &world, &mut body, StepInput::default(), &mut out);
    assert_eq!(out.hit_wall_x, 0, "贴面下落不得产生水平推挤");
    assert_eq!(body.aabb.min.x, before_min_x, "水平位置不得改变");
}

/// T21 房间重置提供干净状态：出生后不得凭空获得一次空中起跳。
#[test]
fn t21_reset_gives_clean_state() {
    let cfg = DEFAULT_PHYSICS;
    let world = flat_world();
    let body = Body::at_center(Vec2::new(100.0, 200.0));
    assert_eq!(body.vel, Vec2::ZERO);
    assert!(!body.grounded);
    assert_eq!(body.buffer_age, u32::MAX);
    assert_eq!(body.airborne_since, COYOTE_STEPS + 1);

    let mut body = body;
    let mut out = StepOutcome::default();
    step(
        &cfg,
        &world,
        &mut body,
        StepInput {
            jump_pressed: true,
            jump_held: true,
            ..Default::default()
        },
        &mut out,
    );
    assert!(!out.jumped, "重置后第一步不得起跳");
}

// ---------- 玩家之间：实心盒（`06` §6.4.10）----------

/// 另一个玩家挡在右边：水平移动被夹在对方左面，速度被吸收。
#[test]
fn other_player_blocks_horizontal_movement() {
    let world = flat_world();
    let cfg = DEFAULT_PHYSICS;
    let mut body = Body::at_center(Vec2::new(100.0, TILE + PLAYER_SIZE.y / 2.0));
    let other = Aabb::from_center_size(Vec2::new(160.0, TILE + PLAYER_SIZE.y / 2.0), PLAYER_SIZE);
    let mut out = StepOutcome::default();
    for _ in 0..120 {
        step_with_players(
            &cfg,
            &world,
            &mut body,
            StepInput {
                move_x: 1,
                ..Default::default()
            },
            &[other],
            &mut out,
        );
    }
    assert!(
        body.aabb.max.x <= other.min.x + 1e-3,
        "不得穿过另一个玩家：max.x = {}，对方 min.x = {}",
        body.aabb.max.x,
        other.min.x
    );
    assert_eq!(out.hit_wall_x, 1);
    assert_box_invariant(&body, "撞到另一个玩家");
}

/// 站在另一个玩家头上：支撑探测必须把他当可站立面，且不穿过去。
#[test]
fn standing_on_another_player_grounds_the_body() {
    let world = flat_world();
    let cfg = DEFAULT_PHYSICS;
    // 对方站在地板上（脚底 = TILE），顶面 = TILE + 28。
    let other = Aabb::from_center_size(Vec2::new(160.0, TILE + PLAYER_SIZE.y / 2.0), PLAYER_SIZE);
    let top = other.max.y;
    let mut body = Body::at_center(Vec2::new(160.0, top + PLAYER_SIZE.y / 2.0 + 1.0));
    let mut out = StepOutcome::default();
    for _ in 0..60 {
        step_with_players(
            &cfg,
            &world,
            &mut body,
            StepInput::default(),
            &[other],
            &mut out,
        );
    }
    assert!(body.grounded, "站在另一个玩家头上必须算 grounded");
    assert!(
        (body.aabb.min.y - top).abs() < 1e-3,
        "脚底必须贴合对方顶面：{} vs {}",
        body.aabb.min.y,
        top
    );
    assert_box_invariant(&body, "站在另一个玩家头上");
}

/// 下方玩家跳起撞到上方玩家：撞头吸收上升速度，位置不重叠。
#[test]
fn other_player_blocks_jumping_through() {
    let world = flat_world();
    let cfg = DEFAULT_PHYSICS;
    let other = Aabb::from_center_size(
        Vec2::new(160.0, TILE + PLAYER_SIZE.y * 1.5 + 16.0),
        PLAYER_SIZE,
    );
    let mut body = Body::at_center(Vec2::new(160.0, TILE + PLAYER_SIZE.y / 2.0));
    let mut out = StepOutcome::default();
    // 起跳
    step_with_players(
        &cfg,
        &world,
        &mut body,
        StepInput {
            jump_pressed: true,
            jump_held: true,
            ..Default::default()
        },
        &[other],
        &mut out,
    );
    for _ in 0..120 {
        step_with_players(
            &cfg,
            &world,
            &mut body,
            StepInput {
                jump_held: true,
                ..Default::default()
            },
            &[other],
            &mut out,
        );
        assert!(
            body.aabb.max.y <= other.min.y + 1e-3,
            "不得穿过上方玩家：max.y = {}，对方 min.y = {}",
            body.aabb.max.y,
            other.min.y
        );
    }
    assert_box_invariant(&body, "撞到上方玩家");
}
