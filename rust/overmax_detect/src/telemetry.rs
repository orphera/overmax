use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

const RUNTIME_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionDeliveryTelemetry {
    pub generation: u64,
    pub generated_at: Instant,
    pub drained_at: Option<Instant>,
    pub published_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq)]
struct RuntimeEnvironment {
    app_version: String,
    build_profile: &'static str,
    compositor: String,
    compositor_version: String,
    session_type: String,
    display: String,
    wayland_display: String,
    foreign_toplevel_version: Option<u32>,
    output: Option<OutputEnvironment>,
}

#[derive(Clone, Debug, PartialEq)]
struct OutputEnvironment {
    name: String,
    logical_size: (i32, i32),
    physical_size: Option<(i32, i32)>,
    scale: f64,
}

#[derive(Clone, Copy, Debug)]
struct WindowObservation {
    target_xid: u64,
    active_xid: Option<u64>,
    active_in_client_list: Option<bool>,
    rect: (i32, i32, i32, i32),
    foreground: bool,
    fullscreen: bool,
}

#[derive(Debug)]
struct RuntimeTelemetryState {
    environment: RuntimeEnvironment,
    session_started: bool,
    interval_started: Instant,
    capture_attempts: u32,
    capture_successes: u32,
    capture_failures: u32,
    capture_samples_us: Vec<u64>,
    last_capture_success: Option<Instant>,
    outputs_generated: u32,
    output_generation: u64,
    outputs_drained: u32,
    last_drained_generation: u64,
    output_to_drain: TimingAggregator,
    publish_calls: u32,
    publish_accepted: u32,
    last_published_generation: u64,
    drain_to_publish: TimingAggregator,
    snapshots_applied: u32,
    last_applied_generation: u64,
    publish_to_apply: TimingAggregator,
    last_applied: Option<(u64, Instant)>,
    snapshots_presented: u32,
    last_presented_generation: u64,
    apply_to_present: TimingAggregator,
    window: Option<WindowObservation>,
}

impl RuntimeTelemetryState {
    fn reset_interval(&mut self, now: Instant) {
        self.interval_started = now;
        self.capture_attempts = 0;
        self.capture_successes = 0;
        self.capture_failures = 0;
        self.capture_samples_us.clear();
        self.outputs_generated = 0;
        self.outputs_drained = 0;
        self.output_to_drain.reset();
        self.publish_calls = 0;
        self.publish_accepted = 0;
        self.drain_to_publish.reset();
        self.snapshots_applied = 0;
        self.publish_to_apply.reset();
        self.snapshots_presented = 0;
        self.apply_to_present.reset();
    }

    fn start_session(&mut self, now: Instant) {
        self.reset_interval(now);
        self.session_started = true;
        self.last_capture_success = None;
    }
}

/// Shared, local-only diagnostics for the capture-to-overlay delivery path.
/// Call sites are compiled out in non-telemetry release builds.
#[derive(Debug)]
pub struct RuntimeTelemetry {
    log_path: PathBuf,
    state: Mutex<RuntimeTelemetryState>,
}

impl RuntimeTelemetry {
    pub fn new(root: &Path, app_version: &str) -> Self {
        #[cfg(target_os = "linux")]
        let compositor = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .unwrap_or_else(|_| "unknown".to_string());
        #[cfg(target_os = "windows")]
        let compositor = "DWM".to_string();
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let compositor = "unknown".to_string();

        #[cfg(target_os = "linux")]
        let compositor_version = environment_value("OVERMAX_TELEMETRY_COMPOSITOR_VERSION");
        #[cfg(not(target_os = "linux"))]
        let compositor_version = String::new();

        #[cfg(target_os = "linux")]
        let session_type = environment_value("XDG_SESSION_TYPE");
        #[cfg(target_os = "windows")]
        let session_type = "Win32".to_string();
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let session_type = "unknown".to_string();

        #[cfg(target_os = "linux")]
        let display = environment_value("DISPLAY");
        #[cfg(not(target_os = "linux"))]
        let display = String::new();

        #[cfg(target_os = "linux")]
        let wayland_display = environment_value("WAYLAND_DISPLAY");
        #[cfg(not(target_os = "linux"))]
        let wayland_display = String::new();

        let environment = RuntimeEnvironment {
            app_version: app_version.to_string(),
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            compositor,
            compositor_version,
            session_type,
            display,
            wayland_display,
            foreign_toplevel_version: None,
            output: None,
        };
        Self {
            log_path: root.join("cache").join("telemetry.log"),
            state: Mutex::new(RuntimeTelemetryState {
                environment,
                session_started: false,
                interval_started: Instant::now(),
                capture_attempts: 0,
                capture_successes: 0,
                capture_failures: 0,
                capture_samples_us: Vec::new(),
                last_capture_success: None,
                outputs_generated: 0,
                output_generation: 0,
                outputs_drained: 0,
                last_drained_generation: 0,
                output_to_drain: TimingAggregator::default(),
                publish_calls: 0,
                publish_accepted: 0,
                last_published_generation: 0,
                drain_to_publish: TimingAggregator::default(),
                snapshots_applied: 0,
                last_applied_generation: 0,
                publish_to_apply: TimingAggregator::default(),
                last_applied: None,
                snapshots_presented: 0,
                last_presented_generation: 0,
                apply_to_present: TimingAggregator::default(),
                window: None,
            }),
        }
    }

    pub fn record_wayland_capabilities(&self, foreign_toplevel_version: Option<u32>) {
        if let Ok(mut state) = self.state.lock() {
            state.environment.foreign_toplevel_version = foreign_toplevel_version;
        }
    }

    pub fn record_output_environment(
        &self,
        name: Option<&str>,
        logical_size: (i32, i32),
        physical_size: Option<(i32, i32)>,
        scale: f64,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.environment.output = Some(OutputEnvironment {
                name: name.unwrap_or("unknown").to_string(),
                logical_size,
                physical_size,
                scale,
            });
        }
    }

    pub fn start_session(&self) {
        let line = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.start_session(Instant::now());
            let environment = &state.environment;
            let foreign_toplevel = environment
                .foreign_toplevel_version
                .map_or_else(|| "unavailable".to_string(), |version| version.to_string());
            let output = environment
                .output
                .as_ref()
                .map_or_else(|| "pending".to_string(), format_output_environment);
            format!(
                "[TelemetrySession] measured_unix_ms={} overmax={} build={} compositor={} compositor_version={} session={} DISPLAY={} WAYLAND_DISPLAY={} foreign_toplevel={} output={}",
                unix_time_ms(),
                environment.app_version,
                environment.build_profile,
                environment.compositor,
                environment.compositor_version,
                environment.session_type,
                environment.display,
                environment.wayland_display,
                foreign_toplevel,
                output,
            )
        };
        self.append_line(&line);
    }

    pub fn record_window_observation(
        &self,
        target_xid: u64,
        active_xid: Option<u64>,
        active_in_client_list: Option<bool>,
        rect: (i32, i32, i32, i32),
        foreground: bool,
        fullscreen: bool,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.window = Some(WindowObservation {
                target_xid,
                active_xid,
                active_in_client_list,
                rect,
                foreground,
                fullscreen,
            });
        }
    }

    pub fn record_capture_attempt(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.capture_attempts += 1;
        }
    }

    pub fn record_capture_success(&self, elapsed_us: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.capture_successes += 1;
            state.capture_samples_us.push(elapsed_us);
            state.last_capture_success = Some(Instant::now());
        }
    }

    pub fn record_capture_failure(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.capture_failures += 1;
        }
    }

    pub fn record_output_generated(&self) -> DetectionDeliveryTelemetry {
        let now = Instant::now();
        let generation = self.state.lock().map_or(0, |mut state| {
            state.outputs_generated += 1;
            state.output_generation = state.output_generation.wrapping_add(1).max(1);
            state.output_generation
        });
        DetectionDeliveryTelemetry {
            generation,
            generated_at: now,
            drained_at: None,
            published_at: None,
        }
    }

    pub fn record_output_drained(&self, delivery: &mut DetectionDeliveryTelemetry) {
        let now = Instant::now();
        delivery.drained_at = Some(now);
        if let Ok(mut state) = self.state.lock() {
            state.outputs_drained += 1;
            state.last_drained_generation = delivery.generation;
            state
                .output_to_drain
                .update(elapsed_us(delivery.generated_at, now));
        }
    }

    pub fn record_publish(&self, delivery: &mut DetectionDeliveryTelemetry, accepted: bool) {
        let now = Instant::now();
        if let Ok(mut state) = self.state.lock() {
            state.publish_calls += 1;
            if !accepted {
                return;
            }
            delivery.published_at = Some(now);
            state.publish_accepted += 1;
            state.last_published_generation = delivery.generation;
            if let Some(drained_at) = delivery.drained_at {
                state.drain_to_publish.update(elapsed_us(drained_at, now));
            }
        }
    }

    pub fn record_snapshot_applied(&self, delivery: &DetectionDeliveryTelemetry) {
        let now = Instant::now();
        if let Ok(mut state) = self.state.lock() {
            state.snapshots_applied += 1;
            state.last_applied_generation = delivery.generation;
            state.last_applied = Some((delivery.generation, now));
            if let Some(published_at) = delivery.published_at {
                state.publish_to_apply.update(elapsed_us(published_at, now));
            }
        }
    }

    pub fn record_snapshot_presented(&self, delivery: &DetectionDeliveryTelemetry) {
        let now = Instant::now();
        if let Ok(mut state) = self.state.lock() {
            if state.last_presented_generation == delivery.generation {
                return;
            }
            state.snapshots_presented += 1;
            state.last_presented_generation = delivery.generation;
            if let Some((generation, applied_at)) = state.last_applied {
                if generation == delivery.generation {
                    state.apply_to_present.update(elapsed_us(applied_at, now));
                }
            }
        }
    }

    pub fn maybe_log(&self) {
        let now = Instant::now();
        let line = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let period = now.duration_since(state.interval_started);
            if !state.session_started || period < RUNTIME_SNAPSHOT_INTERVAL {
                return;
            }
            state.capture_samples_us.sort_unstable();
            let capture_avg = if state.capture_samples_us.is_empty() {
                0
            } else {
                state.capture_samples_us.iter().sum::<u64>() / state.capture_samples_us.len() as u64
            };
            let capture_p95 = percentile_95(&state.capture_samples_us);
            let capture_max = state.capture_samples_us.last().copied().unwrap_or(0);
            let last_success_age = state
                .last_capture_success
                .map_or(u64::MAX, |instant| elapsed_us(instant, now));
            let window = state
                .window
                .map_or_else(|| "missing".to_string(), format_window_observation);
            let output = state
                .environment
                .output
                .as_ref()
                .map_or_else(|| "pending".to_string(), format_output_environment);
            let line = format!(
                "[TelemetryFlow] period={:.1}s capture={}/{}/{} time={}/{}/{} last_success_age={} generated={} last_output_gen={} drain={} last_gen={} latency={}/{} publish={}/{} last_gen={} latency={}/{} apply={} last_gen={} latency={}/{} present={} last_gen={} latency={}/{} window={} output={}",
                period.as_secs_f32(),
                state.capture_attempts,
                state.capture_successes,
                state.capture_failures,
                format_duration_us(capture_avg),
                format_duration_us(capture_p95),
                format_duration_us(capture_max),
                format_optional_duration(last_success_age),
                state.outputs_generated,
                state.output_generation,
                state.outputs_drained,
                state.last_drained_generation,
                format_duration_us(state.output_to_drain.avg_us()),
                format_duration_us(state.output_to_drain.max_us),
                state.publish_calls,
                state.publish_accepted,
                state.last_published_generation,
                format_duration_us(state.drain_to_publish.avg_us()),
                format_duration_us(state.drain_to_publish.max_us),
                state.snapshots_applied,
                state.last_applied_generation,
                format_duration_us(state.publish_to_apply.avg_us()),
                format_duration_us(state.publish_to_apply.max_us),
                state.snapshots_presented,
                state.last_presented_generation,
                format_duration_us(state.apply_to_present.avg_us()),
                format_duration_us(state.apply_to_present.max_us),
                window,
                output,
            );
            state.reset_interval(now);
            line
        };
        self.append_line(&line);
    }

    fn append_line(&self, line: &str) {
        use std::fs::OpenOptions;
        use std::io::Write;
        if let Some(parent) = self.log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = writeln!(file, "[{}] {line}", current_timestamp_str());
        }
    }
}

#[allow(dead_code)]
fn environment_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "unset".to_string())
}

fn unix_time_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn elapsed_us(start: Instant, end: Instant) -> u64 {
    end.saturating_duration_since(start)
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

fn percentile_95(sorted_samples: &[u64]) -> u64 {
    if sorted_samples.is_empty() {
        return 0;
    }
    let index = sorted_samples.len().saturating_mul(95).div_ceil(100) - 1;
    sorted_samples[index]
}

fn format_optional_duration(us: u64) -> String {
    if us == u64::MAX {
        "never".to_string()
    } else {
        format_duration_us(us)
    }
}

fn format_window_observation(window: WindowObservation) -> String {
    let active = window
        .active_xid
        .map_or_else(|| "none".to_string(), |xid| format!("0x{xid:x}"));
    let member = window
        .active_in_client_list
        .map_or("unknown", |member| if member { "yes" } else { "no" });
    format!(
        "target=0x{:x},active={},active_client={},rect={}x{}@{},{} foreground={} fullscreen={}",
        window.target_xid,
        active,
        member,
        window.rect.2,
        window.rect.3,
        window.rect.0,
        window.rect.1,
        window.foreground,
        window.fullscreen,
    )
}

fn format_output_environment(output: &OutputEnvironment) -> String {
    let physical = output.physical_size.map_or_else(
        || "unknown".to_string(),
        |size| format!("{}x{}", size.0, size.1),
    );
    format!(
        "{},logical={}x{},physical={},scale={:.3}",
        output.name, output.logical_size.0, output.logical_size.1, physical, output.scale
    )
}

pub fn current_timestamp_str() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    let total_secs = secs + 9 * 3600;
    let time_in_day = total_secs % 86400;
    let hours = time_in_day / 3600;
    let minutes = (time_in_day % 3600) / 60;
    let seconds = time_in_day % 60;

    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimingAggregator {
    pub total_us: u64,
    pub max_us: u64,
    pub count: u32,
}

impl TimingAggregator {
    pub fn update(&mut self, elapsed_us: u64) {
        self.total_us += elapsed_us;
        if elapsed_us > self.max_us {
            self.max_us = elapsed_us;
        }
        self.count += 1;
    }

    pub fn avg_us(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.total_us / self.count as u64
        }
    }

    pub fn avg_ms(&self) -> f32 {
        self.avg_us() as f32 / 1000.0
    }

    pub fn max_ms(&self) -> f32 {
        self.max_us as f32 / 1000.0
    }

    pub fn reset(&mut self) {
        self.total_us = 0;
        self.max_us = 0;
        self.count = 0;
    }
}

pub fn format_duration_us(us: u64) -> String {
    if us >= 10_000 {
        format!("{:.1}ms", us as f64 / 1000.0)
    } else if us >= 1_000 {
        format!("{:.2}ms", us as f64 / 1000.0)
    } else {
        format!("{}µs", us)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PipelineTelemetrySnapshot {
    pub period_sec: f32,
    pub capture_avg_us: u64,
    pub capture_max_us: u64,
    pub detect_avg_us: u64,
    pub detect_max_us: u64,
    pub scene_avg_us: u64,
    pub scene_max_us: u64,
    pub jacket_avg_us: u64,
    pub jacket_max_us: u64,
    pub play_state_avg_us: u64,
    pub play_state_max_us: u64,
    pub active_frames: u32,
    pub unknown_frames: u32,
    /// 씬 폴링 파싱 실패(미스) 횟수
    pub scene_poll_misses: u32,
    /// 미스 시 관측된 썸네일 diff 최댓값 (게이트 임계 2.5 대조용)
    pub scene_miss_max_thumb_diff: f32,
    /// Centroid Kernel 게이트 동시 거절 횟수
    pub scene_miss_centroid_rejects: u32,
    /// 카테고리 띠 검사 동시 탈락 횟수
    pub scene_miss_band_rejects: u32,
    /// 유사도 미달로 탈락한 경우의 최저 top-similarity (0.0 = 해당 없음)
    pub scene_miss_min_top_similarity: f32,
    pub match_jacket_count: u32,
}

#[derive(Debug)]
pub struct PipelineStatsCollector {
    pub capture: TimingAggregator,
    pub detect: TimingAggregator,
    pub scene: TimingAggregator,
    pub jacket: TimingAggregator,
    pub play_state: TimingAggregator,
    pub active_frames: u32,
    pub unknown_frames: u32,
    pub scene_poll_misses: u32,
    pub scene_miss_max_thumb_diff: f32,
    pub scene_miss_centroid_rejects: u32,
    pub scene_miss_band_rejects: u32,
    pub scene_miss_min_top_similarity: f32,
    pub match_jacket_count: u32,
    pub last_snapshot_ts: Instant,
}

impl Default for PipelineStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStatsCollector {
    pub fn new() -> Self {
        Self {
            capture: TimingAggregator::default(),
            detect: TimingAggregator::default(),
            scene: TimingAggregator::default(),
            jacket: TimingAggregator::default(),
            play_state: TimingAggregator::default(),
            active_frames: 0,
            unknown_frames: 0,
            scene_poll_misses: 0,
            scene_miss_max_thumb_diff: 0.0,
            scene_miss_centroid_rejects: 0,
            scene_miss_band_rejects: 0,
            scene_miss_min_top_similarity: 0.0,
            match_jacket_count: 0,
            last_snapshot_ts: Instant::now(),
        }
    }

    /// 씬 폴링 미스 기록. thumb_diff는 참조 썸네일과의 평균 픽셀 차이 (없으면 None).
    pub fn record_scene_miss(
        &mut self,
        thumb_diff: Option<f32>,
        diag: crate::detection_pipeline::SceneMissDiag,
    ) {
        self.scene_poll_misses += 1;
        if let Some(diff) = thumb_diff {
            if diff > self.scene_miss_max_thumb_diff {
                self.scene_miss_max_thumb_diff = diff;
            }
        }
        if diag.centroid_rejected {
            self.scene_miss_centroid_rejects += 1;
        } else if diag.band_rejected {
            self.scene_miss_band_rejects += 1;
        }
        if let Some(sim) = diag.top_similarity {
            let min = if self.scene_miss_min_top_similarity > 0.0 {
                self.scene_miss_min_top_similarity
            } else {
                f32::MAX
            };
            self.scene_miss_min_top_similarity = sim.min(min);
        }
    }

    pub fn record_match_jacket(&mut self) {
        self.match_jacket_count += 1;
    }

    pub fn record_frame_status(&mut self, is_active: bool) {
        if is_active {
            self.active_frames += 1;
        } else {
            self.unknown_frames += 1;
        }
    }

    pub fn maybe_take_snapshot(&mut self, interval_sec: f32) -> Option<PipelineTelemetrySnapshot> {
        let elapsed = self.last_snapshot_ts.elapsed().as_secs_f32();
        if elapsed < interval_sec {
            return None;
        }

        let snapshot = PipelineTelemetrySnapshot {
            period_sec: elapsed,
            capture_avg_us: self.capture.avg_us(),
            capture_max_us: self.capture.max_us,
            detect_avg_us: self.detect.avg_us(),
            detect_max_us: self.detect.max_us,
            scene_avg_us: self.scene.avg_us(),
            scene_max_us: self.scene.max_us,
            jacket_avg_us: self.jacket.avg_us(),
            jacket_max_us: self.jacket.max_us,
            play_state_avg_us: self.play_state.avg_us(),
            play_state_max_us: self.play_state.max_us,
            active_frames: self.active_frames,
            unknown_frames: self.unknown_frames,
            scene_poll_misses: self.scene_poll_misses,
            scene_miss_max_thumb_diff: self.scene_miss_max_thumb_diff,
            scene_miss_centroid_rejects: self.scene_miss_centroid_rejects,
            scene_miss_band_rejects: self.scene_miss_band_rejects,
            scene_miss_min_top_similarity: if self.scene_miss_min_top_similarity == f32::MAX {
                0.0
            } else {
                self.scene_miss_min_top_similarity
            },
            match_jacket_count: self.match_jacket_count,
        };

        self.reset();
        Some(snapshot)
    }

    pub fn reset(&mut self) {
        self.capture.reset();
        self.detect.reset();
        self.scene.reset();
        self.jacket.reset();
        self.play_state.reset();
        self.active_frames = 0;
        self.unknown_frames = 0;
        self.scene_poll_misses = 0;
        self.scene_miss_max_thumb_diff = 0.0;
        self.scene_miss_centroid_rejects = 0;
        self.scene_miss_band_rejects = 0;
        self.scene_miss_min_top_similarity = 0.0;
        self.match_jacket_count = 0;
        self.last_snapshot_ts = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_delivery_generations_and_capture_p95_are_stable() {
        let telemetry = RuntimeTelemetry::new(Path::new("unused"), "test");
        let first = telemetry.record_output_generated();
        telemetry
            .state
            .lock()
            .unwrap()
            .start_session(Instant::now());
        let mut delivery = telemetry.record_output_generated();
        telemetry.record_output_drained(&mut delivery);
        telemetry.record_publish(&mut delivery, true);
        telemetry.record_snapshot_applied(&delivery);
        telemetry.record_snapshot_presented(&delivery);
        telemetry.record_snapshot_presented(&delivery);

        let state = telemetry.state.lock().unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(delivery.generation, 2);
        assert_eq!(state.outputs_generated, 1);
        assert_eq!(state.last_drained_generation, 2);
        assert_eq!(state.last_published_generation, 2);
        assert_eq!(state.last_applied_generation, 2);
        assert_eq!(state.last_presented_generation, 2);
        assert_eq!(state.snapshots_presented, 1);
        assert_eq!(percentile_95(&[10, 20, 30, 40, 50]), 50);
    }

    #[test]
    fn timing_aggregator_avg_and_max() {
        let mut agg = TimingAggregator::default();
        agg.update(1000);
        agg.update(3000);
        agg.update(2000);

        assert_eq!(agg.count, 3);
        assert_eq!(agg.total_us, 6000);
        assert_eq!(agg.max_us, 3000);
        assert_eq!(agg.avg_us(), 2000);
        assert_eq!(agg.avg_ms(), 2.0);
        assert_eq!(agg.max_ms(), 3.0);

        agg.reset();
        assert_eq!(agg.count, 0);
        assert_eq!(agg.total_us, 0);
        assert_eq!(agg.max_us, 0);
        assert_eq!(agg.avg_us(), 0);
    }

    #[test]
    fn collector_snapshot_interval() {
        let mut collector = PipelineStatsCollector::new();
        collector.capture.update(1000);
        collector.record_frame_status(true);
        collector.record_match_jacket();

        assert!(collector.maybe_take_snapshot(10.0).is_none());
        assert_eq!(collector.capture.count, 1);

        let snapshot = collector.maybe_take_snapshot(0.0).unwrap();
        assert_eq!(snapshot.active_frames, 1);
        assert_eq!(snapshot.match_jacket_count, 1);
        assert_eq!(collector.capture.count, 0);
    }
}
