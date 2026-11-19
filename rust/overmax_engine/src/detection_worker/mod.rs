//! Runtime detection worker: window tracking -> capture -> pipeline -> UI state.

use crate::DetectionPipeline;
use overmax_capture::{AdaptiveCaptureEngine, CaptureEngine, CaptureErrorAction};
#[cfg(target_os = "linux")]
use overmax_capture::{FocusObservation, FocusSource, FocusState, PresentationObservation};
use overmax_capture::{SharedPresentationObservation, WindowTracker};
use overmax_core::GameSessionState;
use overmax_data::{DataCompatibility, ImageIndexDb, Settings};
use overmax_detect::RuntimeTelemetry;
use overmax_detect::{DetectionOutput, JacketMatchStatus, SleepHint};
use overmax_frame::CapturedFrame;
use overmax_frame::{WindowRect, WindowSnapshot};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LOG_INTERVAL: Duration = Duration::from_secs(3);

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux::{initialize_winrt, PlatformState};
#[cfg(all(target_os = "linux", feature = "portal-capture"))]
use overmax_capture::{PortalCapture, PortalConnection, PortalControl, ResetScope};
#[cfg(target_os = "windows")]
use windows::{initialize_winrt, PlatformState};

/// What the run loop does after one platform tick. The Windows tick always
/// returns `Sleep`; `Skip`/`Stop` come from the Linux reconnect/fatal paths.
#[cfg_attr(target_os = "windows", allow(dead_code))]
enum TickFlow {
    /// Sleep for the scheduled interval, then tick again.
    Sleep,
    /// Tick again immediately (the Linux reconnect path already waited).
    Skip,
    /// Capture is permanently unavailable; end the worker.
    Stop,
}

pub struct WorkerControl {
    #[cfg(all(target_os = "linux", feature = "portal-capture"))]
    pub portal: PortalControl,
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
    image_index_reload_rx: Receiver<()>,
) -> WorkerControl {
    #[cfg(all(target_os = "linux", feature = "portal-capture"))]
    let (portal_control, portal) = PortalConnection::new();
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
        )
        .with_image_index_reload(image_index_reload_rx);
        #[cfg(all(target_os = "linux", feature = "portal-capture"))]
        {
            worker.platform.portal = Some(portal);
        }
        worker.run();
    });
    WorkerControl {
        #[cfg(all(target_os = "linux", feature = "portal-capture"))]
        portal: portal_control,
    }
}

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
    platform: PlatformState,
    #[allow(dead_code)]
    presentation_observation: SharedPresentationObservation,
    capture_failure_active: bool,
    /// `image_index.db`가 백그라운드에서 갱신되었을 때 앱이 보내는 재로드 신호
    image_index_reload_rx: Option<Receiver<()>>,
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
            platform: PlatformState::new(),
            presentation_observation,
            capture_failure_active: false,
            image_index_reload_rx: None,
        }
    }

    fn with_image_index_reload(mut self, rx: Receiver<()>) -> Self {
        self.image_index_reload_rx = Some(rx);
        self
    }

    fn run(&mut self) {
        let mut tracker = WindowTracker::new(&window_title(&self.settings));
        let mut capturer_adaptive = match AdaptiveCaptureEngine::new() {
            Ok(c) => c,
            Err(e) => return self.log(format!("[Detection] capture init failed: {e}")),
        };
        self.configure_capture(&mut capturer_adaptive);
        let mut capturer: Box<dyn CaptureEngine> = Box::new(capturer_adaptive);
        let mut pipeline = self.build_pipeline();
        self.log("[Detection] Pure Rust Native Engine initialized".to_string());

        loop {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let Some(telemetry) = &self.runtime_telemetry {
                telemetry.maybe_log();
            }
            self.sync_live_settings(&mut capturer);
            self.reload_image_index_if_requested(&mut pipeline);
            match self.tick_platform(&mut tracker, &mut capturer, &mut pipeline) {
                TickFlow::Sleep => {}
                TickFlow::Skip => continue,
                TickFlow::Stop => return,
            }
            std::thread::sleep(self.sleep_duration());
        }
    }

    fn sync_live_settings(&mut self, capturer: &mut Box<dyn CaptureEngine>) {
        if let Ok(guard) = self.merged_settings.lock() {
            if let Ok(new_settings) = serde_json::from_value::<Settings>(guard.clone()) {
                self.sync_capture_settings(&new_settings, capturer);
                self.settings = new_settings;
            }
        }
    }

    fn request_repaint(&self) {
        (self.repaint_callback)();
    }

    fn reload_image_index_if_requested(&self, pipeline: &mut DetectionPipeline) {
        let Some(rx) = &self.image_index_reload_rx else {
            return;
        };
        let mut requested = false;
        while rx.try_recv().is_ok() {
            requested = true;
        }
        if requested {
            self.log("[Detection] image_index.db 갱신 감지: 인덱스 재로드".to_string());
            pipeline.replace_jacket_index(self.load_image_db().into());
        }
    }

    fn build_pipeline(&self) -> DetectionPipeline {
        DetectionPipeline::new(self.load_image_db())
    }

    fn load_image_db(&self) -> ImageIndexDb {
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
        db
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

    fn roll_telemetry_log(&self) {
        if !cfg!(debug_assertions) && !cfg!(feature = "telemetry") {
            return;
        }
        let prev_log_path = self.root.join("cache").join("telemetry.prev.log");
        if self.telemetry_log_path.exists() {
            let _ = std::fs::rename(&self.telemetry_log_path, &prev_log_path);
        }
    }

    fn on_window_found(&mut self, rect: overmax_frame::WindowRect, foreground: bool) -> bool {
        if !self.was_found {
            self.roll_telemetry_log();
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let Some(telemetry) = &self.runtime_telemetry {
                telemetry.start_session();
            }
            let _ = self.game_found_tx.send(());
            if !foreground {
                self.send_detection_output(self.initial_detecting_output(rect));
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

    fn log_telemetry_snapshot(&self, snap: &overmax_detect::PipelineTelemetrySnapshot) {
        use overmax_detect::format_duration_us;
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

    /// 확정 이벤트(`VerifiedPlayEvent`)를 telemetry 로그에 남긴다.
    /// 선곡 화면의 미플레이(0.00%) 이벤트는 로컬 기록 삭제로 이어지므로 오탐 추적용으로 구분해 기록한다.
    fn log_verified_event(&mut self, out: &DetectionOutput) {
        let Some(event) = &out.event else {
            return;
        };
        let msg = format!(
            "[VerifiedEvent] {} SongID: {}, Mode: {:?}, Diff: {:?}, Rate: {:.2}%, MaxCombo: {}",
            if event.is_result_screen {
                "result"
            } else {
                "select"
            },
            event.song_id,
            event.mode,
            event.diff,
            event.rate,
            event.is_max_combo
        );
        println!("{msg}");
        self.log(msg.clone());
        self.append_to_telemetry_log(&msg);
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
            let ts = overmax_detect::current_timestamp_str();
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
    cached_rect: Option<overmax_frame::WindowRect>,
    cached_foreground: bool,
    /// rect/foreground 와 같은 조회 주기로 갱신한다. `is_fullscreen` 은 Win32 호출을
    /// 최대 5회 수행하므로 매 프레임 호출하지 않는다(Linux 는 `WindowSnapshot` 사용).
    #[cfg(target_os = "windows")]
    cached_fullscreen: bool,
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
            #[cfg(target_os = "windows")]
            cached_fullscreen: false,
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

    fn update(&mut self, rect: Option<overmax_frame::WindowRect>, foreground: bool) {
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
    use super::{DetectionWorker, JacketMatchStatus, RepaintFingerprint};
    use overmax_core::{Difficulty, GameSessionState, Mode, PlayContext, SceneType};
    use overmax_data::Settings;
    use overmax_frame::{WindowRect, WindowSnapshot};
    use std::sync::mpsc;

    pub(super) fn snapshot(rect: WindowRect) -> WindowSnapshot {
        WindowSnapshot {
            window: 7,
            rect,
            foreground: true,
            fullscreen: false,
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
}
