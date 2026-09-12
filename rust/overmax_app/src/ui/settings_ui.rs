//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use crate::ui::dialog_theme::{
    apply_dialog_style, field_row, render_dialog_tabs, rtl_slider, section_card, segmented_row,
    setting_row, text_input_full, text_input_with_button, DialogTheme,
};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, ViewportClass};
use overmax_data::{
    diff_settings, load_merged_settings_from_paths, normalize_settings, save_user_settings_to_path,
    SettingsPaths,
};

use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct SettingsUiContext {
    pub root: Arc<std::path::PathBuf>,
    pub paths: Arc<overmax_data::AppPaths>,
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub debug_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
    pub steam_users:
        Arc<Mutex<std::collections::HashMap<String, crate::system::steam_session::SteamUser>>>,
    /// IPC 서버 실제 바인딩 포트 (None = 비활성)
    pub ipc_bound_port: crate::system::ipc_server::BoundPortSlot,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_dialog_style(ui.ctx());

    ui.add_space(DialogTheme::GAP_SM);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Overmax")
                .color(DialogTheme::TEXT_ACCENT)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
        ui.label(
            RichText::new(crate::t!("settings-title"))
                .color(DialogTheme::TEXT_PRIMARY)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
    });

    ui.add_space(DialogTheme::GAP_LG);

    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    render_dialog_tabs(
        ui,
        "settings_tabs",
        &[
            crate::t!("settings-tab-general"),
            crate::t!("settings-tab-recommend"),
            crate::t!("settings-tab-varchive"),
            crate::t!("settings-tab-advanced"),
        ],
        &mut active,
    );
    ui.data_mut(|d| d.insert_temp(id, active));

    ui.add_space(DialogTheme::GAP_XL);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match active {
            0 => general_tab(ui, draft),
            1 => recommend_tab(ui, draft),
            2 => varchive_tab(ui, draft, ctx),
            _ => advanced_tab(ui, draft, ctx),
        });
}

fn general_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_card(ui, crate::t!("settings-general"), |ui| {
        general_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-overlay-section"), |ui| {
        overlay_section(ui, draft);
    });
}

fn recommend_tab(ui: &mut egui::Ui, draft: &mut Value) {
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

fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
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

fn advanced_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_card(ui, crate::t!("settings-ipc-section"), |ui| {
        ipc_section(ui, draft, ctx);
    });
    section_card(ui, crate::t!("settings-storage-section"), |ui| {
        storage_section(ui, ctx);
    });
    section_card(ui, crate::t!("settings-diagnostics"), |ui| {
        debug_section(ui, draft, ctx);
    });
    section_card(ui, crate::t!("settings-screen-capture"), |ui| {
        capture_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-update-section"), |ui| {
        update_section(ui, draft);
    });
    #[cfg(target_os = "linux")]
    {
        section_card(ui, crate::t!("Linux 앱 실행"), |ui| {
            setting_row(ui, crate::t!("앱 메뉴"), "", |ui| {
                if ui.button(crate::t!("바로가기 생성")).clicked() {
                    let result = crate::system::desktop_entry_linux::install(ctx.root.as_path());
                    let (title, description, level) = match result {
                        Ok(path) => (
                            "Overmax",
                            crate::t!("sys-shortcut-create-success", path = path.display()),
                            rfd::MessageLevel::Info,
                        ),
                        Err(error) => (
                            "Overmax",
                            crate::t!("sys-shortcut-create-failed", error = error),
                            rfd::MessageLevel::Error,
                        ),
                    };
                    let _ = rfd::MessageDialog::new()
                        .set_title(title)
                        .set_description(description)
                        .set_level(level)
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show();
                }
            });
        });
    }
}

fn storage_section(ui: &mut egui::Ui, ctx: &SettingsUiContext) {
    let mode_text = if ctx.paths.is_portable() {
        crate::t!("settings-storage-mode-portable")
    } else {
        crate::t!("settings-storage-mode-installed")
    };

    setting_row(
        ui,
        crate::t!("settings-storage-mode"),
        crate::t!("settings-storage-mode-hint"),
        |ui| {
            ui.label(
                RichText::new(mode_text)
                    .color(DialogTheme::TEXT_PRIMARY)
                    .size(DialogTheme::FONT_BODY)
                    .strong(),
            );
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    let data_dir = ctx.paths.data_dir();
    let mut data_dir_str = data_dir.to_string_lossy().to_string();

    field_row(
        ui,
        crate::t!("settings-storage-folder"),
        crate::t!("settings-storage-folder-hint"),
        |ui| {
            let (_, clicked) = text_input_with_button(
                ui,
                &mut data_dir_str,
                "",
                crate::t!("settings-storage-open-folder"),
                true,
            );
            if clicked {
                let _ = crate::system::native_helpers::open_folder(data_dir);
            }
        },
    );
}

fn debug_section(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let mut is_open = ctx.debug_open.load(Ordering::Relaxed);

    setting_row(
        ui,
        crate::t!("settings-debug-window"),
        crate::t!("settings-debug-window-hint"),
        |ui| {
            if ui
                .checkbox(&mut is_open, crate::t!("settings-show-debug-window"))
                .on_hover_text(crate::t!("settings-debug-window-desc"))
                .changed()
            {
                let debug_obj = object_section_mut(draft, "debug");
                debug_obj.insert("enabled".to_string(), Value::Bool(is_open));
                ctx.debug_open.store(is_open, Ordering::Relaxed);
                ui.ctx().request_repaint();
            }
        },
    );
}

/// 방송 및 외부 앱 연동(로컬 IPC) 섹션: 실시간 데이터 공유 토글 + 포트 설정 + 친절한 안내.
fn ipc_section(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let ipc_obj = object_section_mut(draft, "ipc");

    let mut enabled = ipc_obj
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // ── 기능 안내 설명 문구 ──
    ui.label(
        RichText::new(crate::t!("settings-ipc-desc"))
            .size(DialogTheme::FONT_BODY)
            .color(DialogTheme::TEXT_MUTED),
    );
    ui.add_space(DialogTheme::GAP_SM);

    // ── 데이터 공유 토글 ──
    setting_row(
        ui,
        crate::t!("settings-ipc-enable"),
        crate::t!("settings-ipc-enable-hint"),
        |ui| {
            if ui.checkbox(&mut enabled, "").changed() {
                ipc_obj.insert("enabled".to_string(), Value::Bool(enabled));
            }
        },
    );

    // ── 상태 뱃지 및 상세 설정 (활성화 시에만 노출) ──
    if enabled {
        ui.add_space(DialogTheme::GAP_SM);
        let bound = ctx.ipc_bound_port.lock().ok().and_then(|g| *g);
        let (status_text, status_color) = if let Some(port) = bound {
            (
                format!(
                    "🟢 {} · http://127.0.0.1:{port}",
                    crate::t!("settings-ipc-status-running")
                ),
                Color32::from_rgb(100, 220, 100),
            )
        } else {
            (
                format!("⚪ {}", crate::t!("settings-ipc-status-stopped")),
                DialogTheme::TEXT_MUTED,
            )
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(status_text)
                    .color(status_color)
                    .size(DialogTheme::FONT_BODY)
                    .strong(),
            );
        });

        // ── 포트 설정 ──
        let current_port = ipc_obj.get("port").and_then(Value::as_u64).unwrap_or(30110);
        let mut port_text = port_draft_text(ipc_obj, current_port);
        ui.add_space(DialogTheme::GAP_SM);
        setting_row(
            ui,
            crate::t!("settings-ipc-port"),
            crate::t!("settings-ipc-port-hint"),
            |ui| {
                let text_edit = egui::TextEdit::singleline(&mut port_text)
                    .font(egui::FontId::new(
                        DialogTheme::FONT_BODY,
                        egui::FontFamily::Monospace,
                    ))
                    .vertical_align(egui::Align::Center);
                let response =
                    ui.add_sized(egui::vec2(90.0, DialogTheme::CONTROL_HEIGHT), text_edit);
                if response.changed() {
                    let cleaned: String =
                        port_text.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(port) = cleaned.parse::<u16>() {
                        if (1024..=65535).contains(&port) {
                            ipc_obj.insert("port".to_string(), json!(port));
                        }
                    }
                }
            },
        );
    }
}

/// 포트 임시 편집 문자열. 확정된 값과 다른 미완성 입력을 유지하기 위한 최소 장치.
fn port_draft_text(ipc_obj: &Map<String, Value>, fallback: u64) -> String {
    match ipc_obj.get("port").and_then(Value::as_u64) {
        Some(p) => p.to_string(),
        None => fallback.to_string(),
    }
}

fn capture_section(ui: &mut egui::Ui, draft: &mut Value) {
    let screen_capture = object_section_mut(draft, "screen_capture");

    #[cfg(target_os = "windows")]
    {
        let engine = screen_capture
            .get("engine")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string();

        segmented_row(
            ui,
            crate::t!("settings-capture-method-win"),
            crate::t!("settings-capture-engine-hint"),
            |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

                    // right_to_left 이므로 역순으로 추가하여 화면에는 [자동 | DXGI | GDI] 순으로 배치
                    for (label, val) in [
                        (crate::t!("settings-capture-mode-gdi"), "gdi"),
                        (crate::t!("settings-capture-mode-dxgi"), "dxgi"),
                        (crate::t!("settings-capture-mode-auto"), "auto"),
                    ] {
                        let is_active = engine == val;
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
                            .add_sized(egui::vec2(108.0, DialogTheme::CONTROL_HEIGHT), btn)
                            .clicked()
                        {
                            screen_capture.insert("engine".into(), json!(val));
                        }
                    }
                });
            },
        );

        ui.add_space(DialogTheme::GAP_MD);
    }

    let mut content_protected = screen_capture
        .get("content_protected")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    setting_row(
        ui,
        crate::t!("settings-protect-overlay"),
        crate::t!("settings-protect-overlay-hint"),
        |ui| {
            let response = ui
                .checkbox(
                    &mut content_protected,
                    crate::t!("settings-prevent-screen-capture"),
                )
                .on_hover_text(crate::t!("settings-protect-overlay-desc"));

            if response.changed() {
                screen_capture.insert("content_protected".into(), json!(content_protected));
            }
        },
    );
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

fn update_section(ui: &mut egui::Ui, draft: &mut Value) {
    if crate::system::updater::is_self_update_supported() {
        let app_update = object_section_mut(draft, "app_update");
        let mut enabled = app_update
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        setting_row(
            ui,
            crate::t!("settings-auto-update"),
            crate::t!("settings-auto-update-hint"),
            |ui| {
                if ui
                    .checkbox(
                        &mut enabled,
                        RichText::new(crate::t!("settings-use")).size(DialogTheme::FONT_BODY),
                    )
                    .changed()
                {
                    app_update.insert("enabled".into(), json!(enabled));
                }
            },
        );

        ui.add_space(DialogTheme::GAP_MD);
    }

    setting_row(ui, crate::t!("settings-version-info"), "", |ui| {
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .color(DialogTheme::TEXT_PRIMARY)
                .size(DialogTheme::FONT_BODY),
        );
    });
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

pub fn render_settings_deferred(
    ui: &mut egui::Ui,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::EmbeddedWindow {
        egui::Window::new(title).show(ui.ctx(), |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(DialogTheme::BG_WINDOW)
                    .inner_margin(Margin::same(DialogTheme::PANEL_PADDING as i8)),
            )
            .show(ui, |ui| render_settings_form(ui, draft, settings_ctx));
    }
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out` using `SettingsPaths`.
pub fn save_settings_to_paths(
    paths: &SettingsPaths,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    normalize_settings(draft);
    let diff = diff_settings(base, draft);
    save_user_settings_to_path(&paths.settings_user_json, &diff).map_err(|e| e.to_string())?;
    *merged_out = load_merged_settings_from_paths(paths, defaults.clone());
    Ok(())
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out`.
pub fn save_settings_to_disk(
    root: &Path,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    save_settings_to_paths(
        &SettingsPaths::in_dir(root),
        defaults,
        base,
        draft,
        merged_out,
    )
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn root_map_mut(draft: &mut Value) -> &mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object")
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
    use overmax_data::{load_merged_settings, SettingsPaths};
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

    #[test]
    fn save_settings_to_paths_roundtrip() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("settings.json"),
            r#"{"overlay":{"scale":1.0,"base_opacity":0.8}}"#,
        )
        .unwrap();
        fs::write(root.join("settings.user.json"), "{}").unwrap();

        let paths = SettingsPaths::in_dir(root);
        let defaults = json!({"overlay": {"scale": 1.0, "base_opacity": 0.8}});
        let base = overmax_data::load_base_settings_from_paths(&paths, defaults.clone());
        let mut merged = overmax_data::load_merged_settings_from_paths(&paths, defaults.clone());
        let mut draft = merged.clone();
        draft["overlay"]["scale"] = json!(1.5);

        super::save_settings_to_paths(&paths, &defaults, &base, &mut draft, &mut merged).unwrap();

        let reloaded = overmax_data::load_merged_settings_from_paths(&paths, defaults);
        assert_eq!(reloaded["overlay"]["scale"], json!(1.5));
    }
}
