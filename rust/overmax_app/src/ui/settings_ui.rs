//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use crate::ui::overlay_theme::{apply_secondary_window_style, render_pill_tabs, Theme};
use eframe::egui::{
    self, CornerRadius, Frame, Margin, RichText, Slider, Stroke, TextEdit, ViewportClass,
};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct SettingsUiContext {
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
    pub steam_users:
        Arc<Mutex<std::collections::HashMap<String, crate::system::steam_session::SteamUser>>>,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_secondary_window_style(ui.ctx());

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Overmax")
                .color(Theme::TEXT_ACCENT)
                .size(Theme::FONT_HEADING)
                .strong(),
        );
        ui.label(
            RichText::new("설정")
                .color(Theme::TEXT_PRIMARY)
                .size(Theme::FONT_HEADING)
                .strong(),
        );
    });

    ui.add_space(16.0);

    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    render_pill_tabs(
        ui,
        "settings_tabs",
        &["UI", "V-Archive", "System"],
        &mut active,
    );
    ui.data_mut(|d| d.insert_temp(id, active));

    ui.add_space(20.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match active {
            0 => ui_tab(ui, draft),
            1 => varchive_tab(ui, draft, ctx),
            _ => system_tab(ui, draft),
        });
}

fn ui_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "오버레이 설정", |ui| overlay_section(ui, draft));
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };

    form_row(ui, "크기", |ui| {
        let current_scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = egui::vec2(4.0, 4.0);
            for (label, val) in [("S", 0.75), ("M", 1.0), ("L", 1.25), ("XL", 1.5)] {
                let is_active = (current_scale - val).abs() < 0.01;
                let btn = egui::Button::new(RichText::new(label).size(Theme::FONT_SMALL).strong())
                    .fill(if is_active {
                        Theme::TAB_ACTIVE_BG
                    } else {
                        Theme::TAB_DIM_BG
                    })
                    .stroke(Stroke::new(1.0_f32, Theme::STROKE))
                    .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                    .wrap();

                if ui
                    .add_sized(egui::vec2(36.0, Theme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    overlay.insert("scale".into(), serde_json::json!(val));
                }
            }
        });
    });

    ui.add_space(Theme::ROW_SPACING);

    form_row(ui, "투명도", |ui| {
        let mut opacity = overlay
            .get("base_opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8);

        let slider = Slider::new(&mut opacity, 0.1..=1.0)
            .step_by(0.1)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
            .trailing_fill(true);

        if ui
            .add_sized(
                egui::vec2(ui.available_width(), Theme::CONTROL_HEIGHT),
                slider,
            )
            .changed()
        {
            overlay.insert("base_opacity".into(), serde_json::json!(opacity));
        }
    });

    ui.add_space(Theme::ROW_SPACING);

    form_row(ui, "라이트모드", |ui| {
        let mut lite_mode = overlay
            .get("lite_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let response = ui
            .checkbox(&mut lite_mode, "활성화")
            .on_hover_text("추천 숨기기 및 레이아웃 축소");

        if response.changed() {
            overlay.insert("lite_mode".into(), serde_json::json!(lite_mode));
        }
    });

    ui.add_space(Theme::ROW_SPACING);
    form_row(ui, "오버레이 고정 위치", |ui| {
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

        // Render a visual monitor layout frame (280x120 black rect)
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(280.0, 120.0), egui::Sense::hover());
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(Theme::R_SM),
            egui::Color32::from_black_alpha(220),
            egui::Stroke::new(1.0_f32, Theme::STROKE),
            egui::StrokeKind::Inside,
        );

        let btn_size = egui::vec2(65.0, 30.0);
        let margin = 8.0;

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
        let rect_manual = egui::Rect::from_min_size(
            egui::pos2(
                rect.center().x - btn_size.x / 2.0,
                rect.center().y - btn_size.y / 2.0,
            ),
            btn_size,
        );

        for (label, val, target_rect) in [
            ("좌상단", "top_left", rect_tl),
            ("우상단", "top_right", rect_tr),
            ("좌하단", "bottom_left", rect_bl),
            ("우하단", "bottom_right", rect_br),
            ("수동", "manual", rect_manual),
        ] {
            let is_active = snap == val;
            let btn = egui::Button::new(RichText::new(label).size(Theme::FONT_SMALL).strong())
                .fill(if is_active {
                    Theme::TAB_ACTIVE_BG
                } else {
                    Theme::TAB_DIM_BG
                })
                .stroke(egui::Stroke::new(1.0_f32, Theme::STROKE))
                .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                .wrap();
            if ui.put(target_rect, btn).clicked() {
                snap = val.to_string();
                changed = true;
            }
        }

        if changed {
            position.insert("snap".into(), serde_json::json!(snap));
            overlay.insert("position".into(), serde_json::json!(position));
        }
    });
}

fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_frame(ui, "V-Archive 계정", |ui| {
        form_row(ui, "연동 상태", |ui| {
            ui.label(
                RichText::new(current_steam_label(ctx))
                    .color(Theme::TEXT_MUTED)
                    .size(Theme::FONT_SMALL),
            );
        });

        ui.add_space(Theme::ROW_SPACING);

        if ctx.current_steam_id.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("발견된 Steam 계정이 없습니다.")
                    .color(Theme::WARN)
                    .size(Theme::FONT_SMALL),
            );
            return;
        }

        ui.add_space(16.0);
        steam_account_rows(ui, draft, ctx);
    });
}

fn current_steam_label(ctx: &SettingsUiContext) -> String {
    if ctx.current_steam_id.is_empty() {
        "현재 Steam: -".into()
    } else {
        if let Ok(users) = ctx.steam_users.lock() {
            if let Some(user) = users.get(&ctx.current_steam_id) {
                if !user.persona_name.is_empty() && !user.account_name.is_empty() {
                    return format!(
                        "현재 Steam: {} ({}) [{}]",
                        user.persona_name, user.account_name, ctx.current_steam_id
                    );
                } else if !user.persona_name.is_empty() {
                    return format!(
                        "현재 Steam: {} [{}]",
                        user.persona_name, ctx.current_steam_id
                    );
                }
            }
        }
        format!("현재 Steam: {}", ctx.current_steam_id)
    }
}

fn steam_account_rows(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);

    form_row(ui, "V-Archive ID", |ui| {
        let mut text = entry
            .get("v_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if ui
            .add(
                TextEdit::singleline(&mut text)
                    .font(egui::FontId::proportional(Theme::FONT_BODY))
                    .vertical_align(egui::Align::Center)
                    .margin(egui::Margin::symmetric(8, 0))
                    .desired_width(ui.available_width())
                    .min_size(egui::vec2(0.0, Theme::CONTROL_HEIGHT)),
            )
            .changed()
        {
            entry.insert("v_id".into(), json!(text.trim()));
        }
    });

    ui.add_space(Theme::ROW_SPACING);

    form_row(ui, "데이터 동기화", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = egui::vec2(4.0, 4.0);
            for b in [4, 5, 6, 8] {
                let btn = egui::Button::new(RichText::new(format!("{b}B")).size(Theme::FONT_SMALL))
                    .fill(Theme::TAB_DIM_BG)
                    .stroke(Stroke::new(1.0_f32, Theme::STROKE))
                    .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                    .wrap();
                if ui
                    .add_sized(egui::vec2(40.0, Theme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
                    if !v_id.is_empty() {
                        let _ =
                            ctx.fetch_tx
                                .send((ctx.current_steam_id.clone(), v_id.to_string(), b));
                    }
                }
            }
            let all_btn = egui::Button::new(RichText::new("All").size(Theme::FONT_SMALL).strong())
                .fill(Theme::TAB_ACTIVE_BG)
                .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                .wrap();
            if ui
                .add_sized(egui::vec2(40.0, Theme::CONTROL_HEIGHT), all_btn)
                .clicked()
            {
                let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
                if !v_id.is_empty() {
                    let _ = ctx
                        .fetch_tx
                        .send((ctx.current_steam_id.clone(), v_id.to_string(), 0));
                }
            }
        });
    });

    ui.add_space(Theme::ROW_SPACING);

    form_row(ui, "account.txt", |ui| {
        let mut path_str = entry
            .get("account_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().button_padding = egui::vec2(4.0, 4.0);
            let find_btn = egui::Button::new(RichText::new("찾기").size(Theme::FONT_SMALL))
                .fill(Theme::TAB_ACTIVE_BG)
                .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                .wrap();
            if ui
                .add_sized(egui::vec2(60.0, Theme::CONTROL_HEIGHT), find_btn)
                .clicked()
            {
                if let Some(file_path) = rfd::FileDialog::new()
                    .add_filter("Text Files", &["txt"])
                    .pick_file()
                {
                    let path_str = file_path.to_string_lossy().to_string();
                    entry.insert("account_path".into(), json!(path_str));
                }
            }

            ui.add_space(4.0);

            ui.add(
                TextEdit::singleline(&mut path_str)
                    .font(egui::FontId::proportional(Theme::FONT_BODY))
                    .vertical_align(egui::Align::Center)
                    .margin(egui::Margin::symmetric(8, 0))
                    .desired_width(ui.available_width())
                    .min_size(egui::vec2(0.0, Theme::CONTROL_HEIGHT)),
            );
        });
    });

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.add_space(Theme::LABEL_WIDTH + 8.0);
        let scan_btn = egui::Button::new(
            RichText::new("🔍 동기화 후보 찾기")
                .size(Theme::FONT_BODY)
                .strong(),
        )
        .min_size(egui::vec2(180.0, 40.0))
        .fill(Theme::TEXT_ACCENT)
        .corner_radius(egui::CornerRadius::same(Theme::R_SM));

        if ui.add(scan_btn).clicked() {
            if let Ok(mut sid) = ctx.sync_steam_id.lock() {
                *sid = ctx.current_steam_id.clone();
            }
            ctx.sync_open.store(true, Ordering::Relaxed);
            ctx.scan_pending.store(true, Ordering::Relaxed);
        }
    });
}

fn system_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "업데이트 설정", |ui| update_section(ui, draft));
}

fn update_section(ui: &mut egui::Ui, draft: &mut Value) {
    let app_update = object_section_mut(draft, "app_update");
    form_row(ui, "자동 업데이트", |ui| {
        let mut enabled = app_update
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if ui
            .checkbox(&mut enabled, RichText::new("사용").size(Theme::FONT_BODY))
            .changed()
        {
            app_update.insert("enabled".into(), json!(enabled));
        }
    });
    ui.add_space(Theme::ROW_SPACING);
    form_row(ui, "버전 정보", |ui| {
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .color(Theme::TEXT_PRIMARY)
                .size(Theme::FONT_BODY),
        );
    });
}

pub fn render_settings_deferred(
    ctx: &egui::Context,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Theme::PANEL_BG)
                    .inner_margin(Margin::same(24)),
            )
            .show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
    }
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out`.
pub fn save_settings_to_disk(
    root: &Path,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    normalize_settings(draft);
    let diff = diff_settings(base, draft);
    save_user_settings(root, &diff).map_err(|e| e.to_string())?;
    *merged_out = load_merged_settings(root, defaults.clone());
    Ok(())
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn section_frame(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0_f32, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                // Vertical accent line
                let (rect, _) = ui.allocate_at_least(egui::vec2(3.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 1.5, Theme::PRIMARY);
                ui.add_space(8.0);

                ui.label(
                    RichText::new(title)
                        .color(Theme::TEXT_PRIMARY)
                        .size(Theme::FONT_BODY)
                        .strong(),
                );
            });
            ui.add_space(16.0);
            add(ui);
        });
    ui.add_space(16.0);
}

/// A helper to render a label with fixed width followed by controls, ensuring perfect alignment.
fn form_row(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.add_sized(
            egui::vec2(Theme::LABEL_WIDTH, Theme::CONTROL_HEIGHT),
            egui::Label::new(
                RichText::new(label)
                    .color(Theme::TEXT_SECONDARY)
                    .size(Theme::FONT_BODY),
            ),
        );
        ui.add_space(8.0);
        add_contents(ui);
    });
}

fn object_section_mut<'a>(draft: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    let root = draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object");
    let entry = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
}

fn user_entry_mut<'a>(draft: &'a mut Value, steam_id: &str) -> &'a mut Map<String, Value> {
    let varchive = object_section_mut(draft, "varchive");
    let user_map_value = varchive
        .entry("user_map")
        .or_insert_with(|| Value::Object(Map::new()));
    if !user_map_value.is_object() {
        *user_map_value = Value::Object(Map::new());
    }
    let user_map = user_map_value
        .as_object_mut()
        .expect("user_map must be verified as a JSON Object");
    let entry = user_map
        .entry(steam_id)
        .or_insert_with(|| json!({"v_id": "", "account_path": ""}));
    if let Some(v_id) = entry.as_str().map(str::to_string) {
        *entry = json!({"v_id": v_id, "account_path": ""});
    }
    if !entry.is_object() {
        *entry = json!({"v_id": "", "account_path": ""});
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
}

#[cfg(test)]
mod tests {
    use super::save_settings_to_disk;
    use overmax_data::load_merged_settings;
    use serde_json::json;
    use std::fs;

    #[test]
    fn save_user_roundtrip_matches_python_delta_policy() {
        let root =
            std::env::temp_dir().join(format!("overmax-app-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("settings.json"),
            r#"{"overlay":{"scale":1.0,"base_opacity":0.8},"window_tracker":{"poll_interval_sec":0.5}}"#,
        )
        .unwrap();
        fs::write(root.join("settings.user.json"), "{}").unwrap();

        let defaults = json!({
            "overlay": {"scale": 1.0, "base_opacity": 0.8},
            "window_tracker": {"poll_interval_sec": 0.5}
        });
        let base = overmax_data::load_base_settings(&root, defaults.clone());
        let mut merged = load_merged_settings(&root, defaults.clone());
        let mut draft = merged.clone();
        draft["overlay"]["base_opacity"] = json!(0.55);

        save_settings_to_disk(&root, &defaults, &base, &mut draft, &mut merged).unwrap();

        let reloaded = load_merged_settings(&root, defaults);
        assert_eq!(reloaded["overlay"]["base_opacity"], json!(0.55));
        assert_eq!(reloaded["overlay"]["scale"], json!(1.0));

        let user_text = fs::read_to_string(root.join("settings.user.json")).unwrap();
        assert!(user_text.contains("base_opacity"));

        let _ = fs::remove_dir_all(root);
    }
}
