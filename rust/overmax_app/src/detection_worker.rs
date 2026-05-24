//! Runtime detection worker: window tracking -> capture -> pipeline -> UI state.

use crate::detection_pipeline::{DetectionOutput, DetectionPipeline, JacketMatchStatus};
use crate::screen_capture::ScreenCapturer;
use crate::window_tracker::WindowTracker;
use overmax_core::GameSessionState;
use overmax_data::{DataCompatibility, ImageIndexDb};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const ACTIVE_SLEEP: Duration = Duration::from_millis(120);
const BACKGROUND_SLEEP: Duration = Duration::from_millis(500);
const LOG_INTERVAL: Duration = Duration::from_secs(3);

pub fn spawn(
    root: PathBuf,
    settings: Value,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    ctx_holder: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
) {
    std::thread::spawn(move || {
        initialize_winrt(&log_tx);
        let mut worker = DetectionWorker::new(root, settings, log_tx, game_found_tx, detection_tx, ctx_holder);
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
    settings: Value,
    log_tx: Sender<String>,
    game_found_tx: Sender<()>,
    detection_tx: Sender<DetectionOutput>,
    start: Instant,
    last_window_log: Instant,
    last_detection_log: Instant,
    was_found: bool,
    is_foreground: bool,
    ctx_holder: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
    last_song_id: Option<u32>,
    last_is_song_select: bool,
    last_logo_detected: bool,
    last_jacket_status: JacketMatchStatus,
}

impl DetectionWorker {
    fn new(
        root: PathBuf,
        settings: Value,
        log_tx: Sender<String>,
        game_found_tx: Sender<()>,
        detection_tx: Sender<DetectionOutput>,
        ctx_holder: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
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
            ctx_holder,
            last_song_id: None,
            last_is_song_select: false,
            last_logo_detected: false,
            last_jacket_status: JacketMatchStatus::NotSongSelect,
        }
    }

    fn run(&mut self) {
        let tracker = WindowTracker::new(&window_title(&self.settings));
        let capturer = match ScreenCapturer::new() {
            Ok(c) => c,
            Err(e) => return self.log(format!("[Detection] capture init failed: {e}")),
        };
        let mut pipeline = self.build_pipeline();
        self.log(format!(
            "[Detection] OCR available={}",
            pipeline.ocr_available()
        ));

        loop {
            self.tick(&tracker, &capturer, &mut pipeline);
            std::thread::sleep(self.sleep_duration());
        }
    }

    fn request_repaint(&self) {
        if let Ok(holder) = self.ctx_holder.lock() {
            if let Some(ctx) = &*holder {
                ctx.request_repaint();
            }
        }
    }

    fn build_pipeline(&self) -> DetectionPipeline {
        let db_path = image_index_path(&self.root, &self.settings);
        self.log(format!(
            "[Detection] image_index path={}",
            db_path.display()
        ));
        let mut db = ImageIndexDb::new(db_path, threshold(&self.settings));
        match db.load() {
            Ok(n) => self.log(format!("[Detection] image_index loaded: {n} images")),
            Err(e) => self.log(format!("[Detection] image_index load failed: {e}")),
        }
        DetectionPipeline::new(db)
    }

    fn tick(
        &mut self,
        tracker: &WindowTracker,
        capturer: &ScreenCapturer,
        pipeline: &mut DetectionPipeline,
    ) {
        let Some(rect) = tracker.game_rect() else {
            self.on_window_missing();
            return;
        };
        if !self.on_window_found(rect, tracker.is_foreground()) {
            return;
        }
        match capturer.capture_bgra(rect) {
            Ok(frame) => {
                let mut out = pipeline.detect(&frame, self.start.elapsed().as_secs_f64());
                out.game_rect = Some(rect);
                self.log_detection_summary(&out);
                
                let state_changed = out.current_song_id != self.last_song_id
                    || out.is_song_select != self.last_is_song_select
                    || out.logo_detected != self.last_logo_detected
                    || std::mem::discriminant(&out.jacket_status) != std::mem::discriminant(&self.last_jacket_status);

                self.last_song_id = out.current_song_id;
                self.last_is_song_select = out.is_song_select;
                self.last_logo_detected = out.logo_detected;
                self.last_jacket_status = out.jacket_status.clone();

                let _ = self.detection_tx.send(out);
                if state_changed {
                    self.request_repaint();
                }
            }
            Err(e) => self.log_detection_throttled(format!("[Detection] capture failed: {e}")),
        }
    }

    fn on_window_found(
        &mut self,
        rect: crate::window_tracker::WindowRect,
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

    fn on_window_missing(&mut self) {
        if self.was_found {
            let _ = self.detection_tx.send(DetectionOutput {
                logo_detected: false,
                is_song_select: false,
                is_leaving: false,
                confidence: 0.0,
                state: GameSessionState::detecting(),
                current_song_id: None,
                image_db_ready: false,
                jacket_status: JacketMatchStatus::NotSongSelect,
                game_rect: None,
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
        if self.was_found {
            if self.is_foreground {
                ACTIVE_SLEEP
            } else {
                BACKGROUND_SLEEP
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

fn window_title(settings: &Value) -> String {
    settings
        .get("window_tracker")
        .and_then(|v| v.get("window_title"))
        .and_then(Value::as_str)
        .unwrap_or("DJMAX RESPECT V")
        .to_string()
}

fn image_index_path(root: &Path, settings: &Value) -> PathBuf {
    let fallback = DataCompatibility::current().image_index_db;
    let rel = settings
        .get("jacket_matcher")
        .and_then(|v| v.get("db_path"))
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    root.join(rel)
}

fn threshold(settings: &Value) -> f32 {
    settings
        .get("jacket_matcher")
        .and_then(|v| v.get("similarity_threshold"))
        .and_then(Value::as_f64)
        .unwrap_or(0.6) as f32
}

fn idle_sleep(settings: &Value) -> f64 {
    settings
        .get("screen_capture")
        .and_then(|v| v.get("idle_sleep_sec"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .max(0.5)
}
