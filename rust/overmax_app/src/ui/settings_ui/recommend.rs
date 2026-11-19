//! 설정 창의 추천 탭: 추천 기준과 추천 데이터 제공자.

use crate::ui::dialog_theme::{
    field_row, section_card, segmented_row, setting_row, text_input_full, DialogTheme,
};
use eframe::egui::{self, CornerRadius, RichText, Stroke};

use super::object_section_mut;
use serde_json::{json, Value};

pub(super) fn recommend_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_card(ui, crate::t!("settings-recommend-section"), |ui| {
        recommend_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-recommend-provider"), |ui| {
        recommend_provider_section(ui, draft);
    });
}

fn recommend_section(ui: &mut egui::Ui, draft: &mut Value) {
    let smart_reco = draft
        .get("recommend")
        .and_then(|r| r.get("smart_recommend"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    segmented_row(
        ui,
        crate::t!("settings-smart-recommend"),
        crate::t!("settings-smart-recommend-hint"),
        |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
                ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);

                // right_to_left 이므로 역순으로 추가하여 화면에는 [스마트 | 클래식] 순으로 배치
                for (label, is_smart) in [
                    (crate::t!("settings-reco-mode-classic"), false),
                    (crate::t!("settings-reco-mode-smart"), true),
                ] {
                    let is_active = smart_reco == is_smart;
                    let btn = egui::Button::new(
                        RichText::new(label).size(DialogTheme::FONT_BODY).strong(),
                    )
                    .fill(if is_active {
                        DialogTheme::BG_CONTROL_ACTIVE
                    } else {
                        DialogTheme::BG_CONTROL
                    })
                    .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                    .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                    .wrap_mode(egui::TextWrapMode::Extend);

                    if ui
                        .add_sized(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT), btn)
                        .on_hover_text(crate::t!("settings-smart-recommend-desc"))
                        .clicked()
                    {
                        if !draft.get("recommend").is_some_and(|v| v.is_object()) {
                            draft["recommend"] = serde_json::json!({});
                        }
                        draft["recommend"]["smart_recommend"] = serde_json::json!(is_smart);
                        ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                    }
                }
            });
        },
    );

    if smart_reco {
        ui.add_space(DialogTheme::GAP_MD);

        let current_target = draft
            .get("recommend")
            .and_then(|r| r.get("target_rate"))
            .and_then(|v| v.as_f64())
            .unwrap_or(99.0);

        segmented_row(
            ui,
            crate::t!("settings-target-rate"),
            crate::t!("settings-target-rate-hint"),
            |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

                    // right_to_left 이므로 역순으로 추가하여 화면에는 [97% | 99% | 99.5% | 100%] 순으로 배치
                    for (label, val) in [
                        ("100%", 100.0),
                        ("99.5%", 99.5),
                        ("99%", 99.0),
                        ("97%", 97.0),
                    ] {
                        let is_active = (current_target - val).abs() < 0.001;
                        let btn = egui::Button::new(
                            RichText::new(label).size(DialogTheme::FONT_BODY).strong(),
                        )
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                        if ui
                            .add_sized(egui::vec2(60.0, DialogTheme::CONTROL_HEIGHT), btn)
                            .on_hover_text(crate::t!("settings-target-rate-desc"))
                            .clicked()
                        {
                            if !draft.get("recommend").is_some_and(|v| v.is_object()) {
                                draft["recommend"] = serde_json::json!({});
                            }
                            draft["recommend"]["target_rate"] = serde_json::json!(val);
                            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                        }
                    }
                });
            },
        );
    }
}

fn recommend_provider_section(ui: &mut egui::Ui, draft: &mut Value) {
    let rec_provider = object_section_mut(draft, "recommend_provider");

    ui.label(
        RichText::new(crate::t!("settings-recommend-provider-desc"))
            .color(DialogTheme::TEXT_MUTED)
            .size(DialogTheme::FONT_HINT),
    );
    ui.add_space(DialogTheme::GAP_MD);

    let mut enabled = rec_provider
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    setting_row(ui, crate::t!("settings-use-external-provider"), "", |ui| {
        if ui
            .checkbox(
                &mut enabled,
                RichText::new(crate::t!("settings-use")).size(DialogTheme::FONT_BODY),
            )
            .changed()
        {
            rec_provider.insert("enabled".into(), json!(enabled));
        }
    });

    if enabled {
        ui.add_space(DialogTheme::GAP_MD);

        let mut url = rec_provider
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        field_row(ui, "Provider URL", "", |ui| {
            if text_input_full(ui, &mut url, "http://127.0.0.1:8080") {
                rec_provider.insert("url".into(), json!(url.trim()));
            }
        });

        ui.add_space(DialogTheme::GAP_MD);

        let mut name = rec_provider
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        field_row(ui, crate::t!("settings-display-name"), "", |ui| {
            if text_input_full(
                ui,
                &mut name,
                &crate::t!("settings-eg-hint", domain = "djmax.gg"),
            ) {
                rec_provider.insert("name".into(), json!(name.trim()));
            }
        });
    }
}
