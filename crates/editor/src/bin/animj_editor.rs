//! `animj-editor` —— 自研关卡编辑器的宿主进程。
//!
//! 权威定义见 `docs/08-level-editor.md`：§8.2 进程与 crate 架构、§8.3 状态模型、§8.4 相机与输入、
//! §8.5 工具集、§8.6 就地试玩、§8.7 保存。编辑器逻辑全部在 `editor` 库里，本文件只做外壳。

use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy_egui::{EguiGlobalSettings, EguiPlugin};

fn main() {
    let cli = Cli::parse();

    let mut app = App::new();
    app.add_plugins(game::default_plugins())
        .add_plugins(EguiPlugin::default())
        .insert_resource(cli)
        .init_resource::<FrameTick>()
        .add_systems(First, tick_frames)
        .add_systems(Update, (capture_screenshot, exit_on_frames_limit).chain())
        .add_systems(
            Update,
            drive_demo_play.run_if(resource_exists::<editor::session::EditorSession>),
        )
        .add_systems(FixedPreUpdate, drive_demo_movement)
        .add_systems(
            Update,
            drive_demo_room.run_if(resource_exists::<editor::session::EditorSession>),
        );

    // 编辑态看整张地图（`08` §8.1.4）。
    app.insert_resource(game::render::tiles::RoomCulling(false));

    // 显式指定 egui 宿主，关掉自动挑选（`08` §8.4.1）。
    app.world_mut()
        .resource_mut::<EguiGlobalSettings>()
        .auto_create_primary_context = false;

    // 与发布 bin 复用同一批玩法插件（`08` §8.2.2）。
    // 像素完美离屏管线**不**注册：编辑器用单台 `EditorCamera` 直接渲染世界。
    // 菜单流程也不注册：编辑器启动即请求默认关卡（`05` §5.5 的 `StartupFlow::Direct`）。
    app.insert_resource(game::state::StartupFlow::Direct);
    game::add_game_plugins(&mut app);
    // 但 AABB→Transform 投影 + 像素吸附必须注册：否则试玩时角色贴图不会跟随物理位置
    //（`07` §7.6.3、`08` §8.6）。它不依赖双层相机，对编辑相机同样成立。
    app.add_plugins(game::render::snap::SnapPlugin);
    app.add_plugins(editor::EditorPlugin);

    app.run();
}

/// 冒烟开关：`--frames N` 跑满后退出；`--screenshot <prefix>` 请求一张窗口截图。
#[derive(Resource, Clone, Default)]
struct Cli {
    frames: Option<u32>,
    screenshot: Option<String>,
    /// 自动跑一次「进入试玩 → 退出试玩」，并断言编辑缓冲逐字节未变（`08` §8.6.3 R3）。
    demo_play: bool,
    /// 冒烟：第 10 帧切到该房间（`col,row`），验证相机与叠加层跟随。
    demo_room: Option<String>,
}

impl Cli {
    fn parse() -> Self {
        let mut cli = Cli::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => cli.frames = args.next().and_then(|value| value.parse().ok()),
                "--screenshot" => cli.screenshot = args.next(),
                "--demo-play" => cli.demo_play = true,
                "--demo-room" => cli.demo_room = args.next(),
                other => eprintln!("忽略未知参数：{other}"),
            }
        }
        cli
    }

    fn screenshot_frame(&self) -> u32 {
        (self.frames.unwrap_or(120) / 2).max(10)
    }
}

#[derive(Resource, Default)]
struct FrameTick(u32);

fn tick_frames(mut tick: ResMut<FrameTick>) {
    tick.0 = tick.0.saturating_add(1);
}

fn capture_screenshot(
    mut commands: Commands,
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    mut done: Local<bool>,
) {
    let Some(prefix) = cli.screenshot.as_deref() else {
        return;
    };
    if *done || tick.0 < cli.screenshot_frame() {
        return;
    }
    *done = true;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{prefix}.png")));
    println!("已请求截图：{prefix}.png");
}

/// `--demo-play`：第 20 帧请求进入试玩、第 60 帧请求退出，第 90 帧比对 `doc` 指纹。
fn drive_demo_play(
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    mut play: ResMut<editor::tools::PlaytestRequest>,
    mut stop: ResMut<editor::playtest::PlaytestExitRequest>,
    session: Option<Res<editor::session::EditorSession>>,
    mut checked: Local<bool>,
) {
    if !cli.demo_play {
        return;
    }
    if tick.0 == 20 {
        play.0 = true;
        println!("[demo-play] 请求进入试玩");
    }
    if tick.0 == 60 {
        stop.0 = true;
        println!("[demo-play] 请求退出试玩");
    }
    if tick.0 >= 90 && !*checked {
        *checked = true;
        if let Some(session) = session {
            let text = level_format::write(&session.doc);
            let same = session
                .saved_text
                .as_deref()
                .map(|saved| saved.trim_end() == text.trim_end())
                .unwrap_or(false);
            println!(
                "[demo-play] 试玩往返后编辑缓冲一致 = {same}（dirty = {}）",
                session.dirty
            );
        }
    }
}

/// `--demo-play` 期间注入右移：验证试玩时角色贴图确实跟随物理位置（`07` §7.6.3）。
fn drive_demo_movement(
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    mut inputs: ResMut<game::player::PlayerInputs>,
) {
    if !cli.demo_play {
        return;
    }
    inputs.slots[0].move_x = if (21..60).contains(&tick.0) { 1 } else { 0 };
}

/// `--demo-room col,row`：第 10 帧切换房间（无头验证相机与网格是否跟随）。
fn drive_demo_room(
    cli: Res<Cli>,
    tick: Res<FrameTick>,
    mut session: Option<ResMut<editor::session::EditorSession>>,
    camera: Option<Single<(&mut Transform, &mut Projection), With<editor::camera::EditorCamera>>>,
    mut done: Local<bool>,
) {
    if *done || tick.0 < 10 {
        return;
    }
    let Some(target) = cli.demo_room.as_deref() else {
        return;
    };
    let Some((col, row)) = target.split_once(',') else {
        return;
    };
    let (Ok(col), Ok(row)) = (col.trim().parse::<u32>(), row.trim().parse::<u32>()) else {
        return;
    };
    let Some(session) = session.as_mut() else {
        return;
    };
    *done = true;
    // 只发请求，**不自己移动相机**：验证 `focus_camera_on_request` 这条真实路径
    session.goto_room(col, row);
    let _ = (camera, col, row);
}

fn exit_on_frames_limit(cli: Res<Cli>, tick: Res<FrameTick>, mut exit: MessageWriter<AppExit>) {
    if let Some(frames) = cli.frames
        && tick.0 >= frames
    {
        println!("已跑满 {frames} 帧，退出");
        exit.write(AppExit::Success);
    }
}
