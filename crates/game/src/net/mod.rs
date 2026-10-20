//! 局域网联机（`05` §5.15）。零新依赖：`std::net::UdpSocket` + 现有 `serde`/`ron`。
//!
//! 权威模型：**主机权威**。主机跑全部物理、死亡、房间目标；客户端只上行输入、下行应用快照。
//! 本模块只负责传输与状态同步，玩法逻辑仍在各自模块里（保持纯函数可测）。
//!
//! 分工：
//! - [`protocol`]：消息与紧凑快照编解码。
//! - [`discovery`]：`OnlineMenu` 里的局域网房间搜索。
//! - [`host`]：加入、输入收集、快照广播、超时。
//! - [`client`]：加入重试、输入上行、应用快照。

pub mod client;
pub mod discovery;
pub mod host;
pub mod protocol;

use bevy::prelude::*;

use crate::state::{GameSet, GameState, SessionMode};

/// 主机侧玩法系统的门控：**客户端不跑**物理/死亡/目标/存档。
pub fn not_client(mode: Res<SessionMode>) -> bool {
    !mode.is_client()
}

/// 客户端专属系统门控。
pub fn is_client(mode: Res<SessionMode>) -> bool {
    mode.is_client()
}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionMode>()
            // 编辑器也注册 NetPlugin：`camera::rig` 依赖该资源（编辑器没有像素管线）。
            .init_resource::<crate::render::pixel_camera::ViewLayout>()
            // 客户端：进入 Loading 时建 socket；进入 Playing 后开始上行输入。
            .add_systems(OnEnter(GameState::Loading), client::start_client)
            .add_systems(OnEnter(GameState::Playing), host::start_host)
            // 离开整局（回主菜单）时关闭房间/连接，并把会话模式复位。
            .add_systems(OnEnter(GameState::MainMenu), teardown_net)
            .add_systems(OnEnter(GameState::OnlineMenu), discovery::start_browser)
            .add_systems(OnExit(GameState::OnlineMenu), discovery::stop_browser)
            .add_systems(
                Update,
                (
                    discovery::refresh_browser.run_if(in_state(GameState::OnlineMenu)),
                    host::poll_host.run_if(resource_exists::<host::HostNet>),
                    client::poll_client.run_if(resource_exists::<client::ClientNet>),
                    client::send_join.run_if(resource_exists::<client::ClientNet>),
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    client::send_input.in_set(GameSet::Input),
                    host::broadcast_snapshot.after(GameSet::Physics),
                ),
            )
            // 快照写入 `PlayerMotion` 后，相机与渲染必须看到它（`05` §5.4）。
            .add_systems(
                Update,
                client::apply_snapshot
                    .in_set(GameSet::PostPhysics)
                    .before(GameSet::Camera)
                    .run_if(in_state(GameState::Playing))
                    .run_if(resource_exists::<client::ClientNet>),
            );
    }
}

/// 回主菜单：客户端发 `Bye`，主机直接关 socket；然后复位会话模式。
fn teardown_net(
    mut commands: Commands,
    client: Option<Res<client::ClientNet>>,
    host: Option<Res<host::HostNet>>,
) {
    if let Some(client) = client {
        let _ = client.socket.send_to(
            &protocol::encode(&protocol::NetMessage::Bye).unwrap_or_default(),
            client.server,
        );
        info!("联机：已断开连接");
    }
    if host.is_some() {
        info!("联机：房间已关闭");
    }
    commands.remove_resource::<client::ClientNet>();
    commands.remove_resource::<host::HostNet>();
    commands.remove_resource::<discovery::LanBrowser>();
    commands.insert_resource(SessionMode::Solo);
}
