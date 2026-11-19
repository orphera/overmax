//! Windows worker tick: Win32 window tracking with DXGI/GDI capture.

use super::*;

pub(super) fn initialize_winrt(log_tx: &Sender<String>) {
    use ::windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        let _ = log_tx.send(format!("[Detection] WinRT init failed: {e}"));
    }
}

/// Windows-only worker state. Mirrors `linux::PlatformState`.
pub(super) struct PlatformState {
    pub(super) cached_window_id: Option<u64>,
}

impl PlatformState {
    pub(super) fn new() -> Self {
        Self {
            cached_window_id: None,
        }
    }
}

impl DetectionWorker {
    pub(super) fn configure_capture(&self, capturer: &mut AdaptiveCaptureEngine) {
        let pref = self
            .settings
            .screen_capture()
            .engine
            .parse()
            .unwrap_or_default();
        capturer.set_preferred_engine(pref);
        let atlas_enabled = self.settings.screen_capture().enable_gpu_atlas;
        capturer.set_enable_gpu_atlas(atlas_enabled);
        let white_level = self.settings.screen_capture().hdr_sdr_white_level;
        capturer.set_hdr_sdr_white_level(white_level);
    }

    pub(super) fn sync_capture_settings(
        &self,
        new_settings: &Settings,
        capturer: &mut Box<dyn CaptureEngine>,
    ) {
        let old_pref: overmax_capture::PreferredCaptureEngine = self
            .settings
            .screen_capture()
            .engine
            .parse()
            .unwrap_or_default();
        let new_pref: overmax_capture::PreferredCaptureEngine = new_settings
            .screen_capture()
            .engine
            .parse()
            .unwrap_or_default();
        if old_pref != new_pref {
            self.log(format!(
                "[Detection] capture backend updated: {old_pref:?} -> {new_pref:?}"
            ));
            capturer.set_preferred_engine(new_pref);
        }
        let old_atlas = self.settings.screen_capture().enable_gpu_atlas;
        let new_atlas = new_settings.screen_capture().enable_gpu_atlas;
        if old_atlas != new_atlas {
            self.log(format!(
                "[Detection] gpu atlas updated: {old_atlas} -> {new_atlas}"
            ));
            capturer.set_enable_gpu_atlas(new_atlas);
        }
        let old_white_level = self.settings.screen_capture().hdr_sdr_white_level;
        let new_white_level = new_settings.screen_capture().hdr_sdr_white_level;
        if old_white_level != new_white_level {
            self.log(format!(
                "[Detection] hdr sdr white level updated: {old_white_level:?} -> {new_white_level:?}"
            ));
            capturer.set_hdr_sdr_white_level(new_white_level);
        }
    }

    pub(super) fn initial_detecting_output(&self, rect: WindowRect) -> DetectionOutput {
        self.detecting_output(Some(rect), None, None)
    }

    pub(super) fn tick_platform(
        &mut self,
        tracker: &mut WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
    ) -> TickFlow {
        self.tick(tracker, capturer, pipeline);
        TickFlow::Sleep
    }

    pub(super) fn tick(
        &mut self,
        tracker: &WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
    ) {
        let (rect, foreground) = if self.window_scheduler.should_query() {
            let target = tracker.game_target();
            let id = target.map(|(id, _)| id);
            let r = target.map(|(_, rect)| rect);
            let resized = self
                .window_scheduler
                .cached_rect
                .zip(r)
                .is_some_and(|(old, new)| (old.width, old.height) != (new.width, new.height));
            if self.platform.cached_window_id != id || resized {
                pipeline.reset();
                if self.was_found {
                    self.send_detection_output(self.detecting_output(r, None, None));
                    self.request_repaint();
                }
            }
            self.platform.cached_window_id = id;
            let f = tracker.is_foreground();
            self.window_scheduler.cached_fullscreen = tracker.is_fullscreen();
            self.window_scheduler.update(r, f);
            (r, f)
        } else {
            (
                self.window_scheduler.cached_rect,
                self.window_scheduler.cached_foreground,
            )
        };

        let Some(rect) = rect else {
            pipeline.reset();
            self.on_window_missing();
            return;
        };
        if !foreground && self.is_foreground {
            pipeline.reset();
            self.send_detection_output(self.detecting_output(Some(rect), None, None));
        }
        if !self.on_window_found(rect, foreground) {
            return;
        }
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let Some(telemetry) = &self.runtime_telemetry {
            telemetry.record_capture_attempt();
        }
        let cap_start = Instant::now();
        let cap_res = capturer.capture_bgra_inplace(rect, &mut self.frame_buffer);
        let cap_elapsed = cap_start.elapsed().as_micros() as u64;
        pipeline.stats.capture.update(cap_elapsed);

        match cap_res {
            Ok(_) => {
                self.capture_failure_active = false;
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                if let Some(telemetry) = &self.runtime_telemetry {
                    telemetry.record_capture_success(cap_elapsed);
                }
                let detect_start = Instant::now();
                let mut out =
                    pipeline.detect(&self.frame_buffer, self.start.elapsed().as_secs_f64());
                let detect_elapsed = detect_start.elapsed().as_micros() as u64;
                pipeline.stats.detect.update(detect_elapsed);

                out.telemetry_snapshot = pipeline.stats.maybe_take_snapshot(5.0);
                if let Some(ref snap) = out.telemetry_snapshot {
                    self.log_telemetry_snapshot(snap);
                }
                out.game_rect = Some(rect);
                out.state.is_fullscreen = self.window_scheduler.cached_fullscreen;
                self.log_detection_summary(&out);
                self.check_and_log_scene_transition(&out);
                self.log_verified_event(&out);

                let fingerprint = RepaintFingerprint::from(&out);

                let state_changed = self.last_fingerprint.as_ref() != Some(&fingerprint);
                if state_changed {
                    self.last_fingerprint = Some(fingerprint);
                }

                self.last_sleep_hint = out.sleep_hint;
                self.send_detection_output(out);
                if state_changed {
                    self.request_repaint();
                }
            }
            Err(e) => {
                pipeline.reset();
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                if let Some(telemetry) = &self.runtime_telemetry {
                    telemetry.record_capture_failure();
                }
                self.log_detection_throttled(format!("[Detection] capture failed: {e}"));
                if !self.capture_failure_active
                    || capturer.error_action() == CaptureErrorAction::Stop
                {
                    let fatal = (capturer.error_action() == CaptureErrorAction::Stop).then_some(e);
                    self.send_detection_output(self.detecting_output(Some(rect), None, fatal));
                    self.request_repaint();
                }
                self.capture_failure_active = true;
            }
        }
    }

    fn on_window_missing(&mut self) {
        if self.was_found {
            self.send_detection_output(self.detecting_output(None, None, None));
            self.request_repaint();
            self.log("[WindowTracker] game window lost".into());
        }
        self.was_found = false;
        self.is_foreground = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use overmax_data::Settings;
    use std::sync::mpsc;
    #[test]
    fn windows_window_missing_emits_detecting_output() {
        let (log_tx, _log_rx) = mpsc::channel();
        let (game_tx, _game_rx) = mpsc::channel();
        let (detection_tx, detection_rx) = mpsc::channel();
        let mut worker = DetectionWorker::new(
            std::path::PathBuf::new(),
            Settings::default(),
            std::sync::Arc::new(std::sync::Mutex::new(serde_json::Value::Null)),
            log_tx,
            game_tx,
            detection_tx,
            None,
            std::sync::Arc::new(std::sync::Mutex::new(None)),
            Box::new(|| {}),
        );
        worker.was_found = true;
        worker.on_window_missing();

        assert!(!worker.was_found);
        let out = detection_rx.recv().unwrap();
        assert!(!out.scene_detected);
        assert!(out.state.context.is_none());
        assert!(out.game_rect.is_none());
        assert!(out.capture_fatal.is_none());
    }
}
