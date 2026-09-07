//! Runtime detection worker: window tracking -> capture -> pipeline -> UI state.

use crate::capture::capture_engine::{AdaptiveCaptureEngine, CaptureEngine, CaptureErrorAction};
use crate::capture::frame::CapturedFrame;
#[cfg(target_os = "linux")]
use crate::capture::window_tracker::{
    FocusObservation, FocusSource, FocusState, PresentationObservation,
};
use crate::capture::window_tracker::{
    SharedPresentationObservation, WindowRect, WindowSnapshot, WindowTracker,
};
use crate::detector::detection_pipeline::{
    DetectionOutput, DetectionPipeline, JacketMatchStatus, SleepHint,
};
use crate::detector::telemetry::RuntimeTelemetry;
use overmax_core::GameSessionState;
use overmax_data::{DataCompatibility, ImageIndexDb, Settings};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LOG_INTERVAL: Duration = Duration::from_secs(3);
#[cfg(target_os = "linux")]
const BACKGROUND_DEBOUNCE: Duration = Duration::from_millis(300);
#[cfg(target_os = "linux")]
const UNKNOWN_GRACE: Duration = Duration::from_secs(1);

#[cfg(target_os = "linux")]
enum LinuxTickResult {
    Continue,
    Reconnect,
    Stop,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LinuxFocusLoss {
    Background,
    Unknown,
}

#[cfg(target_os = "linux")]
struct LinuxFocusPolicy {
    observed: Option<FocusState>,
    observed_since: Option<Instant>,
    foreground: bool,
}

#[cfg(target_os = "linux")]
impl LinuxFocusPolicy {
    fn new() -> Self {
        Self {
            observed: None,
            observed_since: None,
            foreground: false,
        }
    }

    fn update(
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

    fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(target_os = "linux")]
fn capture_target_resized(
    previous: Option<WindowSnapshot>,
    current: Option<WindowSnapshot>,
) -> bool {
    previous.zip(current).is_some_and(|(previous, current)| {
        previous.window == current.window
            && (previous.rect.width != current.rect.width
                || previous.rect.height != current.rect.height)
    })
}

#[cfg(target_os = "linux")]
fn effective_focus(
    x11: FocusObservation,
    presentation: Option<PresentationObservation>,
) -> FocusObservation {
    presentation.map_or(x11, |presentation| FocusObservation {
        state: presentation.focus,
        source: FocusSource::WaylandForeignToplevel,
        generation: presentation.generation,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    root: PathBuf,
    settings: Settings,
    merged_settings: Arc<Mutex<serde_json::Value>>,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    runtime_telemetry: Option<Arc<RuntimeTelemetry>>,
    presentation_observation: SharedPresentationObservation,
    repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
) {
    std::thread::spawn(move || {
        initialize_winrt(&log_tx);
        let mut worker = DetectionWorker::new(
            root,
            settings,
            merged_settings,
            log_tx,
            game_found_tx,
            detection_tx,
            runtime_telemetry,
            presentation_observation,
            repaint_callback,
        );
        worker.run();
    });
}

#[cfg(target_os = "windows")]
fn initialize_winrt(log_tx: &Sender<String>) {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        let _ = log_tx.send(format!("[Detection] WinRT init failed: {e}"));
    }
}

#[cfg(not(target_os = "windows"))]
fn initialize_winrt(_log_tx: &Sender<String>) {}

#[derive(Clone, PartialEq)]
struct RepaintFingerprint {
    state: GameSessionState,
    game_rect: Option<WindowRect>,
    window_snapshot: Option<WindowSnapshot>,
    current_song_id: Option<i32>,
    is_song_select: bool,
    scene_detected: bool,
    jacket_status: JacketMatchStatus,
    capture_fatal: Option<String>,
}

impl From<&DetectionOutput> for RepaintFingerprint {
    fn from(output: &DetectionOutput) -> Self {
        Self {
            state: output.state.clone(),
            game_rect: output.game_rect,
            window_snapshot: output.window_snapshot,
            current_song_id: output.current_song_id,
            is_song_select: output.is_song_select,
            scene_detected: output.scene_detected,
            jacket_status: output.jacket_status.clone(),
            capture_fatal: output.capture_fatal.clone(),
        }
    }
}

struct DetectionWorker {
    root: PathBuf,
    telemetry_log_path: PathBuf,
    settings: Settings,
    merged_settings: Arc<Mutex<serde_json::Value>>,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    runtime_telemetry: Option<Arc<RuntimeTelemetry>>,
    start: Instant,
    last_window_log: Instant,
    last_detection_log: Instant,
    was_found: bool,
    is_foreground: bool,
    repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
    last_fingerprint: Option<RepaintFingerprint>,
    last_sleep_hint: SleepHint,
    last_scene_type: overmax_core::SceneType,
    frame_buffer: CapturedFrame,
    window_scheduler: WindowQueryScheduler,
    #[cfg(target_os = "linux")]
    window_snapshot: Option<WindowSnapshot>,
    #[cfg(target_os = "linux")]
    focus_observation: Option<FocusObservation>,
    #[cfg(target_os = "linux")]
    focus_policy: LinuxFocusPolicy,
    #[allow(dead_code)]
    presentation_observation: SharedPresentationObservation,
    #[cfg(target_os = "linux")]
    capture_failure_active: bool,
}

impl DetectionWorker {
    #[allow(clippy::too_many_arguments)]
    fn new(
        root: PathBuf,
        settings: Settings,
        merged_settings: Arc<Mutex<serde_json::Value>>,
        log_tx: Sender<String>,
        game_found_tx: Sender<()>,
        detection_tx: Sender<DetectionOutput>,
        runtime_telemetry: Option<Arc<RuntimeTelemetry>>,
        presentation_observation: SharedPresentationObservation,
        repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        let telemetry_log_path = root.join("cache").join("telemetry.log");
        let prev_log_path = root.join("cache").join("telemetry.prev.log");
        if (cfg!(debug_assertions) || cfg!(feature = "telemetry")) && telemetry_log_path.exists() {
            let _ = std::fs::rename(&telemetry_log_path, &prev_log_path);
        }
        #[cfg(not(any(debug_assertions, feature = "telemetry")))]
        let _ = runtime_telemetry;
        Self {
            root,
            telemetry_log_path,
            settings,
            merged_settings,
            log_tx,
            game_found_tx,
            detection_tx,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            runtime_telemetry,
            start: Instant::now(),
            last_window_log: Instant::now() - LOG_INTERVAL,
            last_detection_log: Instant::now() - LOG_INTERVAL,
            was_found: false,
            is_foreground: false,
            repaint_callback,
            last_fingerprint: None,
            last_sleep_hint: SleepHint::Relaxed,
            last_scene_type: overmax_core::SceneType::Unknown,
            frame_buffer: CapturedFrame {
                width: 0,
                height: 0,
                bgra: Vec::new(),
            },
            window_scheduler: WindowQueryScheduler::new(true),
            #[cfg(target_os = "linux")]
            window_snapshot: None,
            #[cfg(target_os = "linux")]
            focus_observation: None,
            #[cfg(target_os = "linux")]
            focus_policy: LinuxFocusPolicy::new(),
            presentation_observation,
            #[cfg(target_os = "linux")]
            capture_failure_active: false,
        }
    }

    fn run(&mut self) {
        #[cfg(target_os = "windows")]
        let tracker = WindowTracker::new(&window_title(&self.settings));
        #[cfg(target_os = "linux")]
        let mut tracker = WindowTracker::new(&window_title(&self.settings));
        #[allow(unused_mut)]
        let mut capturer_adaptive = match AdaptiveCaptureEngine::new() {
            Ok(c) => c,
            Err(e) => return self.log(format!("[Detection] capture init failed: {e}")),
        };
        #[cfg(target_os = "windows")]
        {
            let pref = self
                .settings
                .screen_capture()
                .engine
                .parse()
                .unwrap_or_default();
            capturer_adaptive.set_preferred_engine(pref);
            let atlas_enabled = self.settings.screen_capture().enable_gpu_atlas;
            capturer_adaptive.set_enable_gpu_atlas(atlas_enabled);
            let white_level = self.settings.screen_capture().hdr_sdr_white_level;
            capturer_adaptive.set_hdr_sdr_white_level(white_level);
        }
        let mut capturer: Box<dyn CaptureEngine> = Box::new(capturer_adaptive);
        let mut pipeline = self.build_pipeline();
        self.log("[Detection] Pure Rust Native Engine initialized".to_string());

        loop {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let Some(telemetry) = &self.runtime_telemetry {
                telemetry.maybe_log();
            }
            self.sync_live_settings(&mut capturer);
            #[cfg(target_os = "windows")]
            self.tick(&tracker, &mut capturer, &mut pipeline);
            #[cfg(target_os = "linux")]
            match self.tick_linux(&tracker, &mut capturer, &mut pipeline) {
                LinuxTickResult::Continue => {}
                LinuxTickResult::Stop => return,
                LinuxTickResult::Reconnect => {
                    std::thread::sleep(Duration::from_secs(1));
                    tracker = WindowTracker::new(&window_title(&self.settings));
                    capturer = match AdaptiveCaptureEngine::new() {
                        Ok(capturer) => Box::new(capturer),
                        Err(error) => {
                            self.log(format!("[Detection] capture reconnect failed: {error}"));
                            continue;
                        }
                    };
                    continue;
                }
            }
            std::thread::sleep(self.sleep_duration());
        }
    }

    fn sync_live_settings(&mut self, _capturer: &mut Box<dyn CaptureEngine>) {
        if let Ok(guard) = self.merged_settings.lock() {
            if let Ok(new_settings) = serde_json::from_value::<Settings>(guard.clone()) {
                #[cfg(target_os = "windows")]
                {
                    let old_pref: crate::capture::capture_engine::PreferredCaptureEngine = self
                        .settings
                        .screen_capture()
                        .engine
                        .parse()
                        .unwrap_or_default();
                    let new_pref: crate::capture::capture_engine::PreferredCaptureEngine =
                        new_settings
                            .screen_capture()
                            .engine
                            .parse()
                            .unwrap_or_default();
                    if old_pref != new_pref {
                        self.log(format!(
                            "[Detection] capture backend updated: {old_pref:?} -> {new_pref:?}"
                        ));
                        _capturer.set_preferred_engine(new_pref);
                    }
                    let old_atlas = self.settings.screen_capture().enable_gpu_atlas;
                    let new_atlas = new_settings.screen_capture().enable_gpu_atlas;
                    if old_atlas != new_atlas {
                        self.log(format!(
                            "[Detection] gpu atlas updated: {old_atlas} -> {new_atlas}"
                        ));
                        _capturer.set_enable_gpu_atlas(new_atlas);
                    }
                    let old_white_level = self.settings.screen_capture().hdr_sdr_white_level;
                    let new_white_level = new_settings.screen_capture().hdr_sdr_white_level;
                    if old_white_level != new_white_level {
                        self.log(format!(
                            "[Detection] hdr sdr white level updated: {old_white_level:?} -> {new_white_level:?}"
                        ));
                        _capturer.set_hdr_sdr_white_level(new_white_level);
                    }
                }
                self.settings = new_settings;
            }
        }
    }

    fn request_repaint(&self) {
        (self.repaint_callback)();
    }

    fn build_pipeline(&self) -> DetectionPipeline {
        let db_path = image_index_path(&self.root, &self.settings);
        self.log(format!(
            "[Detection] image_index path={}",
            db_path.display()
        ));
        let mut db = ImageIndexDb::new(db_path, threshold(&self.settings))
            .with_disable_hog(disable_hog(&self.settings))
            .with_margin_threshold(margin_threshold(&self.settings));
        match db.load() {
            Ok(n) => self.log(format!("[Detection] image_index loaded: {n} images")),
            Err(e) => self.log(format!("[Detection] image_index load failed: {e}")),
        }
        DetectionPipeline::new(db)
    }

    #[cfg(target_os = "windows")]
    fn tick(
        &mut self,
        tracker: &WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
    ) {
        let (rect, foreground) = if self.window_scheduler.should_query() {
            let r = tracker.game_rect();
            let f = tracker.is_foreground();
            self.window_scheduler.update(r, f);
            (r, f)
        } else {
            (
                self.window_scheduler.cached_rect,
                self.window_scheduler.cached_foreground,
            )
        };

        let Some(rect) = rect else {
            self.on_window_missing();
            return;
        };
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
                out.state.is_fullscreen = tracker.is_fullscreen();
                self.log_detection_summary(&out);
                self.check_and_log_scene_transition(&out);

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
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                if let Some(telemetry) = &self.runtime_telemetry {
                    telemetry.record_capture_failure();
                }
                self.log_detection_throttled(format!("[Detection] capture failed: {e}"));
                if capturer.error_action() == CaptureErrorAction::Stop {
                    self.send_detection_output(self.detecting_output(Some(rect), None, Some(e)));
                    self.request_repaint();
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn tick_linux(
        &mut self,
        tracker: &WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
    ) -> LinuxTickResult {
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
                self.capture_failure_active = false;
                let detect_start = Instant::now();
                let mut out =
                    pipeline.detect(&self.frame_buffer, self.start.elapsed().as_secs_f64());
                let detect_elapsed = detect_start.elapsed().as_micros() as u64;
                pipeline.stats.detect.update(detect_elapsed);

                out.telemetry_snapshot = pipeline.stats.maybe_take_snapshot(5.0);
                if let Some(ref snap) = out.telemetry_snapshot {
                    self.log_telemetry_snapshot(snap);
                }
                out.game_rect = Some(snapshot.rect);
                out.window_snapshot = Some(snapshot);
                out.state.is_fullscreen = snapshot.fullscreen;
                self.log_detection_summary(&out);
                self.check_and_log_scene_transition(&out);

                let fingerprint = RepaintFingerprint::from(&out);

                let state_changed = self.last_fingerprint.as_ref() != Some(&fingerprint);
                if state_changed {
                    self.last_fingerprint = Some(fingerprint);
                }

                self.last_sleep_hint = out.sleep_hint;
                self.send_detection_output(out);
                if state_changed || overlay_snapshot_changed {
                    self.request_repaint();
                }
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

    #[cfg(target_os = "linux")]
    fn handle_linux_capture_error(
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

    /// `detecting()` output that closes stale verified state after a capture failure.
    fn detecting_output(
        &self,
        game_rect: Option<WindowRect>,
        window_snapshot: Option<WindowSnapshot>,
        capture_fatal: Option<String>,
    ) -> DetectionOutput {
        DetectionOutput {
            scene_detected: false,
            is_song_select: false,
            is_result: false,
            is_leaving: false,
            confidence: 0.0,
            state: GameSessionState::detecting(),
            event: None,
            current_song_id: None,
            image_db_ready: false,
            jacket_status: JacketMatchStatus::NotSongSelect,
            game_rect,
            window_snapshot,
            capture_fatal,
            top_jacket_similarity: None,
            roi_scale: 1.0,
            roi_offset_y: 0,
            stable_hits: 0,
            sleep_hint: SleepHint::Relaxed,
            telemetry_snapshot: None,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            delivery_telemetry: None,
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_detecting_output(&self, capture_fatal: Option<String>) -> DetectionOutput {
        self.detecting_output(
            self.window_snapshot.map(|s| s.rect),
            self.window_snapshot,
            capture_fatal,
        )
    }

    #[cfg(target_os = "linux")]
    fn on_capture_interrupted(&mut self, pipeline: &mut DetectionPipeline, reason: &str) {
        pipeline.reset();
        if self.capture_failure_active {
            return;
        }
        self.capture_failure_active = true;
        self.send_detection_output(self.linux_detecting_output(None));
        self.request_repaint();
        self.log(format!("[Detection] {reason}; state reset"));
    }

    #[cfg(target_os = "linux")]
    fn on_capture_fatal(&mut self, pipeline: &mut DetectionPipeline, error: String) {
        pipeline.reset();
        self.send_detection_output(self.linux_detecting_output(Some(error.clone())));
        self.request_repaint();
        self.log(format!("[Detection] capture unavailable: {error}"));
    }

    #[cfg(target_os = "linux")]
    fn on_linux_window_missing(&mut self, pipeline: &mut DetectionPipeline) {
        if self.was_found {
            self.on_capture_interrupted(pipeline, "game window lost");
            self.log("[WindowTracker] game window lost".into());
        }
        self.was_found = false;
        self.is_foreground = false;
    }

    fn roll_telemetry_log(&self) {
        if !cfg!(debug_assertions) && !cfg!(feature = "telemetry") {
            return;
        }
        let prev_log_path = self.root.join("cache").join("telemetry.prev.log");
        if self.telemetry_log_path.exists() {
            let _ = std::fs::rename(&self.telemetry_log_path, &prev_log_path);
        }
    }

    fn on_window_found(
        &mut self,
        rect: crate::capture::window_tracker::WindowRect,
        foreground: bool,
    ) -> bool {
        if !self.was_found {
            self.roll_telemetry_log();
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let Some(telemetry) = &self.runtime_telemetry {
                telemetry.start_session();
            }
            let _ = self.game_found_tx.send(());
            if !foreground {
                #[cfg(target_os = "linux")]
                self.send_detection_output(self.linux_detecting_output(None));
                #[cfg(not(target_os = "linux"))]
                self.send_detection_output(self.detecting_output(Some(rect), None, None));
            }
            self.request_repaint();
            self.log("[Detection] game window found".into());
        }
        if foreground != self.is_foreground {
            self.request_repaint();
        }
        self.was_found = true;
        self.is_foreground = foreground;
        if !foreground {
            self.log_window_throttled("[Detection] foreground=false; capture skipped".into());
            return false;
        }
        self.log_window_throttled(format!(
            "[WindowTracker] rect {}x{} @ ({},{}) foreground={foreground}",
            rect.width, rect.height, rect.left, rect.top
        ));
        true
    }

    #[cfg(target_os = "windows")]
    fn on_window_missing(&mut self) {
        if self.was_found {
            self.send_detection_output(self.detecting_output(None, None, None));
            self.request_repaint();
            self.log("[WindowTracker] game window lost".into());
        }
        self.was_found = false;
        self.is_foreground = false;
    }

    fn log_detection_summary(&mut self, out: &DetectionOutput) {
        if !out.is_song_select {
            self.log_detection_throttled(format!(
                "[Detection] song-select=false scene={} confidence={:.2}",
                out.scene_detected, out.confidence
            ));
            return;
        }
        let song = out
            .current_song_id
            .map(|v| v.to_string())
            .unwrap_or_else(|| {
                if out.image_db_ready {
                    "no-match".into()
                } else {
                    "db-not-ready".into()
                }
            });
        self.log_detection_throttled(format!(
            "[Detection] song-select=true confidence={:.2} song_id={song} jacket={} stable={}",
            out.confidence,
            jacket_status_label(&out.jacket_status),
            out.state.is_stable
        ));
    }

    fn sleep_duration(&self) -> Duration {
        let capture_settings = self.settings.screen_capture();
        if self.was_found {
            if self.is_foreground {
                match self.last_sleep_hint {
                    SleepHint::Active => Duration::from_millis(capture_settings.active_sleep_ms),
                    SleepHint::Relaxed => Duration::from_secs(1),
                }
            } else {
                Duration::from_millis(capture_settings.background_sleep_ms)
            }
        } else {
            Duration::from_secs_f64(idle_sleep(&self.settings))
        }
    }

    fn log_telemetry_snapshot(&self, snap: &crate::detector::telemetry::PipelineTelemetrySnapshot) {
        use crate::detector::telemetry::format_duration_us;
        let msg = format!(
            "[Telemetry] 5s snap ({:.1}s): capture={}(max {}) detect={}(max {}) [scene={}(max {}), jacket={}(max {}), play={}(max {})] | active={} unknown={} match_hits={} miss={} maxdiff={:.1} cg={} band={} minsim={:.3}",
            snap.period_sec,
            format_duration_us(snap.capture_avg_us),
            format_duration_us(snap.capture_max_us),
            format_duration_us(snap.detect_avg_us),
            format_duration_us(snap.detect_max_us),
            format_duration_us(snap.scene_avg_us),
            format_duration_us(snap.scene_max_us),
            format_duration_us(snap.jacket_avg_us),
            format_duration_us(snap.jacket_max_us),
            format_duration_us(snap.play_state_avg_us),
            format_duration_us(snap.play_state_max_us),
            snap.active_frames,
            snap.unknown_frames,
            snap.match_jacket_count,
            snap.scene_poll_misses,
            snap.scene_miss_max_thumb_diff,
            snap.scene_miss_centroid_rejects,
            snap.scene_miss_band_rejects,
            snap.scene_miss_min_top_similarity
        );
        println!("{msg}");
        self.log(msg.clone());
        self.append_to_telemetry_log(&msg);
    }

    fn check_and_log_scene_transition(&mut self, out: &DetectionOutput) {
        let current_scene = out.state.scene;
        if self.last_scene_type != current_scene {
            let prev_scene = self.last_scene_type;
            self.last_scene_type = current_scene;
            let details = match &out.state.context {
                Some(ctx) => format!(
                    "SongID: {}, Mode: {:?}, Diff: {:?}, Rate: {:.2}%",
                    ctx.song_id, ctx.mode, ctx.diff, ctx.rate
                ),
                None => match out.current_song_id {
                    Some(id) => format!("SongID: {id}"),
                    None => "No Context".to_string(),
                },
            };
            let msg = format!(
                "[SceneTransition] {:?} -> {:?} ({details})",
                prev_scene, current_scene
            );
            println!("{msg}");
            self.log(msg.clone());
            self.append_to_telemetry_log(&msg);
        }
    }

    fn append_to_telemetry_log(&self, line: &str) {
        if !cfg!(debug_assertions) && !cfg!(feature = "telemetry") {
            return;
        }
        use std::fs::OpenOptions;
        use std::io::Write;
        if let Some(parent) = self.telemetry_log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.telemetry_log_path)
        {
            let ts = crate::detector::telemetry::current_timestamp_str();
            let _ = writeln!(f, "[{ts}] {line}");
        }
    }

    #[allow(unused_mut)]
    fn send_detection_output(&self, mut output: DetectionOutput) {
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let Some(telemetry) = &self.runtime_telemetry {
            output.delivery_telemetry = Some(telemetry.record_output_generated());
        }
        let _ = self.detection_tx.send(output);
    }

    fn log(&self, message: String) {
        let _ = self.log_tx.send(message);
    }

    fn log_window_throttled(&mut self, message: String) {
        if self.last_window_log.elapsed() >= LOG_INTERVAL {
            self.last_window_log = Instant::now();
            self.log(message);
        }
    }

    fn log_detection_throttled(&mut self, message: String) {
        if self.last_detection_log.elapsed() >= LOG_INTERVAL {
            self.last_detection_log = Instant::now();
            self.log(message);
        }
    }
}

fn jacket_status_label(status: &JacketMatchStatus) -> String {
    match status {
        JacketMatchStatus::NotSongSelect => "not-song-select".into(),
        JacketMatchStatus::Leaving => "leaving".into(),
        JacketMatchStatus::DbNotReady => "db-not-ready".into(),
        JacketMatchStatus::Cooldown => "cooldown".into(),
        JacketMatchStatus::CropMissing => "crop-missing".into(),
        JacketMatchStatus::ThumbnailMissing => "thumbnail-missing".into(),
        JacketMatchStatus::Unchanged => "unchanged".into(),
        JacketMatchStatus::NoMatch => "no-match".into(),
        JacketMatchStatus::InvalidId {
            image_id,
            similarity,
        } => format!("invalid-id:{image_id}@{similarity:.4}"),
        JacketMatchStatus::Matched {
            song_id,
            similarity,
        } => format!("matched:{song_id}@{similarity:.4}"),
    }
}

fn window_title(settings: &Settings) -> String {
    settings
        .window_tracker
        .as_ref()
        .map(|t| t.window_title.clone())
        .unwrap_or_else(|| "DJMAX RESPECT V".to_string())
}

fn image_index_path(root: &Path, settings: &Settings) -> PathBuf {
    let fallback = DataCompatibility::current().image_index_db;
    let rel = settings
        .jacket_matcher
        .as_ref()
        .map(|j| j.db_path.as_str())
        .unwrap_or(fallback);
    root.join(rel)
}

fn threshold(settings: &Settings) -> f32 {
    settings
        .jacket_matcher
        .as_ref()
        .map(|j| j.similarity_threshold)
        .unwrap_or(0.6) as f32
}

fn idle_sleep(settings: &Settings) -> f64 {
    settings
        .screen_capture
        .as_ref()
        .map(|s| s.idle_sleep_sec)
        .unwrap_or(1.0)
        .max(0.5)
}

fn disable_hog(settings: &Settings) -> bool {
    settings
        .jacket_matcher
        .as_ref()
        .map(|j| j.disable_hog)
        .unwrap_or(true)
}

fn margin_threshold(settings: &Settings) -> f32 {
    settings
        .jacket_matcher
        .as_ref()
        .map(|j| j.margin_threshold)
        .unwrap_or(3.0) as f32
}

struct WindowQueryScheduler {
    last_query_ts: Instant,
    cached_rect: Option<crate::capture::window_tracker::WindowRect>,
    cached_foreground: bool,
    is_window_moving: bool,
    enabled: bool,
}

impl WindowQueryScheduler {
    fn new(enabled: bool) -> Self {
        Self {
            last_query_ts: Instant::now()
                .checked_sub(Duration::from_secs(5))
                .unwrap_or_else(Instant::now),
            cached_rect: None,
            cached_foreground: false,
            is_window_moving: false,
            enabled,
        }
    }

    fn get_query_interval(&self) -> Duration {
        if !self.enabled {
            return Duration::from_millis(0);
        }
        if self.is_window_moving {
            Duration::from_millis(16) // 드래그 시 고속 폴링 (60FPS)
        } else if self.cached_rect.is_some() {
            Duration::from_millis(300) // 멈춤 시 이완 (300ms)
        } else {
            Duration::from_millis(1000) // 창 미발견 시 1초 대기
        }
    }

    fn should_query(&self) -> bool {
        self.last_query_ts.elapsed() >= self.get_query_interval()
    }

    fn update(
        &mut self,
        rect: Option<crate::capture::window_tracker::WindowRect>,
        foreground: bool,
    ) {
        if !self.enabled {
            self.cached_rect = rect;
            self.cached_foreground = foreground;
            return;
        }

        self.last_query_ts = Instant::now();

        if let (Some(prev), Some(curr)) = (self.cached_rect, rect) {
            self.is_window_moving = prev.left != curr.left || prev.top != curr.top;
        } else {
            self.is_window_moving = false;
        }

        self.cached_rect = rect;
        self.cached_foreground = foreground;
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::{
        capture_target_resized, effective_focus, DetectionPipeline, LinuxFocusLoss,
        LinuxFocusPolicy, LinuxTickResult,
    };
    use super::{DetectionWorker, JacketMatchStatus, RepaintFingerprint};
    #[cfg(target_os = "linux")]
    use crate::capture::capture_engine::{CaptureEngine, CaptureErrorAction};
    #[cfg(target_os = "linux")]
    use crate::capture::frame::CapturedFrame;
    #[cfg(target_os = "linux")]
    use crate::capture::window_tracker::{
        FocusObservation, FocusSource, FocusState, PresentationObservation,
    };
    use crate::capture::window_tracker::{WindowRect, WindowSnapshot};
    use overmax_core::{Difficulty, GameSessionState, Mode, PlayContext, SceneType};
    #[cfg(target_os = "linux")]
    use overmax_data::ImageIndexDb;
    use overmax_data::Settings;
    use std::sync::mpsc;

    #[cfg(target_os = "linux")]
    struct FailedCapture(CaptureErrorAction);

    fn snapshot(rect: WindowRect) -> WindowSnapshot {
        WindowSnapshot {
            window: 7,
            rect,
            foreground: true,
            fullscreen: false,
        }
    }

    #[cfg(target_os = "linux")]
    fn focus(state: FocusState, generation: u64) -> FocusObservation {
        FocusObservation {
            state,
            source: FocusSource::EwmhActiveWindow,
            generation,
        }
    }

    #[cfg(target_os = "linux")]
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
    fn repaint_fingerprint_tracks_session_context_without_heartbeats() {
        let base = RepaintFingerprint {
            state: GameSessionState {
                scene: SceneType::Freestyle,
                context: Some(PlayContext {
                    song_id: 1,
                    mode: Mode::B4,
                    diff: Difficulty::NM,
                    rate: 99.0,
                    is_max_combo: false,
                }),
                is_stable: false,
                is_fullscreen: false,
            },
            game_rect: None,
            window_snapshot: None,
            current_song_id: Some(1),
            is_song_select: true,
            scene_detected: true,
            jacket_status: JacketMatchStatus::NotSongSelect,
            capture_fatal: None,
        };
        assert!(base == base.clone());

        let mut variants = std::array::from_fn::<_, 6, _>(|_| base.state.clone());
        variants[0].scene = SceneType::Online;
        variants[1].is_stable = true;
        variants[2].context.as_mut().unwrap().mode = Mode::B5;
        variants[3].context.as_mut().unwrap().diff = Difficulty::HD;
        variants[4].context.as_mut().unwrap().rate = 99.5;
        variants[5].context.as_mut().unwrap().is_max_combo = true;

        assert!(variants.into_iter().all(|state| RepaintFingerprint {
            state,
            ..base.clone()
        } != base));

        let mut foreground = base;
        foreground.window_snapshot = Some(snapshot(WindowRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        }));
        let mut background = foreground.clone();
        background.window_snapshot.as_mut().unwrap().foreground = false;
        assert!(foreground != background);
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn cold_start_non_foreground_is_fail_closed() {
        let now = std::time::Instant::now();
        for state in [FocusState::Background, FocusState::Unknown] {
            let mut policy = LinuxFocusPolicy::new();
            assert_eq!(policy.update(focus(state, 1), now, now), (false, None));
        }
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_focus_overrides_x11_fallback() {
        let x11 = focus(FocusState::Background, 4);
        let presentation = PresentationObservation {
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[test]
    fn detecting_output_resets_session_context_and_propagates_fatal() {
        let (log_tx, _log_rx) = mpsc::channel();
        let (game_tx, _game_rx) = mpsc::channel();
        let (detection_tx, _detection_rx) = mpsc::channel();
        let worker = DetectionWorker::new(
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
        let test_rect = WindowRect {
            left: 100,
            top: 200,
            width: 1920,
            height: 1080,
        };
        let out =
            worker.detecting_output(Some(test_rect), None, Some("device removed".to_string()));

        assert!(!out.scene_detected);
        assert!(!out.is_song_select);
        assert!(out.state.context.is_none());
        assert_eq!(out.game_rect, Some(test_rect));
        assert_eq!(out.capture_fatal.as_deref(), Some("device removed"));
    }

    #[cfg(target_os = "windows")]
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
