//! 설정 창의 고급 탭: 저장 위치·디버그·IPC·캡처·업데이트.

#[cfg(target_os = "windows")]
use crate::ui::dialog_theme::segmented_row;
use crate::ui::dialog_theme::{
    field_row, section_card, setting_row, text_input_with_button, DialogTheme,
};
use eframe::egui::{self, Color32, RichText};
#[cfg(target_os = "windows")]
use eframe::egui::{CornerRadius, Stroke};

use super::{object_section_mut, SettingsUiContext};
use serde_json::{json, Map, Value};
use std::sync::atomic::Ordering;

pub(super) fn advanced_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
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
        #[cfg(all(target_os = "linux", feature = "portal-capture"))]
        crate::ui::platform::portal_capture_ui::render(ui, &ctx.portal);
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
