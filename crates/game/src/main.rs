//! `animj` 游戏可执行文件。
//!
//! 无参数启动进入**主菜单**（开始游戏 / 读取存档 / 设置 / 退出）。关卡选择扫描
//! `assets/levels/*.level.ron`（`05` §5.5）。
//!
//! 验证开关（`AGENTS.md` §4）：
//!
//! ```text
//! cargo run -p game --bin animj --features dev -- --frames 90 --screenshot target/m0
//! ```
//!
//! - `--frames N`：跑满 N 帧后退出（否则窗口一直开着，CI 无法收尾）。
//! - `--screenshot <prefix>`：请求两张截图 `<prefix>-canvas.png`（内层画布）与 `<prefix>-window.png`。
//! - `--self-check`：打印像素完美管线的不变量自检结果（`VERIFY:` 行）。
//! - `--hide-ui`：截图前关闭 UI 窗口相机（UI 不进入离屏画布，逐像素比较需要）。
//! - `--seconds N`：按墙钟秒数退出（联机验证要两个进程同时存活）。
//! - `--local-players 2`：本地双人；`--host`：开房；`--join <host:port>`：加入主机。
//! - `--demo menu|levels|settings|menu-start|load-save|click|online`：界面冒烟，从主菜单开始。
//! - `--demo walk|jump|hop|checkpoint|edge|pause|restart`：玩法冒烟，直接进默认关卡。
//!
//! **入口规则**：带任何验证开关或 `--demo`（菜单类除外）时用 `StartupFlow::Direct`
//! （`LoadMode::Auto`，与旧验证命令语义一致）；无参数启动才走主菜单。

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
    /// 按墙钟秒数退出（联机验证需要两个进程同时存活；`--frames` 在高帧率下会瞬间跑完）。
    seconds: Option<f32>,
    screenshot_prefix: Option<String>,
    self_check: bool,
    /// `--demo walk|jump`：注入脚本输入（M1 的「可实测」手段）。
    demo: Option<String>,
    /// 截图前关闭 UI 相机：让窗口截图只含 canvas（逐像素比较「窗口 == 画布 × k」）。
    hide_ui: bool,
    /// `--host`：直接以主机身份进入默认关卡。
    host: bool,
    /// `--join <host:port>`：直接以客户端身份加入。
    join: Option<std::net::SocketAddr>,
    /// `--local-players 2`：本地双人（同屏）。
    local_players: u8,
}

impl Cli {
    fn parse() -> Self {
        let mut cli = Cli::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => cli.frames = args.next().and_then(|v| v.parse().ok()),
                "--seconds" => cli.seconds = args.next().and_then(|v| v.parse().ok()),
                "--screenshot" => cli.screenshot_prefix = args.next(),
                "--demo" => cli.demo = args.next(),
                "--self-check" => cli.self_check = true,
                "--hide-ui" => cli.hide_ui = true,
                "--host" => cli.host = true,
                "--join" => {
                    cli.join = args.next().and_then(|value| match value.parse() {
                        Ok(addr) => Some(addr),
                        Err(_) => {
                            eprintln!("无效的 --join 地址：{value}（需要 host:port）");
                            None
                        }
                    })
                }
                "--local-players" => {
                    cli.local_players = args.next().and_then(|v| v.parse().ok()).unwrap_or(1)
                }
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

/// 冒烟：`--demo online` 进入联机页，验证局域网搜索链路。
fn drive_demo_online(
    tick: Res<FrameTick>,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut done: Local<bool>,
) {
    if *done || tick.0 < 20 {
        return;
    }
    *done = true;
    next.set(game::state::GameState::OnlineMenu);
    println!("[demo online] 已进入联机搜索");
}

/// 冒烟：`--demo levels` 在主菜单后进入关卡选择。
fn drive_demo_levels(
    tick: Res<FrameTick>,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut done: Local<bool>,
) {
    if *done || tick.0 < 20 {
        return;
    }
    *done = true;
    next.set(game::state::GameState::LevelSelect);
    println!("[demo levels] 已进入关卡选择");
}

/// 冒烟：`--demo settings` 打开设置，验证音量与全屏开关的投射链路。
fn drive_demo_settings(
    tick: Res<FrameTick>,
    mut settings: ResMut<game::settings::GameSettings>,
    mut return_state: ResMut<game::state::SettingsReturn>,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut stage: Local<u8>,
) {
    match (tick.0, *stage) {
        (20, 0) => {
            return_state.0 = game::state::GameState::MainMenu;
            next.set(game::state::GameState::Settings);
            *stage = 1;
            println!("[demo settings] 已打开设置");
        }
        (30, 1) => {
            settings.adjust_volume(-2);
            *stage = 2;
            println!("[demo settings] 音量 -> {}%", settings.volume_percent());
        }
        (36, 2) => {
            settings.fullscreen = true;
            *stage = 3;
            println!("[demo settings] 请求全屏");
        }
        (42, 3) => {
            settings.fullscreen = false;
            *stage = 4;
            println!("[demo settings] 请求窗口");
        }
        (48, 4) => {
            settings.toggle_language();
            *stage = 5;
            println!("[demo settings] 语言 -> {:?}", settings.language);
        }
        _ => {}
    }
}

/// 冒烟：`--demo menu-start` 走「主菜单 → 关卡选择 → 开始游戏」。
fn drive_demo_menu_start(
    tick: Res<FrameTick>,
    mut commands: Commands,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut stage: Local<u8>,
) {
    match (tick.0, *stage) {
        (20, 0) => {
            next.set(game::state::GameState::LevelSelect);
            *stage = 1;
            println!("[demo menu-start] 已进入关卡选择");
        }
        (30, 1) => {
            commands.insert_resource(game::world::load::StartRequest {
                level_id: game::world::load::DEFAULT_LEVEL_ID.to_string(),
                mode: game::world::load::LoadMode::NewGame,
            });
            next.set(game::state::GameState::Loading);
            *stage = 2;
            println!("[demo menu-start] 已请求开始游戏");
        }
        _ => {}
    }
}

fn log_playing_on_enter() {
    println!("[demo menu-start] 已进入游戏");
}

/// 冒烟：`--demo load-save` 走「主菜单 → 读取存档」。与菜单按钮走同一条 `StartRequest` 路径。
fn drive_demo_load_save(
    tick: Res<FrameTick>,
    mut commands: Commands,
    mut next: ResMut<NextState<game::state::GameState>>,
    mut done: Local<bool>,
) {
    if *done || tick.0 < 20 {
        return;
    }
    *done = true;
    match game::save::load_raw().filter(|data| data.version == game::save::SAVE_VERSION) {
        Some(data) => {
            println!("[demo load-save] 读取存档：{}", data.level_name);
            commands.insert_resource(game::world::load::StartRequest {
                level_id: data.level_name,
                mode: game::world::load::LoadMode::Continue,
            });
            next.set(game::state::GameState::Loading);
        }
        None => println!("[demo load-save] 没有可用存档"),
    }
}

/// 冒烟：验证「游戏中 → 主菜单 → 再开一局」不会残留上一局的实体。
///
/// 第二轮进入 `Playing` 时打印实体计数：玩家必须恰好 1 个、房间根必须等于关卡房间数。
fn drive_demo_restart(
    tick: Res<FrameTick>,
    mut commands: Commands,
    mut next: ResMut<NextState<game::state::GameState>>,
) {
    match tick.0 {
        40 => {
            println!("[demo restart] 回到主菜单");
            next.set(game::state::GameState::MainMenu);
        }
        60 => {
            commands.insert_resource(game::world::load::StartRequest {
                level_id: game::world::load::DEFAULT_LEVEL_ID.to_string(),
                mode: game::world::load::LoadMode::NewGame,
            });
            next.set(game::state::GameState::Loading);
            println!("[demo restart] 再次开始游戏");
        }
        _ => {}
    }
}

/// 第二轮进入 `Playing` 后（瓦片已生成）检查残留：玩家与房间根都不得翻倍。
fn log_restart_counts(
    state: Res<State<game::state::GameState>>,
    players: Query<&game::player::PlayerMotion>,
    roots: Query<&game::render::tiles::RoomRoot>,
    zones: Query<&game::room::RoomZone>,
    mut rounds: Local<u8>,
    mut last_state: Local<Option<game::state::GameState>>,
    mut logged: Local<bool>,
) {
    if *last_state != Some(*state.get()) {
        if *state.get() == game::state::GameState::Playing {
            *rounds += 1;
        }
        *last_state = Some(*state.get());
    }
    if *rounds < 2 || *logged || roots.is_empty() {
        return;
    }
    *logged = true;
    println!(
        "[demo restart] 第二轮实体计数：玩家 {}，房间根 {}，房间实体 {}",
        players.iter().count(),
        roots.iter().count(),
        zones.iter().count()
    );
}

/// 冒烟：`--demo click` 用真实的 `PointerInput` 消息点一次「START GAME」。
///
/// 这条路径与鼠标完全一致：`PointerInput` → 拾取后端 → `PointerPress` observer → `Pressed`
/// → 菜单按钮系统。UI 若仍挂在内层离屏相机上，拾取后端找不到同目标的指针，本 demo 会停在
/// 主菜单并超时（这正是「GUI 点不动」的根因回归测试）。
#[allow(clippy::too_many_arguments)]
fn drive_demo_click(
    tick: Res<FrameTick>,
    state: Res<State<game::state::GameState>>,
    texts: Query<(&Text, &bevy::ui::UiGlobalTransform, &bevy::ui::ComputedNode)>,
    ui_camera: Single<&Camera, With<game::render::pixel_camera::UiCamera>>,
    window: Single<Entity, With<bevy::window::PrimaryWindow>>,
    mut pointer: MessageWriter<bevy::picking::pointer::PointerInput>,
    mut position: Local<Vec2>,
    mut stage: Local<u8>,
) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
    use bevy::window::WindowRef;

    let Some(target) = WindowRef::Primary.normalize(Some(*window)) else {
        return;
    };
    match (tick.0, *stage) {
        (28, 0) if *state.get() == game::state::GameState::MainMenu => {
            let Some((text, transform, _)) = texts
                .iter()
                .find(|(text, ..)| matches!(text.0.as_str(), "开始游戏" | "START GAME"))
            else {
                return;
            };
            // `UiGlobalTransform` 以 UI 视口左上角为原点；指针位置是**窗口**物理坐标。
            // 全屏时 UI 视口居中（有黑边），两者相差视口偏移。
            let viewport_origin = ui_camera
                .viewport
                .as_ref()
                .map(|viewport| viewport.physical_position.as_vec2())
                .unwrap_or(Vec2::ZERO);
            *position = transform.translation + viewport_origin;
            // 指针位置由 `Move` 更新；`Press` 只改按钮状态，不携带位置。
            pointer.write(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::Window(target),
                    position: *position,
                },
                PointerAction::Move { delta: Vec2::ZERO },
            ));
            *stage = 1;
            println!(
                "[demo click] 指针移动到 {} @ ({:.0}, {:.0})",
                text.0, position.x, position.y
            );
        }
        (30, 1) => {
            pointer.write(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::Window(target),
                    position: *position,
                },
                PointerAction::Press(PointerButton::Primary),
            ));
            *stage = 2;
            println!("[demo click] 按下左键");
        }
        (32, 2) => {
            pointer.write(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::Window(target),
                    position: *position,
                },
                PointerAction::Release(PointerButton::Primary),
            ));
            *stage = 3;
        }
        (40, 3) if *state.get() == game::state::GameState::LevelSelect => {
            *stage = 4;
            println!("[demo click] 点击生效：已进入关卡选择");
        }
        (80, 3) => {
            *stage = 4;
            println!("[demo click] 失败：仍停留在主菜单（拾取未命中）");
        }
        _ => {}
    }
}

/// 冒烟专用：进入游戏后立刻打开暂停菜单（正常入口是 `Esc`）。
fn open_pause_for_demo(
    mut menu: ResMut<game::render::pause::PauseMenu>,
    mut next: ResMut<NextState<game::state::GameState>>,
) {
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

    let menu_demo = matches!(
        cli.demo.as_deref(),
        Some("menu" | "levels" | "settings" | "menu-start" | "load-save" | "click" | "online")
    );
    let headless = cli.frames.is_some()
        || cli.seconds.is_some()
        || cli.screenshot_prefix.is_some()
        || cli.self_check;
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
            // 冒烟：进入 Playing 后立刻打开暂停菜单（键盘 `Esc` 无法在无头会话注入）
            app.add_systems(
                OnEnter(game::state::GameState::Playing),
                open_pause_for_demo,
            );
        }
        Some("levels") => {
            app.add_systems(Update, drive_demo_levels);
        }
        Some("online") => {
            app.add_systems(Update, drive_demo_online);
        }
        Some("settings") => {
            app.add_systems(Update, drive_demo_settings);
        }
        Some("menu-start") => {
            app.add_systems(Update, drive_demo_menu_start);
            app.add_systems(
                OnEnter(game::state::GameState::Playing),
                log_playing_on_enter,
            );
        }
        Some("load-save") => {
            app.add_systems(Update, drive_demo_load_save);
        }
        Some("click") => {
            app.add_systems(Update, drive_demo_click);
        }
        Some("restart") => {
            app.add_systems(Update, (drive_demo_restart, log_restart_counts));
        }
        Some("menu") => {}
        Some(other) => eprintln!(
            "未知 demo：{other}（可选 walk|jump|edge|hop|checkpoint|pause|menu|levels|settings|menu-start|load-save|restart|click|online）"
        ),
        None => {}
    }

    // 会话模式：CLI 直接指定；菜单会覆盖它（主菜单/联机页）。
    let session = if let Some(server) = cli.join {
        game::state::SessionMode::Client { server }
    } else if cli.host {
        game::state::SessionMode::Host
    } else if cli.local_players >= 2 {
        game::state::SessionMode::LocalCoop
    } else {
        game::state::SessionMode::Solo
    };
    println!("会话模式：{session:?}");
    app.insert_resource(session);

    // 验证/脚本入口沿用旧语义：直接进默认关卡（Auto = 有存档读档，没有就新开）。
    // 无参数启动才是玩家入口，从主菜单开始。
    let session_direct = cli.host || cli.join.is_some() || cli.local_players >= 2;
    if !menu_demo && (headless || cli.demo.is_some() || session_direct) {
        app.insert_resource(game::state::StartupFlow::Direct);
    }

    app.insert_resource(cli)
        .init_resource::<FrameTick>()
        .add_systems(First, tick_frames)
        .add_systems(Update, (capture_screenshots, exit_after_frames).chain());
    // 菜单界面没有关卡，像素完美自检不适用；普通交互启动也不该往控制台刷 FAIL。
    if headless && !menu_demo {
        // 自检放在 `PostUpdate` 的像素吸附**之后**：`sync_player_transform` 在 `GameSet::Snap` 内写
        // `Transform`，若在 `Update` 里比较，AABB 已前进而 Transform 还是上一帧的投影 ⇒ 假阴性。
        app.add_systems(PostUpdate, self_check.after(game::state::GameSet::Snap));
    }
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
    ui_camera: Option<Single<&mut Camera, With<game::render::pixel_camera::UiCamera>>>,
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
    // UI 现在渲染在原生窗口上（不进入离屏画布）。做管线逐像素验证时显式关掉 UI 相机，
    // 否则窗口里多出的 HUD/菜单必然与画布不同。
    if cli.hide_ui
        && tick.0 + 2 >= cli.check_frame()
        && let Some(mut camera) = ui_camera
        && camera.is_active
    {
        camera.is_active = false;
        println!("截图前已关闭 UI 相机（逐像素比较只含游戏画面）");
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
        Has<game::render::pixel_camera::UiCamera>,
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

    // ① 离屏渲染目标尺寸 = 内层视口
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

    // ② 三台相机（内层/外层/UI）都显式关了 MSAA（Camera 默认带 Msaa::Sample4）
    let camera_count = cameras.iter().count();
    let msaa_off = cameras
        .iter()
        .filter(|(msaa, ..)| **msaa == Msaa::Off)
        .count();
    let ui_cameras = cameras.iter().filter(|(.., is_ui)| *is_ui).count();
    check(
        "3 cameras (inner/outer/ui) Msaa::Off",
        camera_count == 3 && msaa_off == 3 && ui_cameras == 1,
    );

    // ③ 外层缩放严格等于 1 / k
    let k = canvas_scale.0;
    let outer_scale_ok = cameras.iter().any(|(_, _, projection, _, is_outer, ..)| {
        is_outer
            && k >= 1
            && matches!(projection, Projection::Orthographic(p) if (p.scale - 1.0 / k as f32).abs() < 1e-6)
    });
    check("outer scale == 1/k", outer_scale_ok);

    // ④ 内层相机落在整像素上（相机抖 1 像素会让整屏内容抖）
    let inner_integral = cameras
        .iter()
        .filter(|(_, t, _, is_inner, ..)| {
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
    let projection_ok = !players.is_empty()
        && players.iter().all(|(motion, transform)| {
            transform.is_some_and(|transform| {
                let aabb = motion.0.aabb;
                (transform.translation.x - aabb.center().x.round()).abs() < 0.001
                    && (transform.translation.y - aabb.min.y.round()).abs() < 0.001
            })
        });
    check("player transform == aabb", projection_ok);

    // ⑩ 内层相机跟随**所有玩家 AABB 中心的中点**，并被钳制在地图内（`02` §2.2）
    let midpoint = if players.is_empty() {
        None
    } else {
        let sum = players
            .iter()
            .fold(Vec2::ZERO, |sum, (motion, _)| sum + motion.0.aabb.center());
        Some(sum / players.len() as f32)
    };
    let expected_camera = midpoint.and_then(|center| {
        loaded.as_ref().map(|loaded| {
            game::camera::rig::follow_target(center, loaded.level.grid.cols, loaded.level.grid.rows)
        })
    });
    let camera_follows = expected_camera.is_some_and(|expected| {
        cameras.iter().any(|(_, transform, _, is_inner, ..)| {
            is_inner
                && transform.translation.x == game::render::snap::snap_px(expected.x)
                && transform.translation.y == game::render::snap::snap_px(expected.y)
        })
    });
    check("camera follows player", camera_follows);

    // ⑪ 角色碰撞盒尺寸不变形（R1–R5：反向/压扁的 AABB 会静默关闭全部碰撞）
    let box_ok = !players.is_empty()
        && players.iter().all(|(motion, _)| {
            (motion.0.aabb.size() - game::config::physics::PLAYER_SIZE).length() <= 1e-3
        });
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

fn exit_after_frames(
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(seconds) = cli.seconds
        && time.elapsed_secs() >= seconds
    {
        println!("已运行 {seconds:.1} 秒，退出");
        exit.write(AppExit::Success);
        return;
    }
    if let Some(frames) = cli.frames
        && tick.0 >= frames
    {
        println!("已跑满 {frames} 帧，退出");
        exit.write(AppExit::Success);
    }
}
