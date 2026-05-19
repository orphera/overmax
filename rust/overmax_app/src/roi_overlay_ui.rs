use eframe::egui::{self, Color32, FontId, Frame, Pos2, Rect, Stroke, ViewportClass, StrokeKind};
use crate::roi::{RoiManager, RoiRect};
use crate::window_tracker::WindowRect;

pub fn render_roi_overlay(ctx: &egui::Context, class: ViewportClass, game_rect: WindowRect) {
    if class == ViewportClass::Embedded {
        return;
    }

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(Color32::TRANSPARENT))
        .show(ctx, |ui| {
            let roiman = RoiManager::new(game_rect.width, game_rect.height);

            let draw_box = |ui: &mut egui::Ui, name: &str, roi: Option<RoiRect>, color: Color32| {
                if let Some(r) = roi {
                    let egui_rect = Rect::from_min_max(
                        Pos2::new(r.x1 as f32, r.y1 as f32),
                        Pos2::new(r.x2 as f32, r.y2 as f32),
                    );

                    // Draw outline
                    ui.painter().rect_stroke(
                        egui_rect,
                        0.0,
                        Stroke::new(2.0, color),
                        StrokeKind::Inside,
                    );

                    // Draw text label just above the box
                    let text_pos = Pos2::new(egui_rect.min.x, egui_rect.min.y - 2.0);
                    ui.painter().text(
                        text_pos,
                        egui::Align2::LEFT_BOTTOM,
                        name,
                        FontId::monospace(10.0),
                        color,
                    );
                }
            };

            // Define colors
            let logo_color = Color32::from_rgb(255, 0, 0);
            let jacket_color = Color32::from_rgb(0, 128, 255);
            let btn_mode_color = Color32::from_rgb(255, 165, 0);
            let rate_color = Color32::from_rgb(0, 255, 0);
            let diff_color = Color32::from_rgb(255, 0, 255);
            let max_combo_color = Color32::from_rgb(255, 255, 0);

            draw_box(ui, "logo", roiman.get_roi("logo"), logo_color);
            draw_box(ui, "jacket", roiman.get_roi("jacket"), jacket_color);
            draw_box(ui, "btn_mode", roiman.get_roi("btn_mode"), btn_mode_color);
            draw_box(ui, "rate", roiman.get_roi("rate"), rate_color);
            draw_box(ui, "max_combo_badge", roiman.get_roi("max_combo_badge"), max_combo_color);

            for diff in &["NM", "HD", "MX", "SC"] {
                let roi = if *diff == "NM" {
                    roiman.get_roi("diff_panel")
                } else {
                    roiman.get_diff_panel_roi(diff)
                };
                draw_box(ui, &format!("diff_panel ({diff})"), roi, diff_color);
            }
        });
}
