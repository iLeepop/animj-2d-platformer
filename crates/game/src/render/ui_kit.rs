//! UI 设计系统组件。所有者文档：`docs/07-rendering.md` §7.10.5。
//!
//! 所有界面只通过本模块构造按钮/卡片/面板；颜色、尺寸来自 `config::ui`。
//! 交互语义固定为：
//! - `Hovered` 显式插入（`bevy_picking` 只更新已有该组件的实体）；
//! - `Pressed` 由 `bevy_ui_widgets::ButtonPlugin` 维护；
//! - 业务动作由 `Activate` observer 处理（松开鼠标或焦点按钮 Enter/Space）。

use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;

use crate::config::ui::*;
use crate::fonts::UiFont;
use crate::render::pixel_camera::ViewLayout;

/// 按钮视觉层级。每个界面最多一个 `Primary`。
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum UiButtonRole {
    Primary,
    Secondary,
    Danger,
}

/// 所有由 `ui_kit` 构造的按钮。
#[derive(Component)]
pub struct UiButton;

/// 按钮文本；hover/focus 时由反馈系统改色。
#[derive(Component)]
pub struct UiButtonLabel;

pub struct UiKitPlugin;

impl Plugin for UiKitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TabNavigationPlugin)
            .add_systems(Update, button_feedback);
    }
}

#[derive(Clone, Copy)]
struct RoleColors {
    idle_bg: Color,
    idle_border: Color,
    idle_text: Color,
    hot_bg: Color,
    hot_border: Color,
    hot_text: Color,
    down_bg: Color,
    down_border: Color,
    down_text: Color,
}

impl UiButtonRole {
    fn colors(self) -> RoleColors {
        match self {
            UiButtonRole::Primary => RoleColors {
                idle_bg: UI_GOLD,
                idle_border: UI_GOLD,
                idle_text: UI_INK,
                hot_bg: UI_GOLD,
                hot_border: UI_PAPER,
                hot_text: UI_INK,
                down_bg: UI_ORANGE,
                down_border: UI_ORANGE,
                down_text: UI_INK,
            },
            UiButtonRole::Secondary => RoleColors {
                idle_bg: UI_SURFACE,
                idle_border: UI_PLUM,
                idle_text: UI_PAPER,
                hot_bg: UI_SURFACE_HI,
                hot_border: UI_GOLD,
                hot_text: UI_GOLD,
                down_bg: UI_INK,
                down_border: UI_ORANGE,
                down_text: UI_PAPER,
            },
            UiButtonRole::Danger => RoleColors {
                idle_bg: UI_SURFACE,
                idle_border: UI_CRIMSON,
                idle_text: UI_PAPER,
                hot_bg: UI_SURFACE_HI,
                hot_border: UI_ORANGE,
                hot_text: UI_PAPER,
                down_bg: UI_INK,
                down_border: UI_CRIMSON,
                down_text: UI_PAPER,
            },
        }
    }
}

/// 全屏界面根：INK 底、居中列、模态 Tab 组。调用方随后插入 marker / `DespawnOnExit`。
pub fn screen(commands: &mut Commands, layout: &ViewLayout, z: i32) -> Entity {
    commands
        .spawn((
            screen_node(layout),
            BackgroundColor(UI_INK),
            GlobalZIndex(z),
            TabGroup::modal(),
        ))
        .id()
}

/// 全屏遮罩根：SCRIM 底、居中列、模态 Tab 组。
pub fn scrim_screen(commands: &mut Commands, layout: &ViewLayout, z: i32) -> Entity {
    commands
        .spawn((
            screen_node(layout),
            BackgroundColor(UI_SCRIM),
            GlobalZIndex(z),
            TabGroup::modal(),
        ))
        .id()
}

fn screen_node(layout: &ViewLayout) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0.0),
        top: px(0.0),
        width: px(layout.width()),
        height: px(layout.height()),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        padding: UiRect::all(px(MENU_PADDING_PX)),
        row_gap: px(MENU_GAP_PX),
        overflow: Overflow::clip(),
        ..default()
    }
}

/// 面板：INK 95% 底 + 2px PLUM 边框 + 2px 硬阴影。
pub fn panel(commands: &mut Commands, parent: Entity, width: f32, height: Option<f32>) -> Entity {
    let mut node = Node {
        width: px(width),
        max_height: px(UI_PANEL_MAX_HEIGHT),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: px(MENU_GAP_PX),
        padding: UiRect::all(px(UI_PANEL_PADDING)),
        border: UiRect::all(px(UI_BORDER_PX)),
        ..default()
    };
    if let Some(height) = height {
        node.height = px(height);
        node.max_height = px(height);
    }
    commands
        .spawn((
            node,
            BackgroundColor(UI_PANEL_BG),
            BorderColor::all(UI_PLUM),
            BoxShadow::new(UI_SHADOW, px(0.0), px(UI_SHADOW_PX), px(0.0), px(0.0)),
            ChildOf(parent),
        ))
        .id()
}

/// 标题：24px、PAPER、居中。
pub fn title(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
) -> Entity {
    title_with_marker(commands, font, parent, content, ())
}

/// 标题 + 调用方 marker。
pub fn title_with_marker<M: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
    marker: M,
) -> Entity {
    commands
        .spawn((
            Text::new(content),
            font.text_font(MENU_TITLE_FONT_PX),
            TextColor(UI_PAPER),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            marker,
            ChildOf(parent),
        ))
        .id()
}

/// 正文：12px、PAPER。
pub fn body(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
) -> Entity {
    text_with_marker(commands, font, parent, content, MENU_FONT_PX, UI_PAPER, ())
}

/// 正文 + 调用方 marker。
pub fn body_with_marker<M: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
    marker: M,
) -> Entity {
    text_with_marker(
        commands,
        font,
        parent,
        content,
        MENU_FONT_PX,
        UI_PAPER,
        marker,
    )
}

/// 正文但居中。
pub fn body_center(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
) -> Entity {
    commands
        .spawn((
            Text::new(content),
            font.text_font(MENU_FONT_PX),
            TextColor(UI_PAPER),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

/// 次要文本：12px、PAPER 60%。
pub fn muted(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
) -> Entity {
    text_with_marker(commands, font, parent, content, MENU_FONT_PX, UI_MUTED, ())
}

/// 通用文本 + 调用方 marker。
pub fn text_with_marker<M: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    content: impl Into<String>,
    size: f32,
    color: Color,
    marker: M,
) -> Entity {
    commands
        .spawn((
            Text::new(content),
            font.text_font(size),
            TextColor(color),
            marker,
            ChildOf(parent),
        ))
        .id()
}

/// 标准菜单按钮：200×24，居中文本。`action` 挂在**按钮实体**上。
pub fn button_with_action<A: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    action: A,
) -> Entity {
    button_with_action_label(commands, font, parent, label, role, action, ())
}

/// 标准菜单按钮 + 文本 marker（用于设置页同步动态值）。
pub fn button_with_action_label<A: Bundle, L: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    action: A,
    label_marker: L,
) -> Entity {
    button_sized_with_action_label(
        commands,
        font,
        parent,
        label,
        role,
        px(UI_BUTTON_WIDTH),
        UI_BUTTON_HEIGHT,
        action,
        label_marker,
    )
}

/// 固定/自动宽度按钮。`width = Val::Auto` + padding 用于设置控件与 chip。
#[allow(clippy::too_many_arguments)]
pub fn button_sized_with_action<A: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    width: Val,
    height: f32,
    action: A,
) -> Entity {
    button_sized_with_action_label(
        commands,
        font,
        parent,
        label,
        role,
        width,
        height,
        action,
        (),
    )
}

/// 固定/自动宽度按钮 + 文本 marker。
#[allow(clippy::too_many_arguments)]
pub fn button_sized_with_action_label<A: Bundle, L: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    label: impl Into<String>,
    role: UiButtonRole,
    width: Val,
    height: f32,
    action: A,
    label_marker: L,
) -> Entity {
    let colors = role.colors();
    let id = commands
        .spawn((
            Button,
            Node {
                width,
                height: px(height),
                min_height: px(height),
                min_width: px(UI_BUTTON_HEIGHT_SM),
                padding: UiRect::axes(px(UI_SPACE_XS), px(0.0)),
                border: UiRect::all(px(UI_BORDER_PX)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(colors.idle_bg),
            BorderColor::all(colors.idle_border),
            Outline::new(px(UI_FOCUS_OUTLINE_PX), px(1.0), Color::NONE),
            UiTransform::default(),
            Hovered(false),
            TabIndex(0),
            role,
            UiButton,
            ChildOf(parent),
        ))
        .id();
    commands.entity(id).insert(action);
    let label_id = commands
        .spawn((
            Text::new(label),
            font.text_font(MENU_FONT_PX),
            TextColor(colors.idle_text),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            UiButtonLabel,
            ChildOf(id),
        ))
        .id();
    commands.entity(label_id).insert(label_marker);
    id
}

/// 关卡/联机卡片：128×56，标题 + 可选副标题。`action` 挂在按钮实体上。
pub fn card_with_action<A: Bundle>(
    commands: &mut Commands,
    font: &UiFont,
    parent: Entity,
    title: impl Into<String>,
    subtitle: Option<String>,
    role: UiButtonRole,
    action: A,
) -> Entity {
    let colors = role.colors();
    let id = commands
        .spawn((
            Button,
            Node {
                width: px(UI_LEVEL_CARD_WIDTH),
                height: px(UI_LEVEL_CARD_HEIGHT),
                padding: UiRect::all(px(UI_SPACE_XS)),
                border: UiRect::all(px(UI_BORDER_PX)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(2.0),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(colors.idle_bg),
            BorderColor::all(colors.idle_border),
            Outline::new(px(UI_FOCUS_OUTLINE_PX), px(1.0), Color::NONE),
            UiTransform::default(),
            Hovered(false),
            TabIndex(0),
            role,
            UiButton,
            ChildOf(parent),
        ))
        .id();
    commands.entity(id).insert(action);
    commands.spawn((
        Text::new(title),
        font.text_font(MENU_FONT_PX),
        TextColor(colors.idle_text),
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
        UiButtonLabel,
        ChildOf(id),
    ));
    if let Some(subtitle) = subtitle {
        commands.spawn((
            Text::new(subtitle),
            font.text_font(MENU_FONT_PX),
            TextColor(UI_GREEN),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            ChildOf(id),
        ));
    }
    id
}

/// hover / press / focus 的统一反馈。
///
/// 只改颜色与 `UiTransform.translation`，不改 `Node`，因此不会触发布局重排。
#[allow(clippy::type_complexity)]
fn button_feedback(
    focus: Option<Res<InputFocus>>,
    mut buttons: Query<
        (
            Entity,
            &UiButtonRole,
            &Hovered,
            Has<Pressed>,
            &Children,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Outline,
            &mut UiTransform,
        ),
        With<UiButton>,
    >,
    mut labels: Query<&mut TextColor, With<UiButtonLabel>>,
) {
    let focused = focus.as_ref().and_then(|focus| focus.get());
    for (entity, role, hovered, pressed, children, mut bg, mut border, mut outline, mut tf) in
        &mut buttons
    {
        let colors = role.colors();
        let is_focused = focused == Some(entity);
        let is_hot = hovered.get() || is_focused;
        let (bg_color, border_color, text_color) = if pressed {
            (colors.down_bg, colors.down_border, colors.down_text)
        } else if is_hot {
            (colors.hot_bg, colors.hot_border, colors.hot_text)
        } else {
            (colors.idle_bg, colors.idle_border, colors.idle_text)
        };
        if bg.0 != bg_color {
            bg.0 = bg_color;
        }
        if border.top != border_color
            || border.right != border_color
            || border.bottom != border_color
            || border.left != border_color
        {
            border.set_all(border_color);
        }
        let outline_color = if is_focused { UI_GOLD } else { Color::NONE };
        if outline.color != outline_color {
            outline.color = outline_color;
        }
        let next_translation = Val2::new(
            px(0.0),
            px(if pressed {
                1.0
            } else if is_hot {
                -1.0
            } else {
                0.0
            }),
        );
        if tf.translation != next_translation {
            tf.translation = next_translation;
        }
        for child in children.iter() {
            if let Ok(mut label_color) = labels.get_mut(child)
                && label_color.0 != text_color
            {
                label_color.0 = text_color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_geometry_matches_documented_content_width() {
        let content = UI_PAUSE_PANEL_WIDTH - 2.0 * UI_BORDER_PX - 2.0 * UI_PANEL_PADDING;
        assert_eq!(content, UI_PAUSE_BUTTON_WIDTH);
        let settings = UI_SETTINGS_PANEL_WIDTH - 2.0 * UI_BORDER_PX - 2.0 * UI_PANEL_PADDING;
        assert_eq!(settings, UI_SETTINGS_CONTENT_WIDTH);
    }
}
