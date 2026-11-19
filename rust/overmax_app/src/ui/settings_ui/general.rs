//! 설정 창의 일반 탭: 언어와 오버레이 표시 설정.

use crate::ui::dialog_theme::{
    field_row, rtl_slider, section_card, segmented_row, setting_row, DialogTheme,
};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

use super::root_map_mut;
use serde_json::{json, Value};

pub(super) fn general_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_card(ui, crate::t!("settings-general"), |ui| {
        general_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-overlay-section"), |ui| {
        overlay_section(ui, draft);
    });
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };

    ui.label(
        RichText::new(crate::t!("settings-overlay-hint"))
            .color(DialogTheme::TEXT_MUTED)
            .size(DialogTheme::FONT_HINT),
    );
    ui.add_space(DialogTheme::GAP_MD);

    let current_scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    segmented_row(ui, crate::t!("settings-size"), "", |ui| {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

            // right_to_left 이므로 역순으로 추가하여 화면에는 [S | M | L | XL] 순으로 배치
            for (label, val) in [("XL", 1.5), ("L", 1.25), ("M", 1.0), ("S", 0.75)] {
                let is_active = (current_scale - val).abs() < 0.01;
                let btn =
                    egui::Button::new(RichText::new(label).size(DialogTheme::FONT_BODY).strong())
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                if ui
                    .add_sized(egui::vec2(44.0, DialogTheme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    overlay.insert("scale".into(), serde_json::json!(val));
                    ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                }
            }
        });
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut opacity = overlay
        .get("base_opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    setting_row(ui, crate::t!("settings-opacity"), "", |ui| {
        if rtl_slider(
            ui,
            &mut opacity,
            0.1..=1.0,
            0.1,
            |v| format!("{:.0}%", v * 100.0),
            130.0,
        ) {
            overlay.insert("base_opacity".into(), serde_json::json!(opacity));
            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
        }
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut lite_mode = overlay
        .get("lite_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    setting_row(
        ui,
        crate::t!("settings-lite-mode"),
        crate::t!("settings-lite-mode-desc"),
        |ui| {
            let response = ui.checkbox(&mut lite_mode, crate::t!("settings-enable"));
            if response.changed() {
                overlay.insert("lite_mode".into(), serde_json::json!(lite_mode));
                ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    let mut always_visible = overlay
        .get("always_visible")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    setting_row(
        ui,
        crate::t!("settings-overlay-display"),
        crate::t!("settings-always-show-desc"),
        |ui| {
            let response = ui.checkbox(&mut always_visible, crate::t!("settings-always-show"));
            if response.changed() {
                overlay.insert("always_visible".into(), serde_json::json!(always_visible));
                ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    field_row(ui, crate::t!("settings-snap-position"), "", |ui| {
        let mut position = overlay
            .get("position")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut snap = position
            .get("snap")
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string();

        let mut changed = false;

        // Render a visual monitor layout frame
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(280.0, 120.0), egui::Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(DialogTheme::R_SM),
            Color32::from_black_alpha(220),
            Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE),
            egui::StrokeKind::Inside,
        );

        let btn_size = egui::vec2(52.0, 32.0);
        let margin = 10.0;

        let rect_tl = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + margin, rect.min.y + margin),
            btn_size,
        );
        let rect_tr = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - btn_size.x - margin, rect.min.y + margin),
            btn_size,
        );
        let rect_bl = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + margin, rect.max.y - btn_size.y - margin),
            btn_size,
        );
        let rect_br = egui::Rect::from_min_size(
            egui::pos2(
                rect.max.x - btn_size.x - margin,
                rect.max.y - btn_size.y - margin,
            ),
            btn_size,
        );
        let rect_manual = egui::Rect::from_center_size(rect.center(), btn_size);

        for (icon, val, target_rect) in [
            ("↖", "top_left", rect_tl),
            ("↗", "top_right", rect_tr),
            ("↙", "bottom_left", rect_bl),
            ("↘", "bottom_right", rect_br),
            ("✥", "manual", rect_manual),
        ] {
            let is_active = snap == val;
            let btn = egui::Button::new(RichText::new(icon).size(DialogTheme::FONT_BODY).strong())
                .fill(if is_active {
                    DialogTheme::BG_CONTROL_ACTIVE
                } else {
                    DialogTheme::BG_CONTROL
                })
                .stroke(Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE))
                .corner_radius(CornerRadius::same(DialogTheme::R_SM));
            if ui.put(target_rect, btn).clicked() {
                snap = val.to_string();
                changed = true;
            }
        }

        if changed {
            position.insert("snap".into(), serde_json::json!(snap));
            overlay.insert("position".into(), serde_json::json!(position));
            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
        }
    });
}

fn general_section(ui: &mut egui::Ui, draft: &mut Value) {
    let raw_lang = draft.get("language").and_then(Value::as_str);
    let current_lang = match raw_lang {
        Some("ko") => "ko",
        Some("en") => "en",
        Some("ja") => "ja",
        _ => crate::ui::platform::detect_os_language(),
    };

    segmented_row(ui, crate::t!("settings-language"), "", |ui| {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

            // right_to_left 이므로 역순으로 추가하여 화면에는 [한국어 | English | 日本語] 순으로 배치
            for (label, val) in [("日本語", "ja"), ("English", "en"), ("한국어", "ko")] {
                let is_active = current_lang == val;
                let btn =
                    egui::Button::new(RichText::new(label).size(DialogTheme::FONT_BODY).strong())
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                if ui
                    .add_sized(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    root_map_mut(draft).insert("language".into(), json!(val));
                }
            }
        });
    });
}
