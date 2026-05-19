//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, Color32, Frame, Vec2, ViewportBuilder, ViewportCommand};
use std::sync::atomic::Ordering;

use crate::debug_ui;
use crate::native_app::NativeApp;
use crate::native_helpers;
use crate::overlay_ui;
use crate::settings_ui;
use crate::sync_ui;
use crate::window_tracker;

fn game_window_title(settings: &serde_json::Value) -> &str {
    settings
        .get("window_tracker")
        .and_then(|v| v.get("window_title"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("DJMAX RESPECT V")
}

fn is_mouse_over_overlay(ctx: &egui::Context, _scale: f32) -> bool {
    let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
        return false;
    };
    let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos);
    }
    // We no longer use pixels_per_point for scaling, so the mouse position 
    // from Windows is in physical pixels, and rect from egui is also effectively in 
    // physical pixels (since PPI=1.0).
    let mouse_pos = egui::pos2(pos.x as f32, pos.y as f32);
    rect.contains(mouse_pos)
}

impl NativeApp {
    fn auxiliary_viewport(title: &str, size: [f32; 2]) -> ViewportBuilder {
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_visible(true)
            .with_taskbar(false)
    }

    fn show_debug_viewport(&self, ctx: &egui::Context) {
        if !self.debug_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.debug_open.clone();
        let lines = self.log_lines.clone();
        let paused = self.debug_paused.clone();
        let roi = self.debug_roi.clone();
        let filters = self.debug_filters.clone();
        let title = self.debug_title();
        ctx.show_viewport_deferred(
            native_helpers::vp_debug(),
            Self::auxiliary_viewport(&title, [720.0, 460.0]),
            move |ctx, class| {
                debug_ui::render_debug(
                    ctx,
                    class,
                    &title,
                    &lines,
                    &paused,
                    &roi,
                    &filters,
                );
                debug_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_roi_viewport(&self, ctx: &egui::Context) {
        if !self.debug_roi.load(Ordering::Relaxed) {
            return;
        }
        let game_rect = {
            if let Ok(g) = self.game_rect.lock() {
                *g
            } else {
                None
            }
        };
        let Some(rect) = game_rect else {
            return;
        };

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("overmax_roi_vp"),
            egui::ViewportBuilder::default()
                .with_title("Overmax ROI Overlay")
                .with_decorations(false)
                .with_transparent(true)
                .with_mouse_passthrough(true)
                .with_inner_size([rect.width as f32, rect.height as f32])
                .with_position(eframe::egui::pos2(rect.left as f32, rect.top as f32))
                .with_always_on_top(),
            move |ctx, class| {
                ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(true));
                crate::roi_overlay_ui::render_roi_overlay(ctx, class, rect);
            },
        );
    }

    fn show_settings_viewport(&self, ctx: &egui::Context) {
        if !self.settings_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.settings_open.clone();
        let draft = self.settings_draft.clone();
        let root = self.root.clone();
        let defaults = self.defaults.clone();
        let base = self.base_settings.clone();
        let merged = self.merged_settings.clone();
        let settings_ctx = settings_ui::SettingsUiContext {
            current_steam_id: self
                .sync_steam_id
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
            sync_open: self.sync_open.clone(),
            scan_pending: self.scan_pending.clone(),
            sync_steam_id: self.sync_steam_id.clone(),
            fetch_tx: self.fetch_req_tx.clone(),
        };
        ctx.show_viewport_deferred(
            native_helpers::vp_settings(),
            Self::auxiliary_viewport("Overmax 설정", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
                let mut local_draft = draft.lock().map(|g| g.clone()).unwrap_or_default();
                egui::TopBottomPanel::bottom("sett_actions").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("저장").clicked() {
                            let base_g = base.lock().map(|g| g.clone()).unwrap_or_default();
                            let mut merged_g = merged.lock().map(|g| g.clone()).unwrap_or_default();
                            let _ = settings_ui::save_settings_to_disk(
                                root.as_ref(),
                                defaults.as_ref(),
                                &base_g,
                                &mut local_draft,
                                &mut merged_g,
                            );
                            if let Ok(mut m) = merged.lock() {
                                *m = merged_g;
                            }
                        }
                        if ui.button("닫기").clicked() {
                            open.store(false, Ordering::Relaxed);
                            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                        }
                    });
                });
                settings_ui::render_settings_deferred(
                    ctx,
                    class,
                    "설정",
                    &mut local_draft,
                    &settings_ctx,
                );
                if let Ok(mut d) = draft.lock() {
                    *d = local_draft;
                }
                settings_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_sync_viewport(&self, ctx: &egui::Context) {
        if !self.sync_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.sync_open.clone();
        let scan_pending = self.scan_pending.clone();
        let steam = self.sync_steam_id.clone();
        let status = self.sync_status.clone();
        let candidates = self.sync_candidates.clone();
        let upload_tx = self.upload_req_tx.clone();
        let delete_tx = self.delete_req_tx.clone();
        ctx.show_viewport_deferred(
            native_helpers::vp_sync(),
            Self::auxiliary_viewport("V-Archive 동기화", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
                let list = candidates.lock().map(|g| g.clone()).unwrap_or_default();
                let mut steam_g = steam.lock().unwrap_or_else(|e| e.into_inner());
                let status_s = status.lock().map(|g| g.clone()).unwrap_or_default();
                sync_ui::render_sync(
                    ctx,
                    class,
                    &mut *steam_g,
                    &status_s,
                    &list,
                    || {
                        scan_pending.store(true, Ordering::Relaxed);
                    },
                    |i| {
                        let _ = upload_tx.send(i);
                    },
                    |i| {
                        let _ = delete_tx.send(i);
                    },
                );
                sync_ui::close_if_requested(ctx, &open);
            },
        );
    }
}

impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.exit_requested.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let settings_on = self.settings_open.load(Ordering::Relaxed);
        if settings_on && !self.prev_settings_open {
            if let (Ok(m), Ok(mut d)) = (self.merged_settings.lock(), self.settings_draft.lock()) {
                *d = m.clone();
            }
        }
        self.prev_settings_open = settings_on;

        self.start_log_pump(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        self.drain_detection_results();
        if self.drain_ui_commands() {
            ctx.request_repaint();
        }
        self.poll_scan_requests();
        self.poll_upload_requests();
        self.poll_fetch_requests();
        self.drain_sync_scan();
        self.drain_upload_results();
        self.drain_fetch_results();
        self.poll_delete_requests();
        self.drain_game_found_refresh_steam();
        ctx.send_viewport_cmd(ViewportCommand::ContentProtected(true));

        let scale = if let Ok(m) = self.merged_settings.lock() {

            m.get("overlay").and_then(|o| o.get("scale")).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32
        } else {
            1.0
        };

        let overlay_on = self.overlay_visible.load(Ordering::Relaxed);
        let hidden_size = Vec2::new(1.0, 1.0);
        let visible_size = Vec2::new(overlay_ui::BASE_WIDTH * scale, overlay_ui::BASE_HEIGHT * scale);

        // 마우스가 오버레이 영역 위에 있을 때만 상호작용 가능하게 함 (보조창 조작을 위해)
        let is_over = is_mouse_over_overlay(ctx, scale);
        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(!overlay_on || !is_over));

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(if overlay_on {
            visible_size
        } else {
            hidden_size
        }));

        self.show_debug_viewport(ctx);
        self.show_roi_viewport(ctx);
        self.show_settings_viewport(ctx);
        self.show_sync_viewport(ctx);

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if !overlay_on {
                    ui.set_min_size(hidden_size);
                    return;
                }
                ui.set_min_size(visible_size);
                let actions = overlay_ui::draw_overlay_panel(
                    ui,
                    &self.session,
                    self.confidence,
                    &self.current_song_label(),
                    &self.pattern_tabs,
                    &self.recommendations,
                    self.settings_open.clone(),
                    self.debug_open.clone(),
                    self.sync_open.clone(),
                    scale,
                );

                if actions.start_drag {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if actions.restore_game_focus {
                    let max_log_lines = self.max_log_lines();
                    if let Ok(mut settings) = self.merged_settings.lock() {
                        window_tracker::restore_foreground_by_title(game_window_title(&settings));
                        
                        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                            if let Ok(mut draft) = self.settings_draft.lock() {
                                let mut overlay = settings.get("overlay").cloned().unwrap_or_else(|| serde_json::json!({}));
                                if let Some(overlay_obj) = overlay.as_object_mut() {
                                    overlay_obj.insert("position".to_string(), serde_json::json!({
                                        "x": rect.min.x.max(0.0) as i32,
                                        "y": rect.min.y.max(0.0) as i32
                                    }));
                                }
                                settings["overlay"] = overlay.clone();
                                draft["overlay"] = overlay;

                                let base_g = self.base_settings.lock().map(|g| g.clone()).unwrap_or_default();
                                let _ = settings_ui::save_settings_to_disk(
                                    self.root.as_ref(),
                                    self.defaults.as_ref(),
                                    &base_g,
                                    &mut draft,
                                    &mut settings,
                                );
                                debug_ui::push_log(
                                    &self.log_lines,
                                    max_log_lines,
                                    format!("[Overlay] 오버레이 위치 저장 (user.json): ({},{})", rect.min.x as i32, rect.min.y as i32),
                                );
                            }
                        }
                    }
                }
                if let Some(command) = actions.command {
                    self.handle_ui_command(command);
                    ctx.request_repaint();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::NativeApp;
    use eframe::egui;

    #[test]
    fn auxiliary_viewports_stay_out_of_taskbar() {
        let builder = NativeApp::auxiliary_viewport("debug", [720.0, 420.0]);

        assert_eq!(builder.taskbar, Some(false));
        assert_eq!(builder.visible, Some(true));
        assert_ne!(
            builder.window_level,
            Some(egui::viewport::WindowLevel::AlwaysOnTop)
        );
        assert_ne!(builder.active, Some(true));
    }

    #[test]
    fn game_window_title_uses_settings_or_python_default() {
        let settings = serde_json::json!({
            "window_tracker": {"window_title": "DJMAX TEST"}
        });

        assert_eq!(super::game_window_title(&settings), "DJMAX TEST");
        assert_eq!(
            super::game_window_title(&serde_json::json!({})),
            "DJMAX RESPECT V"
        );
    }
}
