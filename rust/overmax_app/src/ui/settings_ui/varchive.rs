//! 설정 창의 V-Archive 탭: 계정 연결과 자동 업로드.

use crate::ui::dialog_theme::{
    field_row, section_card, segmented_row, setting_row, text_input_with_button, DialogTheme,
};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

use super::{object_section_mut, user_entry_mut, SettingsUiContext};
use serde_json::{json, Map, Value};

pub(super) fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_card(ui, crate::t!("settings-varchive-connect"), |ui| {
        setting_row(ui, crate::t!("settings-link-status"), "", |ui| {
            ui.label(
                RichText::new(current_steam_label(ctx))
                    .color(DialogTheme::TEXT_MUTED)
                    .size(DialogTheme::FONT_BODY),
            );
        });

        if ctx.current_steam_id.is_empty() {
            ui.add_space(DialogTheme::GAP_SM);
            ui.label(
                RichText::new(crate::t!("settings-no-steam-account"))
                    .color(DialogTheme::TEXT_WARN)
                    .size(DialogTheme::FONT_HINT),
            );
            return;
        }

        ui.add_space(DialogTheme::GAP_MD);
        v_archive_id_row(ui, draft, ctx);
    });

    if ctx.current_steam_id.is_empty() {
        return;
    }

    section_card(ui, crate::t!("settings-varchive-upload"), |ui| {
        ui.label(
            RichText::new(crate::t!("settings-varchive-upload-desc"))
                .color(DialogTheme::TEXT_MUTED)
                .size(DialogTheme::FONT_HINT),
        );
        ui.add_space(DialogTheme::GAP_MD);
        account_path_row(ui, draft, ctx);
        ui.add_space(DialogTheme::GAP_MD);
        auto_upload_rows(ui, draft);
    });
}

fn auto_upload_rows(ui: &mut egui::Ui, draft: &mut Value) {
    let varchive = object_section_mut(draft, "varchive");
    let auto = varchive
        .entry("auto_upload")
        .or_insert_with(|| Value::Object(Map::new()));
    if !auto.is_object() {
        *auto = Value::Object(Map::new());
    }
    let auto = auto
        .as_object_mut()
        .expect("auto_upload must be verified as a JSON Object");

    // 긴 안내는 줄바꿈되는 문단으로 두고, 행 힌트는 짧게 유지해 우측 컨트롤과 겹치지 않게 한다.
    ui.label(
        RichText::new(crate::t!("settings-auto-upload-desc"))
            .color(DialogTheme::TEXT_MUTED)
            .size(DialogTheme::FONT_HINT),
    );
    ui.add_space(DialogTheme::GAP_SM);

    let mut enabled = auto
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    setting_row(
        ui,
        crate::t!("settings-auto-upload"),
        crate::t!("settings-auto-upload-hint"),
        |ui| {
            if ui.checkbox(&mut enabled, "").changed() {
                auto.insert("enabled".to_string(), Value::Bool(enabled));
            }
        },
    );
    if !enabled {
        return;
    }

    let scope = auto
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("result_only")
        .to_string();
    ui.add_space(DialogTheme::GAP_MD);
    // right_to_left 이므로 역순으로 추가하여 화면에는 [결과창만 | 선곡+결과창] 순으로 배치
    choice_row(
        ui,
        crate::t!("settings-auto-upload-scope"),
        crate::t!("settings-auto-upload-scope-hint"),
        &[
            (
                crate::t!("settings-auto-upload-scope-select"),
                "select_and_result",
            ),
            (
                crate::t!("settings-auto-upload-scope-result"),
                "result_only",
            ),
        ],
        |value| scope == *value,
        |value| {
            auto.insert("scope".to_string(), json!(value));
        },
    );

    let delay = auto.get("delay_sec").and_then(Value::as_u64).unwrap_or(3);
    ui.add_space(DialogTheme::GAP_MD);
    // 화면에는 [즉시 | 3초] 순으로 배치
    choice_row(
        ui,
        crate::t!("settings-auto-upload-delay"),
        crate::t!("settings-auto-upload-delay-hint"),
        &[
            (crate::t!("settings-auto-upload-delay-3s"), 3_u64),
            (crate::t!("settings-auto-upload-delay-immediate"), 0_u64),
        ],
        |value| delay == *value,
        |value| {
            auto.insert("delay_sec".to_string(), json!(value));
        },
    );
}

/// `segmented_row` 안에 선택지 버튼을 오른쪽부터 배치한다. `options` 는 화면 역순으로 넘긴다.
fn choice_row<T: Copy>(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    options: &[(&str, T)],
    is_active: impl Fn(&T) -> bool,
    mut on_select: impl FnMut(T),
) {
    segmented_row(ui, label, hint, |ui| {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
            ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);

            // 라벨 길이가 달라도 버튼 폭을 통일한다 (최소 84px).
            let font = egui::FontId::proportional(DialogTheme::FONT_BODY);
            let width = options
                .iter()
                .map(|(text, _)| {
                    ui.painter()
                        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                })
                .fold(84.0_f32, |acc, w| {
                    acc.max(w + 2.0 * ui.spacing().button_padding.x + 4.0)
                });

            for (text, value) in options {
                let btn =
                    egui::Button::new(RichText::new(*text).size(DialogTheme::FONT_BODY).strong())
                        .fill(if is_active(value) {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap_mode(egui::TextWrapMode::Extend);

                if ui
                    .add_sized(egui::vec2(width, DialogTheme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    on_select(*value);
                    ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                }
            }
        });
    });
}

fn current_steam_label(ctx: &SettingsUiContext) -> String {
    if ctx.current_steam_id.is_empty() {
        "Steam: -".to_string()
    } else {
        if let Ok(users) = ctx.steam_users.lock() {
            if let Some(user) = users.get(&ctx.current_steam_id) {
                if !user.persona_name.is_empty() && !user.account_name.is_empty() {
                    return format!(
                        "Steam: {} ({}) [{}]",
                        user.persona_name, user.account_name, ctx.current_steam_id
                    );
                } else if !user.persona_name.is_empty() {
                    return format!("Steam: {} [{}]", user.persona_name, ctx.current_steam_id);
                }
            }
        }
        format!("Steam: {}", ctx.current_steam_id)
    }
}

fn v_archive_id_row(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    let mut text = entry
        .get("v_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    field_row(ui, "V-Archive ID", "", |ui| {
        let has_id = !text.trim().is_empty();
        let (changed, clicked) =
            text_input_with_button(ui, &mut text, "", crate::t!("sync-refresh"), has_id);
        if clicked {
            let _ = ctx
                .fetch_tx
                .send((ctx.current_steam_id.clone(), text.trim().to_string(), 0));
        }
        if changed {
            entry.insert("v_id".into(), json!(text.trim()));
        }
    });
}

fn account_path_row(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    field_row(ui, "account.txt", "", |ui| {
        let mut path_str = entry
            .get("account_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let (changed, clicked) =
            text_input_with_button(ui, &mut path_str, "", crate::t!("settings-browse"), true);
        if clicked {
            if let Some(file_path) = rfd::FileDialog::new()
                .add_filter("Text Files", &["txt"])
                .pick_file()
            {
                let new_path = file_path.to_string_lossy().to_string();
                entry.insert("account_path".into(), json!(new_path));
            }
        } else if changed {
            entry.insert("account_path".into(), json!(path_str.trim()));
        }
    });
}
