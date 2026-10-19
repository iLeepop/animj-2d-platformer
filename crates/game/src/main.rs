//! `animj` 游戏可执行文件。
//!
//! 除正常游玩外，支持三个**用于验证**的开关（M0 的验收依赖它们，见 `docs/09-dependencies.md` §9.8）：
//!
//! ```text
//! cargo run -p game --bin animj --features dev -- --frames 90 --screenshot target/m0
//! ```
//!
//! - `--frames N`：跑满 N 帧后退出（否则窗口一直开着，CI 无法收尾）。
//! - `--screenshot <prefix>`：请求两张截图 `<prefix>-canvas.png`（内层画布）与 `<prefix>-window.png`。
//! - `--self-check`：打印像素完美管线的不变量自检结果（`VERIFY:` 行）。

use bevy::image::{ImageFilterMode, ImageSampler};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use game::config::render::{VIEWPORT_H, VIEWPORT_W};
use game::render::pixel_camera::{CanvasImage, CanvasScale, InGameCamera, OuterCamera};
use game::render::snap::PixelSnap;
use game::render::tiles::Tileset;
use game::tiles::LevelTiles;
use game::world::CurrentRoom;
use game::world::load::LoadedLevel;

#[derive(Resource, Clone, Default)]
struct Cli {
    frames: Option<u32>,
    screenshot_prefix: Option<String>,
    self_check: bool,
    /// `--demo walk|jump`：注入脚本输入（M1 的「可实测」手段）。
    demo: Option<String>,
}

impl Cli {
    fn parse() -> Self {
        let mut cli = Cli::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => cli.frames = args.next().and_then(|v| v.parse().ok()),
                "--screenshot" => cli.screenshot_prefix = args.next(),
                "--demo" => cli.demo = args.next(),
                "--self-check" => cli.self_check = true,
                other => eprintln!("忽略未知参数：{other}"),
            }
        }
        cli
    }

    /// 截图与自检发生在总帧数的一半处（此时关卡已生成并渲染过若干帧）。
    fn check_frame(&self) -> u32 {
        (self.frames.unwrap_or(120) / 2).max(10)
    }
}

/// 本进程的帧计数（不依赖 `FrameCountPlugin` 是否在插件组里）。
#[derive(Resource, Default)]
struct FrameTick(u32);

/// 冒烟专用：让暂停菜单在固定帧打开（正常入口是 `Esc`）。
fn open_pause_for_demo(
    tick: Res<FrameTick>,
    mut menu: ResMut<game::render::pause::PauseMenu>,
    mut next: ResMut<NextState<game::state::GameState>>,
) {
    if tick.0 != 20 {
        return;
    }
    menu.open = true;
    next.set(game::state::GameState::Paused);
    println!("[demo pause] 已打开暂停菜单");
}

fn main() {
    let cli = Cli::parse();
    println!("animj 启动：assets = {}", game::assets::asset_file_path());

    let mut app = App::new();
    app.add_plugins(game::default_plugins());
    game::add_game_plugins(&mut app);
    game::add_render_plugins(&mut app);
    match cli.demo.as_deref() {
        Some("walk") => {
            app.add_plugins(game::demo::DemoPlugin {
                script: game::demo::DemoScript::walk(),
            });
        }
        Some("jump") => {
            app.add_plugins(game::demo::DemoPlugin {
                script: game::demo::DemoScript::jump(),
            });
        }
        Some("hop") => {
            app.add_plugins(game::demo::DemoPlugin {
                script: game::demo::DemoScript::hop(),
            });
        }
        Some("checkpoint") => {
            app.add_plugins(game::demo::DemoPlugin {
                script: game::demo::DemoScript::checkpoint(),
            });
        }
        Some("edge") => {
            app.add_plugins(game::demo::DemoPlugin {
                script: game::demo::DemoScript::edge(),
            });
        }
        Some("pause") => {
            // 冒烟：第 20 帧打开暂停菜单（键盘 `Esc` 无法在无头会话注入）
            app.add_systems(Update, open_pause_for_demo);
        }
        Some(other) => eprintln!("未知 demo：{other}（可选 walk|jump|edge|hop|checkpoint|pause）"),
        None => {}
    }
    app.insert_resource(cli)
        .init_resource::<FrameTick>()
        .add_systems(First, tick_frames)
        .add_systems(Update, (capture_screenshots, exit_after_frames).chain())
        // 自检放在 `PostUpdate` 的像素吸附**之后**：`sync_player_transform` 在 `GameSet::Snap` 内写
        // `Transform`，若在 `Update` 里比较，AABB 已前进而 Transform 还是上一帧的投影 ⇒ 假阴性。
        .add_systems(PostUpdate, self_check.after(game::state::GameSet::Snap));
    app.run();
}

fn tick_frames(mut tick: ResMut<FrameTick>) {
    tick.0 = tick.0.saturating_add(1);
}

fn capture_screenshots(
    mut commands: Commands,
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    canvas: Option<Res<CanvasImage>>,
    debug_draw: Option<ResMut<game::render::debug::DebugDrawEnabled>>,
    mut done: Local<bool>,
) {
    let Some(prefix) = cli.screenshot_prefix.as_deref() else {
        return;
    };

    // 提前两帧关掉调试绘制：FPS 文本每帧都在变，两次截图会差一帧，
    // 从而在「窗口 == 画布 × k」的逐像素比较里制造假阳性（实测 44 个像素）。
    if tick.0 + 2 >= cli.check_frame()
        && let Some(mut debug_draw) = debug_draw
        && debug_draw.0
    {
        debug_draw.0 = false;
        println!("截图前已关闭调试绘制（避免动态 FPS 文本污染逐像素比较）");
    }

    if *done || tick.0 < cli.check_frame() {
        return;
    }
    if let Some(canvas) = canvas {
        commands
            .spawn(Screenshot::image(canvas.0.clone()))
            .observe(save_to_disk(format!("{prefix}-canvas.png")));
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{prefix}-window.png")));
    *done = true;
    println!("已请求截图：{prefix}-canvas.png / {prefix}-window.png");
}

/// 自检用的相机查询类型别名（避免系统签名里出现过长的元组类型）。
type CameraQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Msaa,
        &'static Transform,
        &'static Projection,
        Has<InGameCamera>,
        Has<OuterCamera>,
    ),
>;

/// 像素完美管线的不变量自检。
///
/// 这些条件是「无插值模糊」的**充分条件**：最近邻采样 + 整数倍缩放 + 所有渲染位置为整像素。
/// 逐条机器校验比肉眼看截图可靠。
#[allow(clippy::too_many_arguments)]
fn self_check(
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    mut done: Local<bool>,
    canvas: Option<Res<CanvasImage>>,
    images: Res<Assets<Image>>,
    canvas_scale: Res<CanvasScale>,
    tileset: Option<Res<Tileset>>,
    cameras: CameraQuery,
    snap_targets: Query<&Transform, With<PixelSnap>>,
    sprites: Query<&Sprite, Without<game::player::PlayerMotion>>,
    tiles: Option<Res<LevelTiles>>,
    loaded: Option<Res<LoadedLevel>>,
    current: Option<Res<CurrentRoom>>,
    player_q: Query<(&game::player::PlayerMotion, Option<&Transform>)>,
) {
    if *done || tick.0 < cli.check_frame() {
        return;
    }
    *done = true;

    let mut failures: Vec<String> = Vec::new();
    let mut check = |name: &str, ok: bool| {
        println!("VERIFY: {:<32} {}", name, if ok { "ok" } else { "FAIL" });
        if !ok {
            failures.push(name.to_string());
        }
    };

    // ① 离屏渲染目标尺寸 = 一个房间
    let canvas_size = canvas
        .as_ref()
        .and_then(|handle| images.get(&handle.0))
        .map(|image| {
            (
                image.texture_descriptor.size.width,
                image.texture_descriptor.size.height,
            )
        });
    check(
        "canvas size == viewport",
        canvas_size == Some((VIEWPORT_W, VIEWPORT_H)),
    );

    // ② 两个相机都显式关了 MSAA（Camera 默认带 Msaa::Sample4）
    let camera_count = cameras.iter().count();
    let msaa_off = cameras
        .iter()
        .filter(|(msaa, ..)| **msaa == Msaa::Off)
        .count();
    check("both cameras Msaa::Off", camera_count == 2 && msaa_off == 2);

    // ③ 外层缩放严格等于 1 / k
    let k = canvas_scale.0;
    let outer_scale_ok = cameras.iter().any(|(_, _, projection, _, is_outer)| {
        is_outer
            && k >= 1
            && matches!(projection, Projection::Orthographic(p) if (p.scale - 1.0 / k as f32).abs() < 1e-6)
    });
    check("outer scale == 1/k", outer_scale_ok);

    // ④ 内层相机落在整像素上（相机抖 1 像素会让整屏内容抖）
    let inner_integral = cameras
        .iter()
        .filter(|(_, t, _, is_inner, _)| {
            *is_inner && t.translation.x.fract() == 0.0 && t.translation.y.fract() == 0.0
        })
        .count();
    check("inner camera at integer px", inner_integral == 1);

    // ⑤ 所有需吸附的实体都在整像素上
    let non_integral = snap_targets
        .iter()
        .filter(|t| t.translation.x.fract() != 0.0 || t.translation.y.fract() != 0.0)
        .count();
    check("all PixelSnap at integer px", non_integral == 0);

    // ⑥ 贴图采样器**不得**是显式线性采样。
    //    注意 `ImageSampler::Default` 的语义是「用 `ImagePlugin.default_sampler`」——官方原文：
    //    "The default image sampler to use when `ImageSampler` is set to `Default`."
    //    （<https://docs.rs/bevy/0.20.0/bevy/image/struct.ImagePlugin.html#structfield.default_sampler>）
    //    因此加载出来的 PNG 通常就是 `Default`；「真的是最近邻」由截图逐像素比较来证明（见 §9.8 的验收命令）。
    let sampler_ok = tileset
        .as_ref()
        .and_then(|tileset| images.get(&tileset.image))
        .map(|image| match &image.sampler {
            ImageSampler::Default => true,
            ImageSampler::Descriptor(desc) => desc.mag_filter == ImageFilterMode::Nearest,
        });
    check("tileset sampler not linear", sampler_ok == Some(true));

    // ⑦ 关卡瓦片数据与渲染实体数量一致（数据 ↔ 渲染的端到端一致性）
    let expected_tiles = loaded.as_ref().map(|loaded| {
        let level = &loaded.level;
        let mut count = 0usize;
        for room in &level.rooms {
            for ty in 0..level_format::ROOM_TILES {
                for tx in 0..level_format::ROOM_TILES {
                    // v2：可渲染性由**关卡调色板**决定（字符 → cell），不再查内置 legend
                    let ch = room.tiles.get(ty).and_then(|line| line.chars().nth(tx));
                    if ch.is_some_and(|ch| level.cell_of(ch).is_some()) {
                        count += 1;
                    }
                }
            }
        }
        count
    });
    let actual_tiles = sprites
        .iter()
        .filter(|sprite| sprite.texture_atlas.is_some())
        .count();
    check(
        &format!("tile sprites == level data ({expected_tiles:?} vs {actual_tiles})"),
        expected_tiles == Some(actual_tiles) && actual_tiles > 0,
    );

    // ⑧ 关卡已加载、瓦片已生成，且当前房间**在地图范围内**
    //    （不写死 (0,0)：M2 起当前房间会随玩家跨界变化，见 `02` §2.2）
    let room_in_bounds = loaded
        .as_ref()
        .zip(current.as_ref())
        .is_some_and(|(loaded, current)| {
            current.col < loaded.level.grid.cols && current.row < loaded.level.grid.rows
        });
    check(
        "level loaded + room in bounds",
        loaded.is_some()
            && tiles.is_some()
            && loaded.as_ref().map(|l| l.level.name.as_str())
                == Some(game::world::load::DEFAULT_LEVEL_ID)
            && room_in_bounds,
    );

    // ⑨ 角色存在，且渲染投影 == `Aabb` 的单向投影（`06` §6.8.2）
    let players: Vec<_> = player_q.iter().collect();
    let projection_ok = players.len() == 1
        && players[0].1.is_some_and(|transform| {
            let aabb = players[0].0.0.aabb;
            (transform.translation.x - aabb.center().x.round()).abs() < 0.001
                && (transform.translation.y - aabb.min.y.round()).abs() < 0.001
        });
    check("player transform == aabb", projection_ok);

    // ⑩ 内层相机跟随玩家 AABB 中心，并被钳制在地图内（`02` §2.2）
    let expected_camera = players.first().and_then(|(motion, _)| {
        loaded.as_ref().map(|loaded| {
            game::camera::rig::follow_target(
                motion.0.aabb.center(),
                loaded.level.grid.cols,
                loaded.level.grid.rows,
            )
        })
    });
    let camera_follows = expected_camera.is_some_and(|expected| {
        cameras.iter().any(|(_, transform, _, is_inner, _)| {
            is_inner
                && transform.translation.x == game::render::snap::snap_px(expected.x)
                && transform.translation.y == game::render::snap::snap_px(expected.y)
        })
    });
    check("camera follows player", camera_follows);

    // ⑪ 角色碰撞盒尺寸不变形（R1–R5：反向/压扁的 AABB 会静默关闭全部碰撞）
    let box_ok = players.len() == 1
        && (players[0].0.0.aabb.size() - game::config::physics::PLAYER_SIZE).length() <= 1e-3;
    check("player collider size", box_ok);

    if failures.is_empty() {
        println!("VERIFY: PASS");
    } else {
        println!(
            "VERIFY: FAIL {} 项：{}",
            failures.len(),
            failures.join(", ")
        );
    }
}

fn exit_after_frames(cli: Res<Cli>, tick: Res<FrameTick>, mut exit: MessageWriter<AppExit>) {
    if let Some(frames) = cli.frames
        && tick.0 >= frames
    {
        println!("已跑满 {frames} 帧，退出");
        exit.write(AppExit::Success);
    }
}
