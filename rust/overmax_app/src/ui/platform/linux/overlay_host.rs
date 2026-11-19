//! Linux overlay host: publishes `NativeApp` state to the Wayland layer overlay.

use eframe::egui;

use crate::ui::native_app::NativeApp;

impl NativeApp {
    pub(crate) fn publish_linux_overlay(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| now >= toast.expires_at)
        {
            self.toast = None;
        }
        if let Some(toast) = &self.toast {
            ctx.request_repaint_after(toast.expires_at.saturating_duration_since(now));
        }

        let overlay = self.settings.get_merged().overlay();
        let position = overlay
            .position
            .x
            .zip(overlay.position.y)
            .map(|(x, y)| (x as i32, y as i32));
        self.platform.linux_overlay.publish(
            crate::ui::platform::layer_overlay::LinuxOverlaySnapshot {
                state: self.session.clone(),
                song_label: self.current_song_label(),
                pattern_tabs: self.pattern_tabs.clone(),
                recommendations: self.recommendations.clone(),
                settings_open: self.ui_state.settings_open.clone(),
                sync_open: self.ui_state.sync_open.clone(),
                scale: overlay.scale as f32,
                opacity: overlay.base_opacity as f32,
                varchive_upload_needed: self.overlay_upload_needed,
                varchive_upload_progress: self.auto_upload.progress(std::time::Instant::now()),
                varchive_account_configured: self.is_varchive_account_configured(),
                lite_mode: overlay.lite_mode,
                always_visible: overlay.always_visible,
                snap: overlay.position.snap,
                position,
                record_manager: self.record_manager.clone(),
                session_initial_record: self.runtime.session_initial_record(),
                toast: self.toast.clone(),
                window_snapshot: self.window_snapshot,
                portal_presentation: {
                    #[cfg(feature = "portal-capture")]
                    {
                        self.portal.view().presentation.and_then(|confirmed| {
                            self.platform
                                .presentation_observation()
                                .lock()
                                .ok()
                                .and_then(|value| *value)
                                .filter(|current| current.target_epoch == confirmed.target_epoch)
                        })
                    }
                    #[cfg(not(feature = "portal-capture"))]
                    {
                        None
                    }
                },
                game_geometry: overmax_core::lock_or_recover(&self.platform.game_geometry).clone(),
                capture_fatal: self.capture_fatal.clone(),
                overlay_visible_override: self.overlay_visible_override,
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                delivery_telemetry: self
                    .last_detection_output
                    .as_ref()
                    .and_then(|output| output.delivery_telemetry),
            },
        );
    }
}
