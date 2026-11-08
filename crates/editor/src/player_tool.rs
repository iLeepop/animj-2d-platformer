//! 全局玩家碰撞编辑（`06` §6.5.1、`08` §8.5.7 D）。
//!
//! 编辑的是项目级配置 `assets/config/player.ron`，**不是** `EditorSession.doc`：
//! 所有关卡、所有玩家共用；保存后游戏重启读取，编辑器试玩立即使用内存中的当前值。

use bevy::prelude::*;
use bevy_egui::egui;
use game::config::physics::{PLAYER_OFFSET_MAX, PLAYER_SIZE_MAX, PLAYER_SIZE_MIN};
use game::player::collision::PlayerCollision;

/// 玩家碰撞面板的保存状态（与 `PlayerCollision` 资源分离，便于显示脏标记）。
#[derive(Resource, Debug, Clone)]
pub struct PlayerEdit {
    /// 最后一次读盘/写盘的值。
    pub saved: PlayerCollision,
    pub dirty: bool,
    pub status: String,
}

impl PlayerEdit {
    pub fn new(saved: PlayerCollision) -> Self {
        Self {
            saved,
            dirty: false,
            status: String::new(),
        }
    }
}

/// 面板：宽高 + 相对精灵脚底的偏移 + 实时预览 + 保存/重载。
pub fn player_panel(
    ui: &mut egui::Ui,
    collision: &mut PlayerCollision,
    edit: &mut PlayerEdit,
    texture: Option<&egui::TextureHandle>,
) {
    ui.heading("玩家碰撞（全局）");
    ui.label("保存于 assets/config/player.ron；所有关卡、所有玩家共用。");
    ui.label("偏移 = 碰撞盒底边中点相对精灵脚底中点（+x 右、+y 上）。默认 (0,0) 即旧行为。");
    ui.separator();

    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("宽");
        changed |= ui
            .add(
                egui::DragValue::new(&mut collision.size.0)
                    .range(PLAYER_SIZE_MIN..=PLAYER_SIZE_MAX)
                    .speed(0.5)
                    .suffix(" px"),
            )
            .changed();
        ui.label("高");
        changed |= ui
            .add(
                egui::DragValue::new(&mut collision.size.1)
                    .range(PLAYER_SIZE_MIN..=PLAYER_SIZE_MAX)
                    .speed(0.5)
                    .suffix(" px"),
            )
            .changed();
        ui.label("水平偏移");
        changed |= ui
            .add(
                egui::DragValue::new(&mut collision.offset.0)
                    .range(-PLAYER_OFFSET_MAX..=PLAYER_OFFSET_MAX)
                    .speed(0.5)
                    .suffix(" px"),
            )
            .changed();
        ui.label("垂直偏移");
        changed |= ui
            .add(
                egui::DragValue::new(&mut collision.offset.1)
                    .range(-PLAYER_OFFSET_MAX..=PLAYER_OFFSET_MAX)
                    .speed(0.5)
                    .suffix(" px"),
            )
            .changed();
    });
    if changed {
        collision.clamp();
    }
    edit.dirty = *collision != edit.saved;

    ui.horizontal_wrapped(|ui| {
        if ui.button("保存").clicked() {
            collision.clamp();
            edit.status = match collision.persist() {
                Ok(()) => {
                    edit.saved = *collision;
                    edit.dirty = false;
                    format!("已保存：{}", PlayerCollision::config_path().display())
                }
                Err(error) => format!("保存失败：{error}"),
            };
        }
        if ui.button("从磁盘重载").clicked() {
            *collision = PlayerCollision::load();
            edit.saved = *collision;
            edit.dirty = false;
            edit.status = "已从磁盘重载".to_string();
        }
        if ui.button("重置默认 20×28").clicked() {
            *collision = PlayerCollision::default();
            edit.dirty = *collision != edit.saved;
        }
        if edit.dirty {
            ui.colored_label(egui::Color32::YELLOW, "* 未保存");
        }
        if !edit.status.is_empty() {
            ui.separator();
            ui.label(&edit.status);
        }
    });
    ui.separator();

    preview(ui, collision, texture);
}

/// 32×32 精灵 + 碰撞矩形预览（世界视口 64×64，y 向上；屏幕 y 向下）。
fn preview(ui: &mut egui::Ui, collision: &PlayerCollision, texture: Option<&egui::TextureHandle>) {
    const VIEW: f32 = 64.0;
    let side = ui.available_width().clamp(160.0, 240.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let scale = side / VIEW;
    let to_screen =
        |x: f32, y: f32| egui::pos2(rect.min.x + x * scale, rect.min.y + (VIEW - y) * scale);
    let world_rect = |x: f32, y: f32, w: f32, h: f32| {
        egui::Rect::from_min_max(to_screen(x, y + h), to_screen(x + w, y))
    };

    // 参考网格（8 px）
    for i in 0..=VIEW as i32 / 8 {
        let offset = i as f32 * 8.0;
        let stroke = egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28),
        );
        ui.painter()
            .line_segment([to_screen(offset, 0.0), to_screen(offset, VIEW)], stroke);
        ui.painter()
            .line_segment([to_screen(0.0, offset), to_screen(VIEW, offset)], stroke);
    }

    // 精灵：32×32，脚底中点 = 世界 (32,16)
    let sprite = world_rect(16.0, 16.0, 32.0, 32.0);
    if let Some(texture) = texture {
        let sheet = texture.size_vec2();
        let uv = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(32.0 / sheet.x, 32.0 / sheet.y),
        );
        ui.painter()
            .image(texture.id(), sprite, uv, egui::Color32::WHITE);
    } else {
        ui.painter()
            .rect_filled(sprite, 0.0, egui::Color32::from_gray(55));
    }
    ui.painter().rect_stroke(
        sprite,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(170)),
        egui::StrokeKind::Inside,
    );

    // 碰撞盒：底边中点 = 精灵脚底 + offset
    let bottom_x = 32.0 + collision.offset.0;
    let bottom_y = 16.0 + collision.offset.1;
    let body = world_rect(
        bottom_x - collision.size.0 / 2.0,
        bottom_y,
        collision.size.0,
        collision.size.1,
    );
    ui.painter().rect_filled(
        body,
        0.0,
        egui::Color32::from_rgba_unmultiplied(80, 220, 120, 70),
    );
    ui.painter().rect_stroke(
        body,
        0.0,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 220, 120)),
        egui::StrokeKind::Inside,
    );

    // 脚底中点
    let foot = to_screen(32.0, 16.0);
    let cross = egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 240, 130));
    ui.painter().line_segment(
        [foot + egui::vec2(-4.0, 0.0), foot + egui::vec2(4.0, 0.0)],
        cross,
    );
    ui.painter().line_segment(
        [foot + egui::vec2(0.0, -4.0), foot + egui::vec2(0.0, 4.0)],
        cross,
    );

    ui.label(format!(
        "碰撞盒 {:.0}×{:.0}，偏移 ({:+.0}, {:+.0})（精灵 32×32，脚底中点 = 十字）",
        collision.size.0, collision.size.1, collision.offset.0, collision.offset.1
    ));
}
