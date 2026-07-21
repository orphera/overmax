//! Runtime detection worker: window tracking -> capture -> pipeline -> UI state.

use crate::capture::capture_engine::{AdaptiveCaptureEngine, CaptureEngine};
use crate::capture::frame::CapturedFrame;
#[cfg(target_os = "linux")]
use crate::capture::window_tracker::WindowSnapshot;
use crate::capture::window_tracker::WindowTracker;
use crate::detector::detection_pipeline::{DetectionOutput, DetectionPipeline, JacketMatchStatus};
use overmax_core::{Changed, GameSessionState};
use overmax_data::{DataCompatibility, ImageIndexDb, Settings};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const LOG_INTERVAL: Duration = Duration::from_secs(3);

pub fn spawn(
    root: PathBuf,
    settings: Settings,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
) {
    std::thread::spawn(move || {
        initialize_winrt(&log_tx);
        let mut worker = DetectionWorker::new(
            root,
            settings,
            log_tx,
            game_found_tx,
            detection_tx,
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

struct DetectionWorker {
    root: PathBuf,
    settings: Settings,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    start: Instant,
    last_window_log: Instant,
    last_detection_log: Instant,
    was_found: bool,
    is_foreground: bool,
    repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
    last_song_id: Changed<Option<i32>>,
    last_is_song_select: Changed<bool>,
    last_logo_detected: Changed<bool>,
    last_jacket_status: Changed<JacketMatchStatus>,
    last_is_fullscreen: Changed<bool>,
    frame_buffer: CapturedFrame,
    window_scheduler: WindowQueryScheduler,
    #[cfg(target_os = "linux")]
    window_snapshot: Option<WindowSnapshot>,
    #[cfg(target_os = "linux")]
    capture_failure_active: bool,
}

impl DetectionWorker {
    fn new(
        root: PathBuf,
        settings: Settings,
        log_tx: Sender<String>,
        game_found_tx: Sender<()>,
        detection_tx: Sender<DetectionOutput>,
        repaint_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            root,
            settings,
            log_tx,
            game_found_tx,
            detection_tx,
            start: Instant::now(),
            last_window_log: Instant::now() - LOG_INTERVAL,
            last_detection_log: Instant::now() - LOG_INTERVAL,
            was_found: false,
            is_foreground: false,
            repaint_callback,
            last_song_id: Changed::new(None),
            last_is_song_select: Changed::new(false),
            last_logo_detected: Changed::new(false),
            last_jacket_status: Changed::new(JacketMatchStatus::NotSongSelect),
            last_is_fullscreen: Changed::new(false),
            frame_buffer: CapturedFrame {
                width: 0,
                height: 0,
                bgra: Vec::new(),
            },
            window_scheduler: WindowQueryScheduler::new(true),
            #[cfg(target_os = "linux")]
            window_snapshot: None,
            #[cfg(target_os = "linux")]
            capture_failure_active: false,
        }
    }

    fn run(&mut self) {
        let tracker = WindowTracker::new(&window_title(&self.settings));
        let mut capturer: Box<dyn CaptureEngine> = match AdaptiveCaptureEngine::new() {
            Ok(c) => Box::new(c),
            Err(e) => return self.log(format!("[Detection] capture init failed: {e}")),
        };
        let mut pipeline = self.build_pipeline();
        self.log(format!(
            "[Detection] OCR available={}",
            pipeline.ocr_available()
        ));

        loop {
            #[cfg(target_os = "windows")]
            self.tick(&tracker, &mut capturer, &mut pipeline);
            #[cfg(target_os = "linux")]
            if !self.tick_linux(&tracker, &mut capturer, &mut pipeline) {
                return;
            }
            std::thread::sleep(self.sleep_duration());
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
        match capturer.capture_bgra_inplace(rect, &mut self.frame_buffer) {
            Ok(_) => {
                let mut out =
                    pipeline.detect(&self.frame_buffer, self.start.elapsed().as_secs_f64());
                out.game_rect = Some(rect);
                out.state.is_fullscreen = tracker.is_fullscreen();
                self.log_detection_summary(&out);

                // IMPORTANT: `.update()` has side effects (mutates cached state).
                // All five calls must execute before combining — do NOT inline into `||` or allow short-circuit.
                let jacket_changed = self.last_jacket_status.update(out.jacket_status.clone());
                let song_changed = self.last_song_id.update(out.current_song_id);
                let song_select_changed = self.last_is_song_select.update(out.is_song_select);
                let logo_changed = self.last_logo_detected.update(out.logo_detected);
                let fullscreen_changed = self.last_is_fullscreen.update(out.state.is_fullscreen);
                let state_changed = jacket_changed
                    | song_changed
                    | song_select_changed
                    | logo_changed
                    | fullscreen_changed;

                let _ = self.detection_tx.send(out);
                if state_changed {
                    self.request_repaint();
                }
            }
            Err(e) => self.log_detection_throttled(format!("[Detection] capture failed: {e}")),
        }
    }

    #[cfg(target_os = "linux")]
    fn tick_linux(
        &mut self,
        tracker: &WindowTracker,
        capturer: &mut Box<dyn CaptureEngine>,
        pipeline: &mut DetectionPipeline,
    ) -> bool {
        let mut overlay_snapshot_changed = false;
        if self.window_scheduler.should_query() {
            let previous_snapshot = self.window_snapshot;
            let snapshot = match tracker.game_snapshot() {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    self.on_capture_fatal(pipeline, format!("window tracking failed: {error}"));
                    return false;
                }
            };
            let target_changed =
                self.window_snapshot.map(|s| s.window) != snapshot.map(|s| s.window);
            let capture_state_changed = previous_snapshot
                .map(|current| (current.window, current.foreground, current.fullscreen))
                != snapshot.map(|current| (current.window, current.foreground, current.fullscreen));
            overlay_snapshot_changed = previous_snapshot != snapshot;
            if let Err(error) = capturer.set_target(snapshot) {
                self.on_capture_fatal(pipeline, error);
                return false;
            }
            self.window_scheduler.update(
                snapshot.map(|s| s.rect),
                snapshot.is_some_and(|s| s.foreground),
            );
            self.window_snapshot = snapshot;

            if capture_state_changed {
                self.capture_failure_active = false;
            }
            if target_changed && snapshot.is_some() && self.was_found {
                self.on_capture_interrupted(pipeline, "capture target changed");
            } else if snapshot.is_some_and(|current| {
                !current.foreground
                    && previous_snapshot.is_none_or(|previous| {
                        previous.window != current.window || previous.foreground
                    })
            }) {
                self.on_capture_interrupted(pipeline, "game window is in the background");
            }
        }

        let Some(snapshot) = self.window_snapshot else {
            self.on_linux_window_missing(pipeline);
            return true;
        };
        if !self.on_window_found(snapshot.rect, snapshot.foreground) {
            return true;
        }
        if !snapshot.fullscreen {
            self.on_capture_interrupted(pipeline, "borderless fullscreen is required");
            return true;
        }

        match capturer.capture_bgra_inplace(snapshot.rect, &mut self.frame_buffer) {
            Ok(()) => {
                self.capture_failure_active = false;
                let mut out =
                    pipeline.detect(&self.frame_buffer, self.start.elapsed().as_secs_f64());
                out.game_rect = Some(snapshot.rect);
                out.window_snapshot = Some(snapshot);
                out.state.is_fullscreen = snapshot.fullscreen;
                self.log_detection_summary(&out);

                // IMPORTANT: `.update()` has side effects (mutates cached state).
                let jacket_changed = self.last_jacket_status.update(out.jacket_status.clone());
                let song_changed = self.last_song_id.update(out.current_song_id);
                let song_select_changed = self.last_is_song_select.update(out.is_song_select);
                let logo_changed = self.last_logo_detected.update(out.logo_detected);
                let fullscreen_changed = self.last_is_fullscreen.update(out.state.is_fullscreen);
                let state_changed = jacket_changed
                    | song_changed
                    | song_select_changed
                    | logo_changed
                    | fullscreen_changed;

                let _ = self.detection_tx.send(out);
                if state_changed || overlay_snapshot_changed {
                    self.request_repaint();
                }
            }
            Err(error) => {
                self.on_capture_interrupted(pipeline, "capture failed");
                self.log_detection_throttled(format!("[Detection] capture failed: {error}"));
            }
        }
        true
    }

    /// `detecting()` output that closes stale verified state after a capture failure.
    #[cfg(target_os = "linux")]
    fn linux_detecting_output(&self, capture_fatal: Option<String>) -> DetectionOutput {
        DetectionOutput {
            logo_detected: false,
            is_song_select: false,
            is_result: false,
            is_leaving: false,
            confidence: 0.0,
            state: GameSessionState::detecting(),
            current_song_id: None,
            image_db_ready: false,
            jacket_status: JacketMatchStatus::NotSongSelect,
            game_rect: self.window_snapshot.map(|s| s.rect),
            window_snapshot: self.window_snapshot,
            capture_fatal,
            ocr_telemetry: None,
        }
    }

    #[cfg(target_os = "linux")]
    fn on_capture_interrupted(&mut self, pipeline: &mut DetectionPipeline, reason: &str) {
        // ponytail: immediate reset can repeatedly repay stabilization after noisy transient
        // failures; add a time debounce only if real sessions show that cost.
        pipeline.reset();
        if self.capture_failure_active {
            return;
        }
        self.capture_failure_active = true;
        let _ = self.detection_tx.send(self.linux_detecting_output(None));
        self.request_repaint();
        self.log(format!("[Detection] {reason}; state reset"));
    }

    #[cfg(target_os = "linux")]
    fn on_capture_fatal(&mut self, pipeline: &mut DetectionPipeline, error: String) {
        pipeline.reset();
        let _ = self
            .detection_tx
            .send(self.linux_detecting_output(Some(error.clone())));
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

    fn on_window_found(
        &mut self,
        rect: crate::capture::window_tracker::WindowRect,
        foreground: bool,
    ) -> bool {
        if !self.was_found {
            let _ = self.game_found_tx.send(());
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
            let _ = self.detection_tx.send(DetectionOutput {
                logo_detected: false,
                is_song_select: false,
                is_result: false,
                is_leaving: false,
                confidence: 0.0,
                state: GameSessionState::detecting(),
                current_song_id: None,
                image_db_ready: false,
                jacket_status: JacketMatchStatus::NotSongSelect,
                game_rect: None,
                window_snapshot: None,
                capture_fatal: None,
                ocr_telemetry: None,
            });
            self.request_repaint();
            self.log("[WindowTracker] game window lost".into());
        }
        self.was_found = false;
        self.is_foreground = false;
    }

    fn log_detection_summary(&mut self, out: &DetectionOutput) {
        if !out.is_song_select {
            self.log_detection_throttled(format!(
                "[Detection] song-select=false logo={} confidence={:.2}",
                out.logo_detected, out.confidence
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
                if *self.last_is_song_select || *self.last_logo_detected {
                    Duration::from_millis(capture_settings.active_sleep_ms)
                } else {
                    Duration::from_millis(1000)
                }
            } else {
                Duration::from_millis(capture_settings.background_sleep_ms)
            }
        } else {
            Duration::from_secs_f64(idle_sleep(&self.settings))
        }
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{DetectionPipeline, DetectionWorker};
    use overmax_data::{ImageIndexDb, Settings};
    use std::sync::mpsc;

    #[test]
    fn sends_detecting_once_per_capture_failure_streak() {
        let (log_tx, _log_rx) = mpsc::channel();
        let (game_tx, _game_rx) = mpsc::channel();
        let (detection_tx, detection_rx) = mpsc::channel();
        let mut worker = DetectionWorker::new(
            std::path::PathBuf::new(),
            Settings::default(),
            log_tx,
            game_tx,
            detection_tx,
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
}
