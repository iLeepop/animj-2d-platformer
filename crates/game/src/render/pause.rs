//! 暂停菜单与装饰品装备界面。所有者文档：`docs/05-architecture.md` §5.5、`docs/01-game-design.md` §1.5。
//!
//! `GameState::Paused` 冻结物理与计时（`05` §5.5）。注意：编辑器也把状态停在 `Paused`，
//! 但它不会把 [`PauseMenu::open`] 置真 ⇒ 编辑态不会弹出本菜单。

use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;

use crate::config::render::{DECORATION_LAYERS, z};
use crate::player::decor::{Decorations, spec_of};
use crate::save;
use crate::state::GameState;
use crate::world::progress::LevelProgress;

#[derive(Resource, Debug, Default)]
pub struct PauseMenu {
    pub open: bool,
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component)]
struct PauseButton(PauseAction);

#[derive(Component, Debug, Clone)]
enum PauseAction {
    Resume,
    Save,
    Quit,
    Equip(String),
    Unequip(usize),
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenu>()
            .add_systems(Update, toggle_pause)
            .add_systems(Update, (rebuild_pause_ui, handle_pause_buttons).chain());
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match state.get() {
        GameState::Playing => {
            menu.open = true;
            next.set(GameState::Paused);
            info!("暂停");
        }
        GameState::Paused if menu.open => {
            menu.open = false;
            next.set(GameState::Playing);
            info!("继续");
        }
        _ => {}
    }
}

/// 菜单开关或装备变化时重建（一次性重建，避免每帧 diff UI 树）。
fn rebuild_pause_ui(
    mut commands: Commands,
    menu: Res<PauseMenu>,
    decorations: Res<Decorations>,
    roots: Query<Entity, With<PauseRoot>>,
) {
    if !menu.is_changed() && !decorations.is_changed() {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !menu.open {
        return;
    }

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(96.0),
                top: px(64.0),
                width: px(320.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                padding: UiRect::all(px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
            PauseRoot,
            GlobalZIndex(10),
        ))
        .id();

    button(&mut commands, root, "RESUME (Esc)", PauseAction::Resume);
    button(&mut commands, root, "SAVE (Ctrl+S)", PauseAction::Save);
    button(&mut commands, root, "QUIT & SAVE", PauseAction::Quit);

    for layer in 0..DECORATION_LAYERS {
        let current = decorations
            .equipped_at(layer)
            .map(str::to_string)
            .unwrap_or_else(|| "-".to_string());
        let row = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(4.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        commands.spawn((
            Text::new(format!("L{layer} {current}")),
            TextColor(Color::WHITE),
            ChildOf(row),
        ));
        for id in decorations.owned() {
            let Some(spec) = spec_of(id) else {
                continue; // 未知 id：不显示（存档缺失元数据视为未装备）
            };
            if spec.layer != layer {
                continue;
            }
            button(&mut commands, row, id, PauseAction::Equip(id.clone()));
        }
        button(&mut commands, row, "x", PauseAction::Unequip(layer));
    }
    let _ = z::DECORATION_BASE;
}

fn button(commands: &mut Commands, parent: Entity, label: &str, action: PauseAction) {
    commands.spawn((
        Button,
        Node {
            padding: UiRect::axes(px(6.0), px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
        PauseButton(action),
        ChildOf(parent),
    ))
    .with_child((Text::new(label), TextColor(Color::WHITE)));
}

fn handle_pause_buttons(
    interactions: Query<&PauseButton, Added<Pressed>>,
    mut decorations: ResMut<Decorations>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
    progress: Option<Res<LevelProgress>>,
    checkpoint: Option<Res<crate::checkpoint::ActiveCheckpoint>>,
    mut exit: MessageWriter<AppExit>,
) {
    for button in &interactions {
        match &button.0 {
            PauseAction::Resume => {
                menu.open = false;
                next.set(GameState::Playing);
            }
            PauseAction::Save => {
                if let Some(progress) = progress.as_ref() {
                    save::persist_now(
                        progress,
                        &decorations,
                        checkpoint.as_ref().and_then(|point| point.0),
                    );
                }
            }
            PauseAction::Quit => {
                if let Some(progress) = progress.as_ref() {
                    save::persist_now(
                        progress,
                        &decorations,
                        checkpoint.as_ref().and_then(|point| point.0),
                    );
                }
                exit.write(AppExit::Success);
            }
            PauseAction::Equip(id) => {
                decorations.equip(id);
            }
            PauseAction::Unequip(layer) => {
                decorations.unequip(*layer);
            }
        }
    }
}
