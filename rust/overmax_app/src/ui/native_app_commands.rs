use std::sync::atomic::Ordering;

use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::ui_command::UiCommand;

fn set_overlay_position(settings: &mut serde_json::Value, x: i32, y: i32) -> bool {
    let Some(settings) = settings.as_object_mut() else {
        return false;
    };
    let overlay = settings
        .entry("overlay")
        .or_insert_with(|| serde_json::json!({}));
    let Some(overlay) = overlay.as_object_mut() else {
        return false;
    };
    let position = overlay
        .entry("position")
        .or_insert_with(|| serde_json::json!({}));
    let Some(position) = position.as_object_mut() else {
        return false;
    };
    position.insert("x".into(), serde_json::json!(x));
    position.insert("y".into(), serde_json::json!(y));
    true
}

impl NativeApp {
    pub(crate) fn drain_ui_commands(&mut self) -> bool {
        let mut handled = false;
        while let Ok(command) = self.ui_cmd_rx.try_recv() {
            self.handle_ui_command(command);
            handled = true;
        }
        handled
    }

    pub(crate) fn handle_ui_command(&self, command: UiCommand) {
        match command {
            UiCommand::OpenSettings => self.open_settings(),
            UiCommand::OpenDebug => self.open_debug(),
            UiCommand::OpenSync => self.open_sync(),
            UiCommand::Exit => self.exit_requested.store(true, Ordering::Relaxed),
            UiCommand::UploadCurrentPattern =>
            {
                #[cfg(target_os = "linux")]
                if let Ok(holder) = self.ctx_holder.lock() {
                    if let Some(ctx) = holder.as_ref() {
                        self.upload_current_pattern(ctx.clone());
                    }
                }
            }
            UiCommand::SetOverlayPosition { x, y } => {
                self.save_overlay_position(x, y);
            }
        }
    }

    fn save_overlay_position(&self, x: i32, y: i32) {
        let max_log_lines = self.max_log_lines();
        let base = overmax_core::lock_clone_or_default(&self.settings.base);
        let Ok(mut merged) = self.settings.merged.lock() else {
            return;
        };
        let mut updated = merged.clone();
        if !set_overlay_position(&mut updated, x, y) {
            return;
        }

        let result = crate::ui::settings_ui::save_settings_to_disk(
            self.root.as_ref(),
            self.settings.defaults.as_ref(),
            &base,
            &mut updated,
            &mut merged,
        );
        if result.is_err() {
            let _ = set_overlay_position(&mut merged, x, y);
        }
        drop(merged);
        if let Ok(mut draft) = self.settings.draft.lock() {
            let _ = set_overlay_position(&mut draft, x, y);
        }
        let message = match result {
            Ok(()) => format!("[Overlay] position saved: ({x},{y})"),
            Err(error) => format!("[Overlay] position save failed: {error}"),
        };
        debug_ui::push_log(&self.debug_state.log_lines, max_log_lines, message);
    }

    fn open_settings(&self) {
        if self.ui_state.settings_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(
                        crate::system::native_helpers::vp_settings(),
                        eframe::egui::ViewportCommand::Focus,
                    );
                }
            }
            return;
        }
        self.ui_state.settings_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open settings".into(),
        );
    }

    fn open_debug(&self) {
        if self.ui_state.debug_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(
                        crate::system::native_helpers::vp_debug(),
                        eframe::egui::ViewportCommand::Focus,
                    );
                }
            }
            return;
        }
        self.ui_state.debug_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open debug".into(),
        );
    }

    fn open_sync(&self) {
        if self.ui_state.sync_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(
                        crate::system::native_helpers::vp_sync(),
                        eframe::egui::ViewportCommand::Focus,
                    );
                }
            }
            return;
        }
        self.ui_state.sync_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open sync".into(),
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn position_merge_preserves_unsaved_overlay_fields() {
        let mut settings = serde_json::json!({
            "overlay": {
                "scale": 1.25,
                "position": { "snap": "manual", "x": 1, "y": 2 }
            }
        });

        assert!(super::set_overlay_position(&mut settings, 30, 40));

        assert_eq!(settings["overlay"]["scale"], 1.25);
        assert_eq!(settings["overlay"]["position"]["snap"], "manual");
        assert_eq!(settings["overlay"]["position"]["x"], 30);
        assert_eq!(settings["overlay"]["position"]["y"], 40);
    }
}
