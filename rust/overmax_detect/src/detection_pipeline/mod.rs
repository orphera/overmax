use crate::hysteresis::HysteresisBuffer;
use crate::jacket_index::JacketIndex;
use crate::play_state::PlayStateDetector;
use crate::roi::RoiManager;
use crate::telemetry::{PipelineStatsCollector, PipelineTelemetrySnapshot};
use overmax_core::{GameSessionState, SceneType, VerifiedPlayEvent};
use overmax_frame::CapturedFrame;
use overmax_frame::WindowSnapshot;
use overmax_frame::{make_thumbnail, mean_abs_diff, thumbnail_changed};
use std::time::Instant;

const JACKET_MATCH_INTERVAL: f64 = 0.25;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const JACKET_FORCE_RECHECK_LONG_SEC: f64 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepHint {
    /// 씬 신호가 있거나 Unknown 진입 초기(<3s): 고속 폴링 주기로 슬립
    Active,
    /// 장기 Unknown 상태: 완화된 주기로 슬립하여 불필요한 캡처 비용 절감
    Relaxed,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneFrameView {
    pub scene_detected: bool,
    pub is_song_select: bool,
    pub is_result: bool,
    pub is_leaving: bool,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetectionOutput {
    pub scene_detected: bool,
    pub is_song_select: bool,
    pub is_result: bool,
    pub is_leaving: bool,
    pub confidence: f32,
    pub state: GameSessionState,
    pub event: Option<VerifiedPlayEvent>,
    pub current_song_id: Option<i32>,
    pub image_db_ready: bool,
    pub jacket_status: JacketMatchStatus,
    pub game_rect: Option<overmax_frame::WindowRect>,
    pub window_snapshot: Option<WindowSnapshot>,
    pub capture_fatal: Option<String>,
    pub top_jacket_similarity: Option<f32>,
    pub roi_scale: f32,
    pub roi_offset_y: i32,
    pub stable_hits: u32,
    pub sleep_hint: SleepHint,
    pub telemetry_snapshot: Option<PipelineTelemetrySnapshot>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub delivery_telemetry: Option<crate::telemetry::DetectionDeliveryTelemetry>,
}

/// 씬 폴링 파싱 실패 시의 원인 진단 정보 (텔레메트리 수집용)
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneMissDiag {
    /// 자켓 Centroid Kernel 1차 게이트에서 거절된 경우
    pub centroid_rejected: bool,
    /// 카테고리 띠 단색성 검사에서 탈락한 경우
    pub band_rejected: bool,
    /// 매칭은 했으나 유사도 임계 미달일 때의 최고 유사도
    pub top_similarity: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JacketMatchStatus {
    NotSongSelect,
    Leaving,
    DbNotReady,
    Cooldown,
    CropMissing,
    ThumbnailMissing,
    Unchanged,
    NoMatch,
    InvalidId { image_id: String, similarity: f32 },
    Matched { song_id: i32, similarity: f32 },
}

pub struct DetectionPipeline<J: JacketIndex> {
    pub stats: PipelineStatsCollector,
    jacket_index: J,
    rois: RoiManager,
    hysteresis: HysteresisBuffer,
    play_state: PlayStateDetector,
    current_song_id: Option<i32>,
    last_scene_check_ts: f64,
    last_scene: SceneType,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
    scene_streak: u32,
    pending_scene: SceneType,
    unknown_since: Option<f64>,
    last_top_jacket_similarity: Option<f32>,
    recovering_from_suspension: bool,
}

impl<J: JacketIndex> DetectionPipeline<J> {
    /// `J`로 변환 가능한 값(예: `ImageIndexDb` → `ImageIndexJacket`)에서 파이프라인을 만든다.
    pub fn new(jacket_index: impl Into<J>) -> Self {
        Self::with_jacket_index(jacket_index.into())
    }

    pub fn with_jacket_index(jacket_index: J) -> Self {
        Self {
            stats: PipelineStatsCollector::new(),
            jacket_index,
            rois: RoiManager::new(1920, 1080),
            hysteresis: HysteresisBuffer::new(4, 0.5, 2, 0.25, 2),
            play_state: PlayStateDetector::new(5),
            current_song_id: None,
            last_scene_check_ts: 0.0,
            last_scene: SceneType::Unknown,
            last_jacket_ts: 0.0,
            last_jacket_match_ts: 0.0,
            last_jacket_thumb: None,
            scene_streak: 0,
            pending_scene: SceneType::Unknown,
            unknown_since: None,
            last_top_jacket_similarity: None,
            recovering_from_suspension: false,
        }
    }

    /// 갱신된 자켓 인덱스로 교체한다. 곡/씬 이력은 유지하고 인덱스만 바꾼다.
    pub fn replace_jacket_index(&mut self, jacket_index: J) {
        self.jacket_index = jacket_index;
    }

    pub fn reset(&mut self) {
        self.reset_tracking();
        self.play_state.reset();
        self.play_state.clear_detected_cache();
    }

    /// Invalidate every visual observation while retaining completion deduplication
    /// for the same capture session and target. A new target must use `reset`.
    pub fn suspend(&mut self) {
        self.reset_tracking();
        self.play_state.suspend();
        self.recovering_from_suspension = true;
    }

    fn reset_tracking(&mut self) {
        self.stats.reset();
        self.current_song_id = None;
        self.last_scene_check_ts = 0.0;
        self.last_scene = SceneType::Unknown;
        self.last_jacket_ts = 0.0;
        self.last_jacket_match_ts = 0.0;
        self.last_jacket_thumb = None;
        self.scene_streak = 0;
        self.pending_scene = SceneType::Unknown;
        self.unknown_since = None;
        self.last_top_jacket_similarity = None;
        self.recovering_from_suspension = false;
        self.hysteresis.reset();
        self.rois.set_scene(SceneType::Unknown);
    }

    pub fn detect(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        let expected_bytes = usize::try_from(frame.width)
            .ok()
            .zip(usize::try_from(frame.height).ok())
            .and_then(|(w, h)| w.checked_mul(h)?.checked_mul(4));
        if expected_bytes != Some(frame.bgra.len()) || frame.width <= 0 || frame.height <= 0 {
            self.reset();
            return self.process_frame_shared(frame, false, now);
        }
        // A backend/format change may arrive on a cached tick. In-game history
        // cannot outlive the availability of its evidence, even before polling.
        if (self.last_scene.is_ingame() || self.pending_scene.is_ingame())
            && !crate::templates::gameplay_scene::supports_frame(frame)
        {
            self.commit_scene(SceneType::Unknown);
        }
        self.rois.update_window_size(frame.width, frame.height);

        let scene_start = Instant::now();
        let maybe_scene = self.detect_scene_if_due(frame, now);
        let scene_elapsed = scene_start.elapsed().as_micros() as u64;
        self.stats.scene.update(scene_elapsed);

        if let Some(scene) = maybe_scene {
            self.process_frame_with_scene(frame, scene, now)
        } else {
            self.process_frame_cached(frame, now)
        }
    }

    pub fn process_frame_with_scene(
        &mut self,
        frame: &CapturedFrame,
        scene: SceneType,
        now: f64,
    ) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);

        if scene != SceneType::Unknown {
            // Once a scene is reacquired, later Unknown exits delimit plays again.
            self.recovering_from_suspension = false;
        }

        let scene_detected = scene.is_record_scene();
        if scene_detected {
            self.rois.set_scene(scene);
        }

        self.hysteresis.update(scene_detected);
        self.process_frame_shared(frame, scene_detected, now)
    }

    pub fn process_frame_cached(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);

        let scene_detected = self.last_scene.is_record_scene();
        self.process_frame_shared(frame, scene_detected, now)
    }

    fn process_frame_shared(
        &mut self,
        frame: &CapturedFrame,
        scene_detected: bool,
        now: f64,
    ) -> DetectionOutput {
        self.stats.record_frame_status(scene_detected);

        // Unknown 진입 시점 추적: 씬 폴링/슬립 스케줄링이 어느 진입 경로든 동일하게 참조
        let acquiring = !scene_detected && !self.hysteresis.is_active;
        if acquiring {
            if self.unknown_since.is_none() {
                self.unknown_since = Some(now);
            }
        } else {
            self.unknown_since = None;
        }
        let sleep_hint = self.compute_sleep_hint(acquiring, now);

        let view = SceneFrameView {
            scene_detected: scene_detected || self.last_scene.is_ingame(),
            is_song_select: self.hysteresis.is_active || self.last_scene.is_result(),
            is_result: self.last_scene.is_result(),
            is_leaving: !self.last_scene.is_result() && self.hysteresis.is_leaving,
            confidence: self.hysteresis.confidence,
        };

        if !view.is_song_select {
            self.reset_on_screen_exit();
            return self.output(
                &view,
                GameSessionState {
                    scene: if self.last_scene.is_ingame() {
                        self.last_scene
                    } else {
                        SceneType::Unknown
                    },
                    ..GameSessionState::detecting()
                },
                JacketMatchStatus::NotSongSelect,
                None,
                sleep_hint,
            );
        }

        if view.is_leaving {
            return self.output(
                &view,
                GameSessionState::detecting(),
                JacketMatchStatus::Leaving,
                None,
                sleep_hint,
            );
        }

        let jacket_start = Instant::now();
        let jacket_status = self.update_song_id_from_jacket(frame, now);
        let jacket_elapsed = jacket_start.elapsed().as_micros() as u64;
        self.stats.jacket.update(jacket_elapsed);

        let play_state_start = Instant::now();
        let (state, event) = self
            .play_state
            .detect(frame, &self.rois, self.current_song_id, now);
        let play_state_elapsed = play_state_start.elapsed().as_micros() as u64;
        self.stats.play_state.update(play_state_elapsed);

        self.output(&view, state, jacket_status, event, sleep_hint)
    }

    /// 슬립 스케줄링 힌트: 씬 신호가 있거나 Unknown 진입 초기에는 고속 폴링으로
    /// 씬 전환 반응성을 확보하고, 장기 Unknown 상태에서는 완화 주기로 전환한다.
    fn compute_sleep_hint(&self, acquiring: bool, now: f64) -> SleepHint {
        if !acquiring {
            return SleepHint::Active;
        }
        let unknown_duration = now - self.unknown_since.unwrap_or(now);
        if unknown_duration < 3.0 {
            SleepHint::Active
        } else {
            SleepHint::Relaxed
        }
    }

    fn detect_scene_if_due(&mut self, frame: &CapturedFrame, now: f64) -> Option<SceneType> {
        // 탐색 초기에는 0.3초, 장기 탐색/인게임은 1.5초, 안정된 기록 씬은 2초.
        // 참고: unknown_since 타임라인은 process_frame_shared 가 단일 소유자로 갱신한다.
        let acquiring = !self.last_scene.is_record_scene() || !self.hysteresis.is_active;

        let cooldown: f64 = if acquiring {
            let unknown_duration = now - self.unknown_since.unwrap_or(now);
            if unknown_duration < 3.0 {
                0.3
            } else {
                1.5
            }
        } else {
            2.0
        };

        // Confirm a new in-game candidate on the next relaxed capture without
        // increasing the sustained polling rate or counting cached ticks.
        let cooldown = if self.pending_scene.is_ingame() && self.scene_streak == 1 {
            cooldown.min(1.0)
        } else {
            cooldown
        };

        if now - self.last_scene_check_ts < cooldown {
            return None;
        }

        let gameplay_scene = crate::templates::gameplay_scene::read_scene(frame, &self.rois);
        let is_ingame = gameplay_scene.is_ingame();
        let final_scene: SceneType;
        if is_ingame {
            debug_println!(
                "    [detect_scene_if_due] now={}, ingame_scene={:?}",
                now,
                gameplay_scene
            );
            final_scene = self.commit_scene(gameplay_scene);
        } else {
            let is_unknown = !self.last_scene.is_record_scene();
            let (static_scene, miss_diag) =
                parse_static_scene(frame, &self.rois, &self.jacket_index, is_unknown);
            if let Some((scene, matched_song_id)) = static_scene {
                if let Some(song_id) = matched_song_id {
                    self.current_song_id = Some(song_id);
                    self.last_jacket_match_ts = now;
                    if let Some(thumb) = self.rois.and_then_roi(frame, "jacket", make_thumbnail) {
                        self.last_jacket_thumb = Some(thumb);
                    }
                }
                debug_println!(
                    "    [detect_scene_if_due] now={}, static_scene={:?}",
                    now,
                    scene
                );
                if scene.is_record_scene() {
                    self.rois.set_scene(scene);
                }
                final_scene = self.commit_scene(scene);
            } else {
                debug_println!("    [detect_scene_if_due] scene miss! now={}", now);
                let thumb_diff = self.screen_static_thumb_diff(frame);
                self.stats.record_scene_miss(thumb_diff, miss_diag);
                final_scene = self.commit_scene(SceneType::Unknown);
            }
        };
        self.last_scene_check_ts = now;
        Some(final_scene)
    }

    /// 현재 프레임의 자켓 ROI 썸네일과 마지막 저장 썸네일의 평균 픽셀 차이.
    /// 참조 썸네일 부재 또는 ROI 크롭 실패 시 None (정적 여부 판단 불가).
    fn screen_static_thumb_diff(&self, frame: &CapturedFrame) -> Option<f32> {
        let previous = self.last_jacket_thumb.as_deref()?;
        let current = self.rois.and_then_roi(frame, "jacket", make_thumbnail)?;
        Some(mean_abs_diff(&current, previous))
    }

    /// Results and in-game scenes share the existing two-inspection commitment.
    /// A miss breaks the streak; cached ticks never call this method.
    fn commit_scene(&mut self, candidate: SceneType) -> SceneType {
        if candidate.is_result() || candidate.is_ingame() {
            if candidate == self.pending_scene {
                self.scene_streak = self.scene_streak.saturating_add(1).min(2);
            } else {
                self.pending_scene = candidate;
                self.scene_streak = 1;
            }
            if candidate.is_ingame() {
                // Positive non-record evidence must close stale selection state
                // even while the new scene is awaiting its second inspection.
                self.hysteresis.reset();
                self.last_scene = SceneType::Unknown;
            } else if self.last_scene.is_ingame() {
                self.last_scene = SceneType::Unknown;
            }
            if self.scene_streak >= 2 {
                self.last_scene = candidate;
            }
        } else {
            self.scene_streak = 0;
            self.pending_scene = SceneType::Unknown;
            self.last_scene = candidate;
        }
        self.last_scene
    }

    fn update_song_id_from_jacket(&mut self, frame: &CapturedFrame, now: f64) -> JacketMatchStatus {
        if !self.jacket_index.is_ready() {
            return JacketMatchStatus::DbNotReady;
        }
        if now - self.last_jacket_ts < JACKET_MATCH_INTERVAL {
            return JacketMatchStatus::Cooldown;
        }
        self.last_jacket_ts = now;
        let Some(jacket) = self.rois.get_roi("jacket").and_then(|roi| roi.crop(frame)) else {
            return JacketMatchStatus::CropMissing;
        };
        let Some(thumb) = make_thumbnail(&jacket) else {
            return JacketMatchStatus::ThumbnailMissing;
        };
        let image_changed = thumbnail_changed(
            &thumb,
            self.last_jacket_thumb.as_deref(),
            JACKET_CHANGE_THRESHOLD,
        );
        if !self.should_match_jacket(image_changed, now) {
            return JacketMatchStatus::Unchanged;
        }

        self.last_jacket_thumb = Some(thumb);
        self.last_jacket_match_ts = now;
        let jacket_region = jacket.to_image_region();
        self.apply_jacket_match(&jacket_region)
    }

    fn apply_jacket_match(&mut self, jacket: &overmax_frame::ImageRegion) -> JacketMatchStatus {
        self.stats.record_match_jacket();
        let Some(result) = self.jacket_index.match_jacket(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        ) else {
            self.current_song_id = None;
            self.last_top_jacket_similarity = None;
            return JacketMatchStatus::NoMatch;
        };
        self.last_top_jacket_similarity = Some(result.similarity);
        match result.image_id.parse::<i32>() {
            Ok(song_id) => {
                self.current_song_id = Some(song_id);
                JacketMatchStatus::Matched {
                    song_id,
                    similarity: result.similarity,
                }
            }
            Err(_) => {
                self.current_song_id = None;
                JacketMatchStatus::InvalidId {
                    image_id: result.image_id,
                    similarity: result.similarity,
                }
            }
        }
    }

    fn reset_on_screen_exit(&mut self) {
        self.current_song_id = None;
        self.last_top_jacket_similarity = None;
        if self.recovering_from_suspension && self.last_scene == SceneType::Unknown {
            // Preserve only across explicit suspension recovery, including the
            // first result candidate before its second scene inspection.
            self.play_state.suspend();
        } else {
            // Ordinary exits also delimit plays when gameplay is unrecognized.
            self.play_state.reset();
        }
        // 재진입 시 이전 자켓과 동일한 곡이어도 즉시 매칭이 실행되도록 초기화.
        self.last_jacket_thumb = None;
        self.last_jacket_match_ts = 0.0;
    }

    fn should_match_jacket(&self, image_changed: bool, now: f64) -> bool {
        let limit = if self.current_song_id.is_some() {
            JACKET_FORCE_RECHECK_LONG_SEC
        } else {
            JACKET_FORCE_RECHECK_SEC
        };
        image_changed || now - self.last_jacket_match_ts >= limit
    }

    fn output(
        &self,
        view: &SceneFrameView,
        state: GameSessionState,
        jacket_status: JacketMatchStatus,
        event: Option<VerifiedPlayEvent>,
        sleep_hint: SleepHint,
    ) -> DetectionOutput {
        DetectionOutput {
            scene_detected: view.scene_detected,
            is_song_select: view.is_song_select,
            is_result: view.is_result,
            is_leaving: view.is_leaving,
            confidence: view.confidence,
            state,
            event,
            current_song_id: self.current_song_id,
            image_db_ready: self.jacket_index.is_ready(),
            jacket_status,
            game_rect: None,
            window_snapshot: None,
            capture_fatal: None,
            top_jacket_similarity: self.last_top_jacket_similarity,
            roi_scale: self.rois.scale(),
            roi_offset_y: self.rois.offset_y(),
            stable_hits: self.play_state.stable_hits(),
            sleep_hint,
            telemetry_snapshot: None,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            delivery_telemetry: None,
        }
    }
}

mod scene;
use scene::parse_static_scene;
pub use scene::{
    check_open_match_badge, detect_freestyle_result_colorbar_match, detect_static_scene,
};

#[cfg(test)]
mod tests;
