use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{make_thumbnail, mean_abs_diff, thumbnail_changed};
use crate::capture::window_tracker::WindowSnapshot;
use crate::detector::hysteresis::HysteresisBuffer;
use crate::detector::play_state::PlayStateDetector;
use crate::detector::roi::RoiManager;
use crate::detector::telemetry::{PipelineStatsCollector, PipelineTelemetrySnapshot};
use overmax_core::{GameSessionState, SceneType, VerifiedPlayEvent};
use overmax_data::ImageIndexDb;
use std::time::Instant;

const JACKET_MATCH_INTERVAL: f64 = 0.25;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const JACKET_FORCE_RECHECK_LONG_SEC: f64 = 30.0;
const STRICT_EDGE_THRESHOLD: f32 = 25.0;

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
    pub game_rect: Option<crate::capture::window_tracker::WindowRect>,
    pub window_snapshot: Option<WindowSnapshot>,
    pub capture_fatal: Option<String>,
    pub top_jacket_similarity: Option<f32>,
    pub roi_scale: f32,
    pub roi_offset_y: i32,
    pub stable_hits: u32,
    pub sleep_hint: SleepHint,
    pub telemetry_snapshot: Option<PipelineTelemetrySnapshot>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub delivery_telemetry: Option<crate::detector::telemetry::DetectionDeliveryTelemetry>,
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

pub struct DetectionPipeline {
    pub stats: PipelineStatsCollector,
    image_db: ImageIndexDb,
    jacket_matcher: overmax_data::JacketMatcher,
    rois: RoiManager,
    hysteresis: HysteresisBuffer,
    play_state: PlayStateDetector,
    current_song_id: Option<i32>,
    last_scene_check_ts: f64,
    last_static_scene: SceneType,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
    result_scene_streak: u32,
    last_detected_result_scene: SceneType,
    unknown_since: Option<f64>,
    last_top_jacket_similarity: Option<f32>,
}

impl DetectionPipeline {
    pub fn new(image_db: ImageIndexDb) -> Self {
        let jacket_matcher = image_db.matcher();
        Self {
            stats: PipelineStatsCollector::new(),
            image_db,
            jacket_matcher,
            rois: RoiManager::new(1920, 1080),
            hysteresis: HysteresisBuffer::new(4, 0.5, 2, 0.25, 2),
            play_state: PlayStateDetector::new(5),
            current_song_id: None,
            last_scene_check_ts: 0.0,
            last_static_scene: SceneType::Unknown,
            last_jacket_ts: 0.0,
            last_jacket_match_ts: 0.0,
            last_jacket_thumb: None,
            result_scene_streak: 0,
            last_detected_result_scene: SceneType::Unknown,
            unknown_since: None,
            last_top_jacket_similarity: None,
        }
    }

    pub fn reset(&mut self) {
        self.stats.reset();
        self.current_song_id = None;
        self.last_scene_check_ts = 0.0;
        self.last_static_scene = SceneType::Unknown;
        self.last_jacket_ts = 0.0;
        self.last_jacket_match_ts = 0.0;
        self.last_jacket_thumb = None;
        self.result_scene_streak = 0;
        self.last_detected_result_scene = SceneType::Unknown;
        self.unknown_since = None;
        self.hysteresis.reset();
        self.play_state.reset();
        self.play_state.clear_detected_cache();
        self.rois.set_scene(SceneType::Unknown);
    }

    pub fn detect(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
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

        let scene_detected = scene != SceneType::Unknown && scene != SceneType::Online;
        if scene_detected {
            self.rois.set_scene(scene);
        }

        self.hysteresis.update(scene_detected);
        self.process_frame_shared(frame, scene_detected, now)
    }

    pub fn process_frame_cached(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);

        let scene_detected = self.last_static_scene != SceneType::Unknown
            && self.last_static_scene != SceneType::Online;
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
            scene_detected,
            is_song_select: self.hysteresis.is_active || self.last_static_scene.is_result(),
            is_result: self.last_static_scene.is_result(),
            is_leaving: !self.last_static_scene.is_result() && self.hysteresis.is_leaving,
            confidence: self.hysteresis.confidence,
        };

        if !view.is_song_select {
            self.reset_on_screen_exit();
            return self.output(
                &view,
                GameSessionState::detecting(),
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

        // 결과창에서 다시 선곡 화면으로 복귀하는 경우 결과창 캐시 리셋
        if !view.is_result {
            self.play_state.clear_detected_cache();
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
        // 씬이 Unknown인 경우(진입 대기): 빠른 인식을 위해 0.3초 주기로 감시
        // 씬이 이미 확정된 경우(유지 중): CPU 소모 최소화를 위해 2.0초 주기로 완화 (이탈은 픽셀 매칭으로 즉시 처리되므로 반응성 무관)
        // 참고: unknown_since 타임라인은 process_frame_shared 가 단일 소유자로 갱신한다.
        let acquiring = self.last_static_scene == SceneType::Unknown || !self.hysteresis.is_active;

        let cooldown = if acquiring {
            let unknown_duration = now - self.unknown_since.unwrap_or(now);
            if unknown_duration < 3.0 {
                0.3
            } else {
                1.5
            }
        } else {
            2.0
        };

        if now - self.last_scene_check_ts < cooldown {
            return None;
        }

        let is_unknown = self.last_static_scene == SceneType::Unknown;
        let (parse_res, miss_diag) =
            parse_static_scene(frame, &self.rois, &self.jacket_matcher, is_unknown);
        let Some((scene, matched_song_id)) = parse_res else {
            debug_println!("    [detect_scene_if_due] static scene miss! now={}", now);

            // 미스 진단 기록: 참조 썸네일 대비 픽셀 차이 + 거절 단계(centroid/band/유사도)
            let thumb_diff = self.screen_static_thumb_diff(frame);
            self.stats.record_scene_miss(thumb_diff, miss_diag);

            self.last_static_scene = SceneType::Unknown;
            self.last_scene_check_ts = now;
            return Some(SceneType::Unknown);
        };

        if let Some(song_id) = matched_song_id {
            self.current_song_id = Some(song_id);
            self.last_jacket_match_ts = now;

            // process_frame_shared 에서 중복 매칭이 돌지 않도록 썸네일 캐시 갱신
            if let Some(thumb) = self.rois.and_then_roi(frame, "jacket", make_thumbnail) {
                self.last_jacket_thumb = Some(thumb);
            }
        }

        debug_println!(
            "    [detect_scene_if_due] now={}, static_scene={:?}",
            now,
            scene
        );

        if scene != SceneType::Unknown && scene != SceneType::Online {
            self.rois.set_scene(scene);
        }

        let final_scene = self.commit_result_scene(scene);
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

    fn commit_result_scene(&mut self, candidate: SceneType) -> SceneType {
        let is_detected_result = candidate.is_result();

        if is_detected_result {
            if candidate == self.last_detected_result_scene {
                self.result_scene_streak += 1;
            } else {
                self.last_detected_result_scene = candidate;
                self.result_scene_streak = 1;
            }

            // 1프레임 대기 후, 2프레임차에 최종 검증 수행
            if self.result_scene_streak >= 2 {
                self.last_static_scene = candidate;
            }
        } else {
            self.result_scene_streak = 0;
            self.last_detected_result_scene = SceneType::Unknown;
            self.last_static_scene = candidate;
        }

        self.last_static_scene
    }

    fn update_song_id_from_jacket(&mut self, frame: &CapturedFrame, now: f64) -> JacketMatchStatus {
        if !self.image_db.is_ready() {
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

    fn apply_jacket_match(
        &mut self,
        jacket: &crate::capture::frame_utils::ImageRegion,
    ) -> JacketMatchStatus {
        self.stats.record_match_jacket();
        let Some(result) = self.jacket_matcher.match_jacket(
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
        self.play_state.reset();
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
            image_db_ready: self.image_db.is_ready(),
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

use crate::detector::templates::FREESTYLE_RESULT_MODE_COLORS;
use overmax_cv::Bgr;

fn get_min_color_distance(mean: Bgr, colors: &[Bgr]) -> f32 {
    let mut min_dist = f32::MAX;
    for color in colors {
        let dist = mean.distance_f32(*color);
        if dist < min_dist {
            min_dist = dist;
        }
    }
    min_dist
}

pub fn detect_freestyle_result_colorbar_match(mean: Bgr) -> bool {
    get_min_color_distance(mean, &FREESTYLE_RESULT_MODE_COLORS) <= 40.0f32
}

pub fn check_open_match_badge(frame: &CapturedFrame, rois: &RoiManager) -> Option<SceneType> {
    // PlayerPanel ROI 엣지 확인
    let edge_strength_result_open3 = rois
        .get_roi_for_scene("player_panel", SceneType::ResultOpen3)
        .and_then(|roi| detect_rect_edges(frame, roi));

    let edge_strength_result_open2 = rois
        .get_roi_for_scene("player_panel", SceneType::ResultOpen2)
        .and_then(|roi| detect_rect_edges(frame, roi));

    match (edge_strength_result_open2, edge_strength_result_open3) {
        (Some(strength2), Some(strength3)) => {
            if strength2 >= STRICT_EDGE_THRESHOLD && strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(if strength2 > strength3 {
                    SceneType::ResultOpen2
                } else {
                    SceneType::ResultOpen3
                });
            } else if strength2 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen2);
            } else if strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen3);
            }
        }
        (Some(strength2), None) => {
            if strength2 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen2);
            }
        }
        (None, Some(strength3)) => {
            if strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen3);
            }
        }
        (None, None) => {}
    }

    None
}

fn detect_result_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
    is_unknown: bool,
) -> Option<(SceneType, i32)> {
    // ResultFreestyle, ResultOpen3, ResultOpen2 재킷 ROI는 같은 위치를 공유함
    let jacket_roi = rois.get_roi_for_scene("jacket", SceneType::ResultFreestyle)?;
    let colorbar_roi = rois.get_roi_for_scene("mode_colorbar", SceneType::ResultFreestyle)?;
    let mean = crate::capture::frame_utils::region_mean_bgr(frame, colorbar_roi);

    let is_freestyle_result = detect_freestyle_result_colorbar_match(mean);
    let open_match_scene = check_open_match_badge(frame, rois);
    let gate_ok = is_freestyle_result || open_match_scene.is_some();

    if gate_ok {
        let jacket = jacket_roi.and_then(frame, |jacket_img| Some(jacket_img.to_image_region()))?;

        // 1차 초고속 게이트: Centroid Kernel 사전 검사 (결과창 노이즈로 인한 98ms 스파이크 차단)
        let kernel_ok = matcher.check_centroid_kernel(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        );

        if !kernel_ok {
            if is_unknown {
                debug_println!(
                    "    [scene_gate] Rejected by 1st Centroid Kernel Gate for result scene"
                );
            }
            return None;
        }

        if is_unknown {
            debug_println!(
                "    [telemetry] match_jacket triggered while scene=Unknown, candidate=result"
            );
        }
        // 결과창 재킷 매칭 시도
        let mut song_id = None;
        if let Some(match_res) = matcher.match_jacket(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        ) {
            let threshold = matcher.similarity_threshold();
            if match_res.similarity >= threshold {
                if let Ok(id) = match_res.image_id.parse::<i32>() {
                    song_id = Some(id);
                    debug_println!(
                        "    [detect_result_scene_via_edge] Result screen jacket verified. SongID={}, Similarity={}",
                        id, match_res.similarity
                    );
                }
            }
        }

        // 재킷 매칭이 확실히 성공한 경우에만 결과창 씬으로 반환
        if let Some(id) = song_id {
            if is_freestyle_result
                && detect_rect_edges(frame, colorbar_roi)
                    .map(|edge_strength| edge_strength >= STRICT_EDGE_THRESHOLD)
                    .unwrap_or(false)
            {
                debug_println!("    [detect_result_scene_via_edge] Result screen detected via freestyle colorbar!");
                return Some((SceneType::ResultFreestyle, id));
            }

            if let Some(fallback_scene) = open_match_scene {
                debug_println!("    [detect_result_scene_via_edge] Result screen detected via openmatch badge!");
                return Some((fallback_scene, id));
            }
        }
    }
    None
}

/// 선곡 계열 씬의 공통 자켓 게이트 체인:
/// ROI 크롭 → Centroid Kernel 게이트 → 카테고리 띠 단색성 검사 → 자켓 유사도 매칭.
/// 성공 시 (song_id, similarity), 실패 시 거절 단계가 기록된 SceneMissDiag를 반환한다.
fn run_jacket_match_gate(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
    scene: SceneType,
    is_unknown: bool,
    #[allow(unused_variables)] gate_label: &str,
) -> (Option<(i32, f32)>, SceneMissDiag) {
    let mut diag = SceneMissDiag::default();
    let Some(jacket_roi) = rois.get_roi_for_scene("jacket", scene) else {
        return (None, diag);
    };
    let Some(jacket) = jacket_roi.and_then(frame, |jacket_img| Some(jacket_img.to_image_region()))
    else {
        return (None, diag);
    };

    // 1차 게이트: Centroid Kernel 사전 검사
    if !matcher.check_centroid_kernel(
        &jacket.bgra,
        jacket.width as usize,
        jacket.height as usize,
        4,
    ) {
        if is_unknown {
            debug_println!(
                "    [scene_gate] Rejected by 1st Centroid Kernel Gate for {gate_label}"
            );
        }
        diag.centroid_rejected = true;
        return (None, diag);
    }

    // 2차 게이트: 카테고리 띠 단색성 검사
    if !check_category_band_solid(frame, jacket_roi, rois.scale()) {
        diag.band_rejected = true;
        return (None, diag);
    }

    if is_unknown {
        debug_println!(
            "    [telemetry] match_jacket triggered while scene=Unknown, candidate={gate_label}"
        );
    }

    let Some(match_res) = matcher.match_jacket(
        &jacket.bgra,
        jacket.width as usize,
        jacket.height as usize,
        4,
    ) else {
        return (None, diag);
    };
    let threshold = matcher.similarity_threshold();
    if match_res.similarity < threshold {
        diag.top_similarity = Some(match_res.similarity);
        return (None, diag);
    }
    match match_res.image_id.parse::<i32>() {
        Ok(song_id) => (Some((song_id, match_res.similarity)), diag),
        Err(_) => (None, diag),
    }
}

fn detect_freestyle_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
    is_unknown: bool,
) -> (Option<(SceneType, i32, f32)>, SceneMissDiag) {
    let (res, diag) = run_jacket_match_gate(
        frame,
        rois,
        matcher,
        SceneType::Freestyle,
        is_unknown,
        "freestyle",
    );
    (res.map(|(id, sim)| (SceneType::Freestyle, id, sim)), diag)
}

fn detect_openmatch_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
    is_unknown: bool,
) -> (Option<(SceneType, i32, f32)>, SceneMissDiag) {
    let (res, diag) = run_jacket_match_gate(
        frame,
        rois,
        matcher,
        SceneType::OpenMatch,
        is_unknown,
        "openmatch",
    );
    (res.map(|(id, sim)| (SceneType::OpenMatch, id, sim)), diag)
}

fn parse_static_scene(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
    is_unknown: bool,
) -> (Option<(SceneType, Option<i32>)>, SceneMissDiag) {
    // 1. 결과창 감지 우선 (Native CV)
    if let Some((scene, song_id)) = detect_result_scene_via_edge(frame, rois, matcher, is_unknown) {
        return (Some((scene, Some(song_id))), SceneMissDiag::default());
    }

    // 2. 프리스타일 및 오픈매치 선곡창 자켓 매칭 동시 비교하여 경합 (Native CV)
    let (freestyle_res, fs_diag) =
        detect_freestyle_scene_via_edge(frame, rois, matcher, is_unknown);
    let (openmatch_res, om_diag) =
        detect_openmatch_scene_via_edge(frame, rois, matcher, is_unknown);

    // 실패 원인 진단: 두 후보 중 관측된 최악의 게이트 상태를 병합
    let diag = SceneMissDiag {
        centroid_rejected: fs_diag.centroid_rejected && om_diag.centroid_rejected,
        band_rejected: fs_diag.band_rejected && om_diag.band_rejected,
        top_similarity: match (fs_diag.top_similarity, om_diag.top_similarity) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        },
    };

    let result = match (freestyle_res, openmatch_res) {
        (Some((f_scene, f_id, f_sim)), Some((o_scene, o_id, o_sim))) => {
            // 둘 다 임계치를 넘었을 경우, 유사도(Similarity)가 더 높은 씬을 승자로 채택
            if f_sim >= o_sim {
                debug_println!("    [parse_static_scene] Both matched. Freestyle selected by similarity ({:.4} vs {:.4})", f_sim, o_sim);
                Some((f_scene, Some(f_id)))
            } else {
                debug_println!("    [parse_static_scene] Both matched. OpenMatch selected by similarity ({:.4} vs {:.4})", o_sim, f_sim);
                Some((o_scene, Some(o_id)))
            }
        }
        (Some((f_scene, f_id, _)), None) => Some((f_scene, Some(f_id))),
        (None, Some((o_scene, o_id, _))) => Some((o_scene, Some(o_id))),
        (None, None) => None,
    };
    (result, diag)
}

pub fn detect_static_scene(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> SceneType {
    parse_static_scene(frame, rois, matcher, true)
        .0
        .map(|(scene, _)| scene)
        .unwrap_or(SceneType::Unknown)
}

fn detect_rect_edges(frame: &CapturedFrame, roi: crate::detector::roi::RoiRect) -> Option<f32> {
    let margin = 8;
    roi.with_margin(margin)
        .and_then(frame, |ext_img| ext_img.detect_edges(margin as usize).ok())
}

fn check_category_band_solid(
    frame: &CapturedFrame,
    jacket_roi: crate::detector::roi::RoiRect,
    scale: f32,
) -> bool {
    let width = ((4.0 * scale).round() as i32).max(4);
    let band_roi = crate::detector::roi::RoiRect {
        x1: jacket_roi.x2,
        y1: jacket_roi.y1,
        x2: jacket_roi.x2 + width,
        y2: jacket_roi.y2,
    };

    // 띠 경계선 엣지 검사 우회 (자켓과 띠의 색상이 유사해 엣지가 뭉개지는 케이스 방지)
    // 3단계 통계 기준(최소 밝기 >= 60, 수직 편차 <= 20, 채도/무채색 정합성)으로 띠를 판별

    band_roi
        .and_then(frame, |band_img| {
            let total_pixels = band_img.width * band_img.height ;
            if total_pixels == 0 {
                return Some(false);
            }

            let mut sum_bgr = overmax_cv::Bgr::new(0.0, 0.0, 0.0);

            for y in 0..band_img.height {
                let row = band_img.row(y);
                for pixel in row.chunks_exact(4) {
                    sum_bgr += overmax_cv::Bgr::from_bgra_slice_f64(pixel);
                }
            }

            let mean = sum_bgr / total_pixels as f64;

            // 1. Brightness 가드 (>= 60.0)
            let brightness = mean.luma(overmax_cv::LumaMethod::Weighted);
            if brightness < 60.0 {
                return Some(false);
            }

            // 2. AvgDiff 수직 단색성 가드 (<= 20.0)
            let mut diff_sum = 0.0;
            for y in 0..band_img.height {
                let row = band_img.row(y);
                for pixel in row.chunks_exact(4) {
                    let px = overmax_cv::Bgr::from_bgra_slice_f64(pixel);
                    diff_sum += px.abs_diff(mean).sum_channels();
                }
            }

            let avg_diff = diff_sum / (total_pixels * 3) as f64;
            if avg_diff > 20.0 {
                return Some(false);
            }

            // 3. Saturation 및 무채색(Gray) 채널 균등성 검증
            let max_c = mean.max_channel();
            let min_c = mean.min_channel();
            let saturation = if max_c > 0.0 {
                (max_c - min_c) / max_c
            } else {
                0.0
            };

            if saturation < 0.15
                && mean.max_channel_diff() > 15.0 {
                    debug_println!(
                        "    [check_category_band_solid] Category band rejected: channel diff {:.1} > 15.0",
                        mean.max_channel_diff()
                    );
                    return Some(false);
                }

            debug_println!(
                "    [check_category_band_solid] Category band solid detected! brightness={:.1}, avg_diff={:.2}",
                brightness, avg_diff
            );
            Some(true)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{DetectionPipeline, JacketMatchStatus, SleepHint};
    use crate::capture::frame::CapturedFrame;
    use overmax_data::ImageIndexDb;

    #[test]
    fn scene_poll_miss_flips_to_unknown_and_records_diag() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        pipeline.last_static_scene = SceneType::Freestyle;
        pipeline.hysteresis.is_active = true;
        pipeline.rois.set_scene(SceneType::Freestyle);

        // 파싱 실패 시 Unknown 전환 + 진단 지표(거절 단계) 집계 확인
        let out = pipeline.detect(&frame, 10.0);
        assert_eq!(pipeline.last_static_scene, SceneType::Unknown);
        assert_eq!(out.sleep_hint, SleepHint::Active);

        let snap = pipeline.stats.maybe_take_snapshot(0.0).unwrap();
        assert_eq!(snap.scene_poll_misses, 1);
        assert!(snap.scene_miss_band_rejects > 0 || snap.scene_miss_centroid_rejects > 0);
    }

    #[test]
    fn sleep_hint_is_active_during_unknown_warmup_and_relaxed_after() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        // Unknown 진입 초기(<3s): 고속 폴링 유지로 씬 전환 반응성 확보
        let warmup = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 1.0);
        assert_eq!(warmup.sleep_hint, SleepHint::Active);
        let late_warmup = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 2.9);
        assert_eq!(late_warmup.sleep_hint, SleepHint::Active);

        // 장기 Unknown(>=3s): 완화 주기 전환으로 캡처 비용 절감
        let relaxed = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 4.0);
        assert_eq!(relaxed.sleep_hint, SleepHint::Relaxed);

        // 씬 신호 복귀 시 즉시 Active 복원
        let recovered = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 5.0);
        assert_eq!(recovered.sleep_hint, SleepHint::Active);
    }

    #[test]
    fn stays_detecting_until_hysteresis_activates() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        let first = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
        let second = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 2.0);

        assert!(!first.is_song_select);
        assert_eq!(first.jacket_status, JacketMatchStatus::NotSongSelect);
        assert!(second.is_song_select);
        assert_eq!(second.jacket_status, JacketMatchStatus::DbNotReady);
    }

    #[test]
    fn resets_state_when_song_select_is_lost() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        for idx in 0..2 {
            pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, idx as f64);
        }
        let output = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 10.0);

        assert!(output.is_song_select);
        assert!(output.state.context.is_none());
    }

    #[test]
    fn cached_frames_do_not_repeat_a_scene_miss() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
        pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 2.0);
        let miss = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 3.0);
        assert!(miss.is_song_select);
        assert!(!miss.is_leaving);

        for now in [3.1, 3.2, 3.3, 3.4] {
            let cached = pipeline.process_frame_cached(&frame, now);
            assert!(cached.is_song_select);
            assert!(!cached.is_leaving);
        }
    }

    #[test]
    fn cached_frames_do_not_complete_scene_entry() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        let fresh = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
        pipeline.last_static_scene = SceneType::Freestyle;
        let cached = pipeline.process_frame_cached(&frame, 1.1);
        let second_fresh = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.2);

        assert!(!fresh.is_song_select);
        assert!(!cached.is_song_select);
        assert!(second_fresh.is_song_select);
    }

    #[test]
    fn reset_clears_result_cache_and_roi_scene() {
        use overmax_core::SceneType;

        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        pipeline.rois.set_scene(SceneType::ResultFreestyle);
        pipeline.play_state.seed_detected_cache_for_test();

        pipeline.reset();

        assert_eq!(pipeline.rois.current_scene(), SceneType::Unknown);
        assert!(pipeline.play_state.detected_cache_is_empty_for_test());
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    #[test]
    #[ignore]
    fn test_scratch_images() {
        use crate::capture::frame_utils::crop_roi;
        use image::GenericImageView;
        use overmax_core::SceneType;

        let scratch_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch");
        let images = [
            "hd_test_1.png",
            "hd_test_2.png",
            "hd_test_3.png",
            "hd_test_4.png",
            "hd_test_5.png",
            "hd_test_2p_1.png",
            "hd_test_2p_2.png",
        ];

        let db_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/image_index.db");
        let db_path_str = db_path.to_str().unwrap();

        let roi_dir = scratch_dir.join("roi");
        std::fs::create_dir_all(&roi_dir).unwrap();

        for img_name in &images {
            let path = scratch_dir.join(img_name);
            if !path.exists() {
                println!("{}: Not found", img_name);
                continue;
            }

            // Create a fresh pipeline for each image to isolate OCR checksum bypass caches
            let mut pipeline = DetectionPipeline::new(ImageIndexDb::new(db_path_str, 0.6));
            let _ = pipeline.image_db.load();

            let img = image::ImageReader::open(&path)
                .expect("Failed to open file")
                .with_guessed_format()
                .expect("Failed to guess format")
                .decode()
                .expect("Failed to decode image");
            let (w, h) = img.dimensions();
            let mut bgra = vec![0u8; (w * h * 4) as usize];

            for (x, y, pixel) in img.pixels() {
                let idx = ((y * w + x) * 4) as usize;
                overmax_cv::Bgr::new(pixel[2], pixel[1], pixel[0])
                    .write_to_bgra(&mut bgra[idx..idx + 4], pixel[3]);
            }

            let frame = CapturedFrame {
                width: w as i32,
                height: h as i32,
                bgra,
            };

            pipeline.rois.update_window_size(w as i32, h as i32);
            pipeline.reset_on_screen_exit();

            // 1. Detect final stable scene
            let mut final_scene = SceneType::Unknown;
            for step in 0..10 {
                let t = step as f64 * 0.4;
                let _ = pipeline.detect(&frame, t);
                final_scene = pipeline.last_static_scene;
            }
            println!("IMAGE: {} -> Detected Scene: {:?}", img_name, final_scene);

            // 2. Build ROI name list for current scene
            let mut roi_names = Vec::new();
            if let Some(cfg) = pipeline.rois.config.scenes.get(&final_scene) {
                for name in cfg.rois.keys() {
                    roi_names.push(name.clone());
                }
            }
            if final_scene == SceneType::Freestyle
                || final_scene == SceneType::OpenMatch
                || final_scene == SceneType::LadderMatch
            {
                for diff in overmax_core::Difficulty::ALL {
                    roi_names.push(format!("diff_panel_{}", diff.as_str()));
                }
            }

            // 3. Crop and save each ROI
            for roi_name in roi_names {
                let roi_rect = if roi_name.starts_with("diff_panel_") {
                    let diff_name = roi_name.strip_prefix("diff_panel_").unwrap();
                    overmax_core::Difficulty::from_str(diff_name).and_then(|diff| {
                        pipeline
                            .rois
                            .get_diff_panel_roi_for_scene(diff, final_scene)
                    })
                } else {
                    pipeline.rois.get_roi_for_scene(&roi_name, final_scene)
                };

                let Some(roi) = roi_rect else {
                    continue;
                };

                let Some(cropped) = crop_roi(&frame, roi) else {
                    continue;
                };

                let mut rgba = cropped.to_image_region().bgra;
                for chunk in rgba.chunks_exact_mut(4) {
                    chunk.swap(0, 2); // BGR -> RGB
                }

                let out_filename = format!(
                    "{}_{}.png",
                    img_name.strip_suffix(".png").unwrap_or(img_name),
                    roi_name
                );
                let out_path = roi_dir.join(out_filename);
                image::save_buffer(
                    &out_path,
                    &rgba,
                    cropped.width as u32,
                    cropped.height as u32,
                    image::ColorType::Rgba8,
                )
                .expect("Failed to save cropped image");
                println!("    Saved ROI '{}' to {:?}", roi_name, out_path);
            }
        }
    }
}
