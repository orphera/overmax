//! Linux worker tick: X11/Wayland focus policy, XComposite capture and the
//! optional native Wayland (portal) path.

use super::*;

pub(super) const BACKGROUND_DEBOUNCE: Duration = Duration::from_millis(300);
pub(super) const UNKNOWN_GRACE: Duration = Duration::from_secs(1);

pub(super) enum LinuxTickResult {
    Continue,
    Reconnect,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LinuxFocusLoss {
    Background,
    Unknown,
}

pub(super) struct LinuxFocusPolicy {
    observed: Option<FocusState>,
    observed_since: Option<Instant>,
    foreground: bool,
}

impl LinuxFocusPolicy {
    pub(super) fn new() -> Self {
        Self {
            observed: None,
            observed_since: None,
            foreground: false,
        }
    }

    pub(super) fn update(
        &mut self,
        observation: FocusObservation,
        committed_at: Instant,
        now: Instant,
    ) -> (bool, Option<LinuxFocusLoss>) {
        if self.observed != Some(observation.state) {
            self.observed = Some(observation.state);
            self.observed_since = Some(committed_at.min(now));
        }
        if observation.state == FocusState::Focused {
            self.foreground = true;
            return (true, None);
        }
        if !self.foreground {
            return (false, None);
        }
        let elapsed = now.saturating_duration_since(self.observed_since.unwrap_or(now));
        let loss = match observation.state {
            FocusState::Focused => None,
            FocusState::Background if elapsed >= BACKGROUND_DEBOUNCE => {
                Some(LinuxFocusLoss::Background)
            }
            FocusState::Unknown if elapsed >= UNKNOWN_GRACE => Some(LinuxFocusLoss::Unknown),
            FocusState::Background | FocusState::Unknown => None,
        };
        if loss.is_some() {
            self.foreground = false;
        }
        (self.foreground, loss)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::new();
    }
}

pub(super) fn capture_target_resized(
    previous: Option<WindowSnapshot>,
    current: Option<WindowSnapshot>,
) -> bool {
    previous.zip(current).is_some_and(|(previous, current)| {
        previous.window == current.window
            && (previous.rect.width != current.rect.width
                || previous.rect.height != current.rect.height)
    })
}

pub(super) fn effective_focus(
    x11: FocusObservation,
    presentation: Option<PresentationObservation>,
) -> FocusObservation {
    presentation.map_or(x11, |presentation| FocusObservation {
        state: presentation.focus,
        source: FocusSource::WaylandForeignToplevel,
        generation: presentation.generation,
    })
}

pub(super) fn initialize_winrt(_log_tx: &Sender<String>) {}

impl DetectionWorker {
    pub(super) fn tick_linux(
        &mut self,
        tracker: &WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
        #[cfg(feature = "portal-capture")] portal: &mut Option<PortalConnection>,
    ) -> LinuxTickResult {
        #[cfg(feature = "portal-capture")]
        if let Some(portal) = portal {
            let observation = self
                .presentation_observation
                .lock()
                .ok()
                .and_then(|value| *value);
            let fps =
                (1000 / self.settings.screen_capture().active_sleep_ms.max(1)).clamp(1, 30) as u32;
            let poll = portal.poll(
                || tracker.game_snapshot().map(|value| value.is_some()),
                || {
                    capturer.set_target(None)?;
                    PortalCapture::start(fps)
                },
                observation,
                &mut self.frame_buffer,
                Instant::now(),
            );
            if let Some(reset) = poll.reset {
                self.window_snapshot = None;
                self.focus_observation = None;
                self.focus_policy.reset();
                self.window_scheduler = WindowQueryScheduler::new(true);
                match reset {
                    ResetScope::Observations => {
                        pipeline.suspend();
                        self.publish_capture_interruption("native Wayland capture paused");
                    }
                    ResetScope::Session => {
                        self.on_capture_interrupted(
                            pipeline,
                            "native Wayland capture state changed",
                        );
                    }
                }
                if !poll.handling {
                    self.was_found = false;
                }
                self.is_foreground = false;
            }
            if poll.changed {
                self.request_repaint();
            }
            if poll.handling {
                if let Some((_stamp, observed)) = poll.frame {
                    let current = self
                        .presentation_observation
                        .lock()
                        .ok()
                        .and_then(|value| *value);
                    if !current.is_some_and(|value| {
                        value.target_epoch == observed.target_epoch
                            && value.focus == FocusState::Focused
                    }) {
                        if current.is_some_and(|value| {
                            value.target_epoch == observed.target_epoch
                                && value.focus == FocusState::Background
                        }) {
                            pipeline.suspend();
                            self.publish_capture_interruption("native Wayland capture paused");
                        } else {
                            self.on_capture_interrupted(
                                pipeline,
                                "native Wayland target changed during frame delivery",
                            );
                        }
                        self.is_foreground = false;
                        return LinuxTickResult::Continue;
                    }
                    if !self.was_found {
                        let _ = self.game_found_tx.send(());
                    }
                    self.was_found = true;
                    self.is_foreground = true;
                    self.process_linux_frame(
                        pipeline,
                        None,
                        observed.fullscreen.unwrap_or(false),
                        poll.changed,
                    );
                }
                return LinuxTickResult::Continue;
            }
        }
        let previous_snapshot = self.window_snapshot;
        let queried = self.window_scheduler.should_query();
        let mut target_changed = false;
        let mut target_resized = false;
        if queried {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            let snapshot_result =
                tracker
                    .game_snapshot_with_telemetry()
                    .map(|(observation, focus_telemetry)| {
                        if let (Some(telemetry), Some((snapshot, _)), Some(focus)) =
                            (&self.runtime_telemetry, observation, focus_telemetry)
                        {
                            telemetry.record_window_observation(
                                focus.target_xid,
                                focus.active_xid,
                                focus.active_in_client_list,
                                (
                                    snapshot.rect.left,
                                    snapshot.rect.top,
                                    snapshot.rect.width,
                                    snapshot.rect.height,
                                ),
                                snapshot.foreground,
                                snapshot.fullscreen,
                            );
                        }
                        observation
                    });
            #[cfg(not(any(debug_assertions, feature = "telemetry")))]
            let snapshot_result = tracker.game_snapshot();
            let observation = match snapshot_result {
                Ok(observation) => observation,
                Err(error) => {
                    self.on_capture_fatal(pipeline, format!("window tracking failed: {error}"));
                    return LinuxTickResult::Reconnect;
                }
            };
            let snapshot = observation.map(|(snapshot, _)| snapshot);
            self.focus_observation = observation.map(|(_, focus)| focus);
            target_changed = self.window_snapshot.map(|s| s.window) != snapshot.map(|s| s.window);
            target_resized = capture_target_resized(previous_snapshot, snapshot);
            if let Err(error) = capturer.set_target(snapshot) {
                return self.handle_linux_capture_error(capturer.as_ref(), pipeline, error);
            }
            self.window_snapshot = snapshot;
        }

        let presentation = self
            .presentation_observation
            .lock()
            .ok()
            .and_then(|observation| *observation);
        let focus_loss = match (self.window_snapshot.as_mut(), self.focus_observation) {
            (Some(snapshot), Some(x11_observation)) => {
                let now = Instant::now();
                let observation = effective_focus(x11_observation, presentation);
                let committed_at = presentation.map_or(now, |value| value.committed_at);
                if let Some(fullscreen) = presentation.and_then(|value| value.fullscreen) {
                    snapshot.fullscreen = fullscreen;
                }
                let (foreground, loss) = self.focus_policy.update(observation, committed_at, now);
                snapshot.foreground = foreground;
                loss
            }
            _ => {
                self.focus_policy.reset();
                None
            }
        };
        if queried {
            self.window_scheduler.update(
                self.window_snapshot.map(|snapshot| snapshot.rect),
                self.window_snapshot
                    .is_some_and(|snapshot| snapshot.foreground),
            );
        }
        let capture_state_changed = previous_snapshot
            .map(|current| (current.window, current.foreground, current.fullscreen))
            != self
                .window_snapshot
                .map(|current| (current.window, current.foreground, current.fullscreen));
        let overlay_snapshot_changed = previous_snapshot != self.window_snapshot;
        if capture_state_changed {
            self.capture_failure_active = false;
        }
        if target_changed && self.window_snapshot.is_some() && self.was_found {
            self.on_capture_interrupted(pipeline, "capture target changed");
        } else if let Some(loss) = focus_loss {
            let reason = match loss {
                LinuxFocusLoss::Background => "game window is in the background",
                LinuxFocusLoss::Unknown => "game window focus is unknown",
            };
            self.on_capture_interrupted(pipeline, reason);
        } else if target_resized {
            self.on_capture_interrupted(pipeline, "capture target resized");
        }

        let Some(snapshot) = self.window_snapshot else {
            self.on_linux_window_missing(pipeline);
            return LinuxTickResult::Continue;
        };
        if !self.on_window_found(snapshot.rect, snapshot.foreground) {
            return LinuxTickResult::Continue;
        }
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let Some(telemetry) = &self.runtime_telemetry {
            telemetry.record_capture_attempt();
        }
        let cap_start = Instant::now();
        let cap_res = capturer.capture_bgra_inplace(snapshot.rect, &mut self.frame_buffer);
        let cap_elapsed = cap_start.elapsed().as_micros() as u64;
        pipeline.stats.capture.update(cap_elapsed);

        match cap_res {
            Ok(()) => {
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                if let Some(telemetry) = &self.runtime_telemetry {
                    telemetry.record_capture_success(cap_elapsed);
                }
                self.process_linux_frame(
                    pipeline,
                    Some(snapshot),
                    snapshot.fullscreen,
                    overlay_snapshot_changed,
                );
            }
            Err(error) => {
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                if let Some(telemetry) = &self.runtime_telemetry {
                    telemetry.record_capture_failure();
                }
                return self.handle_linux_capture_error(capturer.as_ref(), pipeline, error);
            }
        }
        LinuxTickResult::Continue
    }

    pub(super) fn process_linux_frame(
        &mut self,
        pipeline: &mut DetectionPipeline,
        snapshot: Option<WindowSnapshot>,
        fullscreen: bool,
        overlay_changed: bool,
    ) {
        self.capture_failure_active = false;
        let detect_start = Instant::now();
        let mut out = pipeline.detect(&self.frame_buffer, self.start.elapsed().as_secs_f64());
        pipeline
            .stats
            .detect
            .update(detect_start.elapsed().as_micros() as u64);
        out.telemetry_snapshot = pipeline.stats.maybe_take_snapshot(5.0);
        if let Some(ref snap) = out.telemetry_snapshot {
            self.log_telemetry_snapshot(snap);
        }
        out.game_rect = snapshot.map(|value| value.rect);
        out.window_snapshot = snapshot;
        out.state.is_fullscreen = fullscreen;
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
        if state_changed || overlay_changed {
            self.request_repaint();
        }
    }

    pub(super) fn handle_linux_capture_error(
        &mut self,
        capturer: &dyn CaptureEngine,
        pipeline: &mut DetectionPipeline,
        error: String,
    ) -> LinuxTickResult {
        match capturer.error_action() {
            CaptureErrorAction::Retry => {
                self.on_capture_interrupted(pipeline, "capture failed");
                self.log_detection_throttled(format!("[Detection] capture failed: {error}"));
                LinuxTickResult::Continue
            }
            CaptureErrorAction::Reconnect => {
                self.on_capture_interrupted(pipeline, "capture connection lost");
                self.log(format!("[Detection] capture reconnect required: {error}"));
                LinuxTickResult::Reconnect
            }
            CaptureErrorAction::Stop => {
                self.on_capture_fatal(pipeline, error);
                LinuxTickResult::Stop
            }
        }
    }

    pub(super) fn linux_detecting_output(&self, capture_fatal: Option<String>) -> DetectionOutput {
        self.detecting_output(
            self.window_snapshot.map(|s| s.rect),
            self.window_snapshot,
            capture_fatal,
        )
    }

    pub(super) fn on_capture_interrupted(
        &mut self,
        pipeline: &mut DetectionPipeline,
        reason: &str,
    ) {
        pipeline.reset();
        self.publish_capture_interruption(reason);
    }

    pub(super) fn publish_capture_interruption(&mut self, reason: &str) {
        if self.capture_failure_active {
            return;
        }
        self.capture_failure_active = true;
        self.send_detection_output(self.linux_detecting_output(None));
        self.request_repaint();
        self.log(format!("[Detection] {reason}; state reset"));
    }

    pub(super) fn on_capture_fatal(&mut self, pipeline: &mut DetectionPipeline, error: String) {
        pipeline.reset();
        self.send_detection_output(self.linux_detecting_output(Some(error.clone())));
        self.request_repaint();
        self.log(format!("[Detection] capture unavailable: {error}"));
    }

    pub(super) fn on_linux_window_missing(&mut self, pipeline: &mut DetectionPipeline) {
        if self.was_found {
            self.on_capture_interrupted(pipeline, "game window lost");
            self.log("[WindowTracker] game window lost".into());
        }
        self.was_found = false;
        self.is_foreground = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection_worker::tests::snapshot;
    use overmax_capture::{CaptureEngine, CaptureErrorAction};
    use overmax_capture::{FocusObservation, FocusSource, FocusState, PresentationObservation};
    use overmax_data::ImageIndexDb;
    use overmax_data::Settings;
    use overmax_frame::CapturedFrame;
    use overmax_frame::WindowRect;
    use std::sync::mpsc;
    struct FailedCapture(CaptureErrorAction);

    fn focus(state: FocusState, generation: u64) -> FocusObservation {
        FocusObservation {
            state,
            source: FocusSource::EwmhActiveWindow,
            generation,
        }
    }

    impl CaptureEngine for FailedCapture {
        fn capture_bgra(&mut self, _rect: WindowRect) -> Result<CapturedFrame, String> {
            unreachable!()
        }

        fn capture_bgra_inplace(
            &mut self,
            _rect: WindowRect,
            _out_frame: &mut CapturedFrame,
        ) -> Result<(), String> {
            unreachable!()
        }

        fn error_action(&self) -> CaptureErrorAction {
            self.0
        }
    }

    #[test]
    fn resets_only_when_the_capture_size_changes() {
        let initial = snapshot(WindowRect {
            left: 20,
            top: 30,
            width: 1280,
            height: 720,
        });
        let moved = snapshot(WindowRect {
            left: 50,
            top: 60,
            ..initial.rect
        });
        let resized = snapshot(WindowRect {
            width: 1600,
            height: 900,
            ..initial.rect
        });

        assert!(!capture_target_resized(Some(initial), Some(moved)));
        assert!(capture_target_resized(Some(initial), Some(resized)));
    }

    #[test]
    fn debounces_background_and_graces_unknown_once() {
        let start = std::time::Instant::now();
        let mut policy = LinuxFocusPolicy::new();

        assert_eq!(
            policy.update(focus(FocusState::Focused, 1), start, start),
            (true, None)
        );
        let background_commit = start + std::time::Duration::from_millis(100);
        assert_eq!(
            policy.update(
                focus(FocusState::Background, 2),
                background_commit,
                background_commit + std::time::Duration::from_millis(299)
            ),
            (true, None)
        );
        assert_eq!(
            policy.update(
                focus(FocusState::Background, 2),
                background_commit,
                background_commit + std::time::Duration::from_millis(300)
            ),
            (false, Some(LinuxFocusLoss::Background))
        );
        assert_eq!(
            policy.update(
                focus(FocusState::Background, 2),
                background_commit,
                start + std::time::Duration::from_secs(2)
            ),
            (false, None)
        );

        let focused_commit = start + std::time::Duration::from_secs(3);
        assert_eq!(
            policy.update(
                focus(FocusState::Focused, 3),
                focused_commit,
                focused_commit
            ),
            (true, None)
        );
        let unknown_commit = focused_commit + std::time::Duration::from_millis(100);
        assert_eq!(
            policy.update(
                focus(FocusState::Unknown, 4),
                unknown_commit,
                unknown_commit
            ),
            (true, None)
        );
        assert_eq!(
            policy.update(
                focus(FocusState::Unknown, 4),
                unknown_commit,
                unknown_commit + std::time::Duration::from_millis(999)
            ),
            (true, None)
        );
        assert_eq!(
            policy.update(
                focus(FocusState::Unknown, 4),
                unknown_commit,
                unknown_commit + std::time::Duration::from_secs(1)
            ),
            (false, Some(LinuxFocusLoss::Unknown))
        );
        assert_eq!(
            policy.update(
                focus(FocusState::Unknown, 4),
                unknown_commit,
                unknown_commit + std::time::Duration::from_secs(2)
            ),
            (false, None)
        );
    }

    #[test]
    fn cold_start_non_foreground_is_fail_closed() {
        let now = std::time::Instant::now();
        for state in [FocusState::Background, FocusState::Unknown] {
            let mut policy = LinuxFocusPolicy::new();
            assert_eq!(policy.update(focus(state, 1), now, now), (false, None));
        }
    }

    #[test]
    fn publishes_initial_non_foreground_window_once() {
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
        let mut background = snapshot(WindowRect {
            left: 1920,
            top: 0,
            width: 2560,
            height: 1440,
        });
        background.foreground = false;
        worker.window_snapshot = Some(background);

        assert!(!worker.on_window_found(background.rect, background.foreground));
        assert_eq!(
            detection_rx.recv().unwrap().window_snapshot,
            Some(background)
        );
        assert!(!worker.on_window_found(background.rect, background.foreground));
        assert!(detection_rx.try_recv().is_err());
    }

    #[test]
    fn wayland_focus_overrides_x11_fallback() {
        let x11 = focus(FocusState::Background, 4);
        let presentation = PresentationObservation {
            target_epoch: 1,
            focus: FocusState::Focused,
            fullscreen: Some(true),
            generation: 7,
            committed_at: std::time::Instant::now(),
        };

        assert_eq!(effective_focus(x11, None), x11);
        assert_eq!(
            effective_focus(x11, Some(presentation)),
            FocusObservation {
                state: FocusState::Focused,
                source: FocusSource::WaylandForeignToplevel,
                generation: 7,
            }
        );
    }

    #[test]
    fn sends_detecting_once_per_capture_failure_streak() {
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
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));

        worker.on_capture_interrupted(&mut pipeline, "first");
        worker.on_capture_interrupted(&mut pipeline, "same streak");

        let first_streak: Vec<_> = detection_rx.try_iter().collect();
        assert_eq!(first_streak.len(), 1);
        assert!(first_streak[0].state.context.is_none());
        assert!(first_streak[0].capture_fatal.is_none());

        worker.capture_failure_active = false;
        worker.on_capture_interrupted(&mut pipeline, "next streak");
        assert_eq!(detection_rx.try_iter().count(), 1);
    }

    #[test]
    fn permanent_capture_error_stops_instead_of_reconnecting() {
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
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));

        let result = worker.handle_linux_capture_error(
            &FailedCapture(CaptureErrorAction::Stop),
            &mut pipeline,
            "unsupported pixel format".to_string(),
        );

        assert!(matches!(result, LinuxTickResult::Stop));
        assert_eq!(
            detection_rx.recv().unwrap().capture_fatal.as_deref(),
            Some("unsupported pixel format")
        );
    }
}
