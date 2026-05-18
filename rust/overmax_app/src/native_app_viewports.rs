//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, Color32, Frame, Vec2, ViewportBuilder, ViewportCommand};
use std::sync::atomic::Ordering;

use crate::debug_ui;
use crate::native_app::NativeApp;
use crate::native_helpers;
use crate::overlay_ui;
use crate::settings_ui;
use crate::sync_ui;

impl NativeApp {
    fn show_debug_viewport(&self, ctx: &egui::Context) {
        if !self.debug_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.debug_open.clone();
        let lines = self.log_lines.clone();
        let title = self.debug_title();
        ctx.show_viewport_deferred(
            native_helpers::vp_debug(),
            ViewportBuilder::default()
                .with_title(&title)
                .with_inner_size([720.0, 420.0]),
            move |ctx, class| {
                debug_ui::render_debug(ctx, class, &title, &lines);
                debug_ui::close_if_requested(ctx, &open);
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
        ctx.show_viewport_deferred(
            native_helpers::vp_settings(),
            ViewportBuilder::default()
                .with_title("Overmax 설정")
                .with_inner_size([440.0, 520.0]),
            move |ctx, class| {
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
                        }
                    });
                });
                settings_ui::render_settings_deferred(ctx, class, "설정", &mut local_draft);
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
        ctx.show_viewport_deferred(
            native_helpers::vp_sync(),
            ViewportBuilder::default()
                .with_title("V-Archive 동기화")
                .with_inner_size([520.0, 560.0]),
            move |ctx, class| {
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

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        self.drain_logs();
        self.poll_scan_requests();
        self.poll_upload_requests();
        self.drain_sync_scan();
        self.drain_upload_results();
        self.drain_game_found_refresh_steam();
        self.apply_overlay_visual(ctx);

        let debug_on = self.debug_open.load(Ordering::Relaxed);
        let sync_on = self.sync_open.load(Ordering::Relaxed);
        let overlay_on = self.overlay_visible.load(Ordering::Relaxed);
        let blocking_viewport_open = settings_on || sync_on || debug_on;
        let hidden_size = Vec2::new(1.0, 1.0);
        let visible_size = Vec2::new(overlay_ui::WIDTH, overlay_ui::HEIGHT);

        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(
            !overlay_on || blocking_viewport_open,
        ));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(if overlay_on {
            visible_size
        } else {
            hidden_size
        }));

        self.show_debug_viewport(ctx);
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
                    self.settings_open.clone(),
                    self.debug_open.clone(),
                    self.sync_open.clone(),
                );
                if actions.start_drag {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            });
    }
}
