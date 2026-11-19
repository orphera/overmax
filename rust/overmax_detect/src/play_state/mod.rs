use crate::roi::RoiManager;
use crate::templates;
use overmax_core::{
    Changed, Difficulty, GameSessionState, Mode, PatternRecord, PlayContext, RecordKey,
    VerifiedPlayEvent,
};
use overmax_frame::CapturedFrame;
use overmax_frame::{compute_pixel_checksum, region_mean_bgr};
use std::collections::VecDeque;

pub const MIN_VALID_RATE: f32 = 80.0;

const BTN_MODE_MAX_DIST: f32 = 60.0;
const DIFF_MIN_BRIGHTNESS: f32 = 45.0;
const DIFF_CONFIDENT_MARGIN: f32 = 15.0;
const RATE_DETECTION_INTERVAL_SEC: f64 = 0.20;
const RATE_CHECKSUM_CHANGE_THRESHOLD: u64 = 50;

#[derive(Clone, Copy, Debug, Eq)]
struct RateInputChecksums {
    score: u64,
    badge: u64,
}

impl PartialEq for RateInputChecksums {
    fn eq(&self, other: &Self) -> bool {
        self.score.abs_diff(other.score) <= RATE_CHECKSUM_CHANGE_THRESHOLD
            && self.badge.abs_diff(other.badge) <= RATE_CHECKSUM_CHANGE_THRESHOLD
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModeDiffChecksums {
    mode: u64,
    diff: u64,
}

/// 화면 ROI 단위의 관측값과 체크섬을 묶어 관리하는 단일 캐시 단위
#[derive(Clone, Debug, PartialEq)]
pub struct RoiCache<Key, Checksum, Value> {
    current_key: Option<Key>,
    last_checksum: Option<Checksum>,
    last_value: Option<Value>,
    last_detect_ts: f64,
    interval_sec: f64,
}

impl<Key: PartialEq + Clone, Checksum: PartialEq + Copy, Value: Clone>
    RoiCache<Key, Checksum, Value>
{
    pub fn new(interval_sec: f64) -> Self {
        Self {
            current_key: None,
            last_checksum: None,
            last_value: None,
            last_detect_ts: 0.0,
            interval_sec,
        }
    }

    /// 현재 키(RecordKey 등)가 이전과 다르면 캐시를 즉시 비움 (잔류 원천 차단)
    pub fn sync_key(&mut self, key: Option<Key>) {
        if self.current_key != key {
            self.current_key = key;
            self.last_value = None;
            self.last_checksum = None;
        }
    }

    /// 체크섬이 변경되었거나 시간 쿨다운이 되었을 때만 계산 수행
    pub fn get_or_detect<F>(
        &mut self,
        checksum: Option<Checksum>,
        now: f64,
        detect_fn: F,
    ) -> Option<Value>
    where
        F: FnOnce() -> Option<Value>,
    {
        let checksum_changed = checksum.is_none() || checksum != self.last_checksum;
        let time_due = now - self.last_detect_ts >= self.interval_sec;

        if (checksum_changed || self.last_value.is_none()) && time_due {
            self.last_value = detect_fn();
            self.last_checksum = checksum;
            self.last_detect_ts = now;
        }

        self.last_value.clone()
    }

    /// 현재 캐시된 값 조회
    pub fn get(&self) -> Option<&Value> {
        self.last_value.as_ref()
    }

    /// 캐시 수동 설정
    pub fn set(&mut self, value: Option<Value>, checksum: Option<Checksum>, now: f64) {
        self.last_value = value;
        self.last_checksum = checksum;
        self.last_detect_ts = now;
    }

    /// 캐시 완전 초기화
    pub fn clear(&mut self) {
        self.current_key = None;
        self.last_checksum = None;
        self.last_value = None;
        self.last_detect_ts = 0.0;
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    context: Option<PlayContext>,
}

/// 결과창 애니메이션/노이즈 보정용 mode·diff 래치 (Sample-and-Hold Latch).
#[derive(Default)]
struct ResultModeDiffLatch {
    mode: Changed<Option<Mode>>,
    diff: Changed<Option<Difficulty>>,
}

impl ResultModeDiffLatch {
    fn new() -> Self {
        Self::default()
    }

    /// 결과창 템플릿 매칭 결과가 Some일 때 래치 업데이트 (자가 보정)
    fn update_if_some(&mut self, detected_mode: Option<Mode>, detected_diff: Option<Difficulty>) {
        if detected_mode.is_some() {
            self.mode.update(detected_mode);
        }
        if detected_diff.is_some() {
            self.diff.update(detected_diff);
        }
    }

    /// 현재 래치된 모드/난이도 반환
    fn get(&self) -> (Option<Mode>, Option<Difficulty>) {
        (*self.mode.get(), *self.diff.get())
    }

    /// 결과창 -> 선곡창 복귀 시 래치 해제
    fn clear(&mut self) {
        self.mode.update(None);
        self.diff.update(None);
    }
}

pub struct PlayStateDetector {
    history_size: usize,
    history: VecDeque<Option<RawPlayState>>,
    mode_diff_cache: RoiCache<(), ModeDiffChecksums, (Option<Mode>, Option<Difficulty>, bool)>,
    rate_cache: RoiCache<RecordKey, RateInputChecksums, PatternRecord>,
    result_mode_diff: ResultModeDiffLatch,
    result_rate_window: VecDeque<f32>,
    last_emitted_event: Option<VerifiedPlayEvent>,
    /// 직전 프레임이 결과창이었는지 (결과창 → 선곡 복귀 시점 1회 감지용)
    was_result: bool,
    /// 이번 프레임의 선곡 화면 rate 판독 실패 여부 (안정화 이력 제외용)
    rate_unread: bool,
}

impl PlayStateDetector {
    pub fn stable_hits(&self) -> u32 {
        self.history.iter().filter(|h| h.is_some()).count() as u32
    }

    fn rate_input_checksums(
        frame: &CapturedFrame,
        rois: &RoiManager,
    ) -> Option<RateInputChecksums> {
        let score = rois
            .get_roi("score")
            .and_then(|roi| compute_pixel_checksum(frame, roi))
            .or_else(|| {
                rois.get_roi("rate")
                    .and_then(|roi| compute_pixel_checksum(frame, roi))
            })?;
        let badge = rois
            .get_roi("max_combo_badge")
            .and_then(|roi| compute_pixel_checksum(frame, roi))
            .unwrap_or(0);
        Some(RateInputChecksums { score, badge })
    }

    fn process_rate_detection(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        scene: overmax_core::SceneType,
        is_result: bool,
        now: f64,
    ) -> Option<PatternRecord> {
        let checksums = Self::rate_input_checksums(frame, rois);
        let is_song_select = matches!(
            scene,
            overmax_core::SceneType::Freestyle | overmax_core::SceneType::OpenMatch
        );

        if is_result {
            if now - self.rate_cache.last_detect_ts >= RATE_DETECTION_INTERVAL_SEC {
                let inputs_changed =
                    checksums.is_none() || checksums != self.rate_cache.last_checksum;

                if inputs_changed {
                    let detected_score = rois.and_then_roi(frame, "score", templates::detect_score);
                    let direct_rate =
                        rois.and_then_roi(frame, "rate", |img| templates::detect_rate(img));

                    let detected_rate = match (detected_score, direct_rate) {
                        (Some(score_val), Some(rate_val)) => {
                            let calc_rate = (score_val as f32 / 10000.0 * 100.0).floor() / 100.0;
                            if (calc_rate - rate_val).abs() <= 0.02 {
                                Some(calc_rate)
                            } else {
                                debug_println!(
                                    "    [detect result] score/rate conflict: score_val={} (calc={:.2}%) vs rate={:.2}%. Prioritizing direct rate.",
                                    score_val, calc_rate, rate_val
                                );
                                Some(rate_val)
                            }
                        }
                        (Some(score_val), None) => {
                            let calc_rate = (score_val as f32 / 10000.0 * 100.0).floor() / 100.0;
                            (0.0..=100.0).contains(&calc_rate).then_some(calc_rate)
                        }
                        (None, Some(rate_val)) => Some(rate_val),
                        (None, None) => None,
                    };

                    if let Some(r) = detected_rate {
                        self.push_result_rate_sample(r);
                        if let Some(median_r) = self.median_result_rate() {
                            let is_max_combo = detect_max_combo_result(frame, rois);
                            let record = PatternRecord::Played {
                                rate: median_r,
                                is_max_combo,
                            };
                            self.rate_cache.set(Some(record), checksums, now);
                        }
                    }
                }
            }
            Some(
                self.rate_cache
                    .get()
                    .copied()
                    .unwrap_or(PatternRecord::Unplayed),
            )
        } else {
            self.result_rate_window.clear();
            // 반환 None = 읽기 실패. 캐시에 남지 않아 다음 주기에 다시 판독하며,
            // 호출부는 이 프레임을 안정화 이력에서 제외한다 (기록 삭제 이벤트 방지).
            self.rate_cache.get_or_detect(checksums, now, || {
                let detected_score = rois.and_then_roi(frame, "score", templates::detect_score);
                let rate_reading = rois
                    .get_roi("rate")
                    .and_then(|roi| roi.and_then(frame, |img| Some(templates::read_rate(img))))
                    .unwrap_or(templates::RateReading::Unreadable);
                let direct_rate = match rate_reading {
                    templates::RateReading::Valid(value) => Some(value),
                    _ => None,
                };

                let final_rate = match (detected_score, direct_rate) {
                    (Some(score_val), Some(rate_val)) => {
                        let calc_rate = (score_val as f32 / 10000.0 * 100.0).floor() / 100.0;
                        if (calc_rate - rate_val).abs() <= 0.02 {
                            Some(calc_rate)
                        } else {
                            debug_println!(
                                "    [detect] score/rate conflict: score_val={} (calc={:.2}%) vs rate={:.2}%. Prioritizing direct rate.",
                                score_val, calc_rate, rate_val
                            );
                            Some(rate_val)
                        }
                    }
                    (Some(score_val), None) => {
                        let calc_rate = (score_val as f32 / 10000.0 * 100.0).floor() / 100.0;
                        (MIN_VALID_RATE..=100.0).contains(&calc_rate).then_some(calc_rate)
                    }
                    (None, Some(rate_val)) => Some(rate_val),
                    (None, None) => None,
                };

                if let Some(rate) = final_rate {
                    if is_song_select && (MIN_VALID_RATE..=100.0).contains(&rate) {
                        debug_println!(
                            "    [detect] rate detected. rate={:.2}%",
                            rate
                        );
                        let is_max_combo = detect_max_combo(frame, rois);
                        return Some(PatternRecord::Played {
                            rate,
                            is_max_combo,
                        });
                    }
                }
                // 선곡 화면은 기록 없는 패턴을 0.00%로 표시한다. 그 표시를 직접 읽었을 때만
                // 미플레이로 확정하고, 그 외(읽기 실패·80% 미만 등)는 판단을 보류한다.
                if !is_song_select || rate_reading == templates::RateReading::Zero {
                    Some(PatternRecord::Unplayed)
                } else {
                    None
                }
            })
        }
    }

    fn push_result_rate_sample(&mut self, r: f32) {
        self.result_rate_window.push_back(r);
        if self.result_rate_window.len() > 7 {
            self.result_rate_window.pop_front();
        }
    }

    fn median_result_rate(&self) -> Option<f32> {
        let mut sorted: Vec<f32> = self.result_rate_window.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted.get(sorted.len() / 2).copied()
    }

    fn mode_diff_checksums(frame: &CapturedFrame, rois: &RoiManager) -> Option<ModeDiffChecksums> {
        let mode = rois
            .get_roi("btn_mode")
            .and_then(|roi| compute_pixel_checksum(frame, roi))?;

        let mut diff: u64 = 0;
        for d in Difficulty::ALL {
            if let Some(roi) = rois.get_diff_panel_roi(d) {
                if let Some(cs) = compute_pixel_checksum(frame, roi) {
                    diff = diff.wrapping_add(cs);
                }
            }
        }
        Some(ModeDiffChecksums { mode, diff })
    }

    fn process_mode_diff_detection(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        now: f64,
    ) -> (Option<Mode>, Option<Difficulty>, bool) {
        let checksums = Self::mode_diff_checksums(frame, rois);
        let result = self.mode_diff_cache.get_or_detect(checksums, now, || {
            let m = detect_button_mode(frame, rois);
            let (d, conf) = detect_difficulty(frame, rois);
            Some((m, d, conf))
        });
        result.unwrap_or((None, None, false))
    }

    pub fn new(history_size: usize) -> Self {
        Self {
            history_size: history_size.max(1),
            history: VecDeque::new(),
            mode_diff_cache: RoiCache::new(0.0),
            rate_cache: RoiCache::new(RATE_DETECTION_INTERVAL_SEC),
            result_mode_diff: ResultModeDiffLatch::new(),
            result_rate_window: VecDeque::new(),
            last_emitted_event: None,
            was_result: false,
            rate_unread: false,
        }
    }

    pub fn reset(&mut self) {
        self.clear_observations();
        // 결과창 진입 시 복구용 래치(result_mode_diff)는 reset 시에도 보존합니다.
        self.last_emitted_event = None;
    }

    /// A gap in observations is not evidence of another completed play.
    /// Retain only event deduplication, never visual evidence or a stable context.
    pub fn suspend(&mut self) {
        self.clear_observations();
        self.result_mode_diff.clear();
        self.last_emitted_event = self
            .last_emitted_event
            .filter(|event| event.is_result_screen);
    }

    fn clear_observations(&mut self) {
        self.history.clear();
        self.mode_diff_cache.clear();
        self.rate_cache.clear();
        self.result_rate_window.clear();
    }

    pub fn clear_detected_cache(&mut self) {
        self.result_mode_diff.clear();
        self.last_emitted_event = None;
        self.rate_cache.clear();
        self.result_rate_window.clear();
    }

    #[cfg(test)]
    pub(crate) fn seed_detected_cache_for_test(&mut self) {
        self.result_mode_diff.mode.update(Some(Mode::B4));
        self.result_mode_diff.diff.update(Some(Difficulty::NM));
    }

    /// Inject successful CV observations; history and event emission still run normally.
    #[cfg(test)]
    pub(crate) fn seed_result_observation_for_test(&mut self, rate: f32, is_max_combo: bool) {
        self.seed_detected_cache_for_test();
        self.rate_cache.set(
            Some(PatternRecord::Played { rate, is_max_combo }),
            None,
            0.0,
        );
    }

    #[cfg(test)]
    pub(crate) fn detected_cache_is_empty_for_test(&self) -> bool {
        self.result_mode_diff.mode.get().is_none() && self.result_mode_diff.diff.get().is_none()
    }

    fn resolve_result_mode_diff(
        &mut self,
        scene: overmax_core::SceneType,
        frame: &CapturedFrame,
        rois: &RoiManager,
    ) -> (Option<Mode>, Option<Difficulty>) {
        let (detected_mode, detected_diff) = match scene {
            overmax_core::SceneType::ResultFreestyle => (
                rois.and_then_roi(frame, "mode_digit", templates::detect_freestyle_mode),
                rois.and_then_roi(frame, "diff_panel", templates::detect_result_difficulty),
            ),
            overmax_core::SceneType::ResultOpen3 | overmax_core::SceneType::ResultOpen2 => (
                detect_button_mode_from_roi(frame, rois, "openmatch_mode"),
                rois.and_then_roi(
                    frame,
                    "openmatch_diff",
                    templates::detect_openmatch_result_difficulty,
                ),
            ),
            _ => (None, None),
        };

        // 결과창 실시간 템플릿 매칭 성공 시 래치 업데이트 (자가 보정)
        self.result_mode_diff
            .update_if_some(detected_mode, detected_diff);

        // 래치된 최종값 반환
        self.result_mode_diff.get()
    }

    /// [메인 진입점] 프레임 단위 감지 파이프라인
    pub fn detect(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<i32>,
        now: f64,
    ) -> (GameSessionState, Option<VerifiedPlayEvent>) {
        let scene = rois.current_scene();

        // 결과창 → 선곡 복귀 전환 시점에만 결과창 캐시/이벤트 래치를 비운다.
        // (선곡 화면 프레임마다 비우면 이벤트가 매 프레임 재방출되고 rate 캐시가 무력화된다)
        if self.was_result && !scene.is_result() {
            self.clear_detected_cache();
        }
        self.was_result = scene.is_result();
        self.rate_unread = false;

        // 1. 현재 프레임의 플레이 컨텍스트 관측 (모드, 난이도, 점수)
        let current_context = self.observe_current_context(frame, rois, song_id, scene, now);

        // 2. 5프레임 연속 일치(Hysteresis) 이력 갱신 및 안정화 상태 판독
        // rate를 읽지 못한 프레임은 표시용으로만 쓰고 안정화 이력에는 넣지 않는다
        let history_context = if self.rate_unread {
            None
        } else {
            current_context.clone()
        };
        let stable_context = self.commit_history_and_get_stable(history_context);

        // 3. 최종 게임 세션 상태 및 결과창 1회 도메인 이벤트 생성
        self.build_session_state_and_event(scene, current_context, stable_context)
    }

    fn observe_current_context(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<i32>,
        scene: overmax_core::SceneType,
        now: f64,
    ) -> Option<PlayContext> {
        let is_result = scene.is_result();

        // 1-1. 모드 / 난이도 판독
        let (mode, diff, confident) = if is_result {
            let (m, d) = self.resolve_result_mode_diff(scene, frame, rois);
            (m, d, true)
        } else {
            self.result_mode_diff.clear();
            let (m, d, conf) = self.process_mode_diff_detection(frame, rois, now);
            (m, d, conf)
        };

        let sid = song_id?;
        let m = mode?;
        let d = diff?;
        if !confident {
            return None;
        }

        let pattern_key = (sid, m, d);
        if !is_result {
            self.rate_cache.sync_key(Some(pattern_key));
        }

        debug_println!(
            "    [detect] song_id={:?}, mode={:?}, diff={:?}, confident={}",
            Some(sid),
            Some(m),
            Some(d),
            confident
        );

        // 1-2. 점수 / Rate / MAX COMBO 판독
        let record = self.process_rate_detection(frame, rois, scene, is_result, now);
        self.rate_unread = record.is_none();
        let record = record.unwrap_or(PatternRecord::Unplayed);
        let rate = record.rate();
        let rate_valid = !is_result || rate >= MIN_VALID_RATE;

        Some(PlayContext {
            song_id: sid,
            mode: m,
            diff: d,
            rate: if rate_valid { rate } else { 0.0 },
            is_max_combo: if rate_valid && rate > 0.0 {
                record.is_max_combo()
            } else {
                false
            },
        })
    }

    fn commit_history_and_get_stable(
        &mut self,
        current_context: Option<PlayContext>,
    ) -> Option<PlayContext> {
        self.push_raw(RawPlayState {
            context: current_context,
        });
        self.stable_raw().and_then(|s| s.context.clone())
    }

    fn build_session_state_and_event(
        &mut self,
        scene: overmax_core::SceneType,
        current_context: Option<PlayContext>,
        stable_context: Option<PlayContext>,
    ) -> (GameSessionState, Option<VerifiedPlayEvent>) {
        if let Some(stable_ctx) = stable_context {
            let is_result = scene.is_result();
            let state = GameSessionState {
                scene,
                context: Some(stable_ctx.clone()),
                is_stable: true,
                is_fullscreen: false, // will be overwritten/updated by detection worker
            };

            let mut verified_event = None;
            let should_emit = if is_result {
                stable_ctx.rate >= MIN_VALID_RATE
            } else {
                stable_ctx.rate >= MIN_VALID_RATE || stable_ctx.rate == 0.0
            };

            if should_emit {
                let current_event = VerifiedPlayEvent {
                    song_id: stable_ctx.song_id,
                    mode: stable_ctx.mode,
                    diff: stable_ctx.diff,
                    rate: stable_ctx.rate,
                    is_max_combo: stable_ctx.is_max_combo,
                    is_result_screen: is_result,
                };

                let should_fire = match &self.last_emitted_event {
                    None => true,
                    Some(last) => {
                        let is_same_pattern = last.song_id == current_event.song_id
                            && last.mode == current_event.mode
                            && last.diff == current_event.diff
                            && last.is_result_screen == current_event.is_result_screen;

                        if !is_same_pattern {
                            true
                        } else if is_result {
                            // 결과창에서는 기록 개선(MAX COMBO 추가 달성 또는 더 높은 Rate) 시 재방출 허용
                            let mc_improved = !last.is_max_combo && current_event.is_max_combo;
                            let rate_improved = current_event.rate > last.rate + 0.001;
                            mc_improved || rate_improved
                        } else {
                            // 선곡창에서는 미플레이 교정이나 점수/맥스콤보 변경 시 재방출 허용
                            (last.rate != current_event.rate)
                                || (last.is_max_combo != current_event.is_max_combo)
                        }
                    }
                };

                if should_fire {
                    self.last_emitted_event = Some(current_event);
                    verified_event = Some(current_event);
                }
            }

            (state, verified_event)
        } else {
            (
                GameSessionState {
                    scene,
                    context: current_context,
                    is_stable: false,
                    is_fullscreen: false,
                },
                None,
            )
        }
    }

    fn push_raw(&mut self, raw: RawPlayState) {
        if self.history.len() == self.history_size {
            self.history.pop_front();
        }
        self.history.push_back(raw.context.is_some().then_some(raw));
    }

    fn stable_raw(&self) -> Option<&RawPlayState> {
        if self.history.len() != self.history_size {
            return None;
        }
        let first = self.history.front()?.as_ref()?;
        self.history
            .iter()
            .all(|item| item.as_ref() == Some(first))
            .then_some(first)
    }
}

pub fn detect_button_mode_from_roi(
    frame: &CapturedFrame,
    rois: &RoiManager,
    roi_name: &str,
) -> Option<Mode> {
    let roi = rois.get_roi(roi_name)?;
    let mean = region_mean_bgr(frame, roi);
    let mut best = (None, f32::INFINITY);

    let colors_table = if roi_name == "openmatch_mode" {
        &templates::OPENMATCH_MODE_COLORS
    } else {
        &templates::FREESTYLE_MODE_COLORS
    };

    for (idx, &color) in colors_table.iter().enumerate() {
        let dist = mean.distance_f32(color);
        if dist < best.1 {
            best = (Some(Mode::ALL[idx]), dist);
        }
    }
    (best.1 <= BTN_MODE_MAX_DIST).then_some(best.0).flatten()
}

pub fn detect_button_mode(frame: &CapturedFrame, rois: &RoiManager) -> Option<Mode> {
    detect_button_mode_from_roi(frame, rois, "btn_mode")
}

pub fn detect_difficulty(frame: &CapturedFrame, rois: &RoiManager) -> (Option<Difficulty>, bool) {
    let mut max_bright = 0.0f32;
    let mut second_bright = 0.0f32;
    let mut best_diff = None;

    for &diff in Difficulty::ALL.iter() {
        let Some(roi) = rois.get_diff_panel_roi(diff) else {
            continue;
        };
        let bright = region_mean_bgr(frame, roi).to_f64().average() as f32;

        if bright > max_bright {
            second_bright = max_bright;
            max_bright = bright;
            best_diff = Some(diff);
        } else if bright > second_bright {
            second_bright = bright;
        }
    }

    let Some(best) = best_diff else {
        return (None, false);
    };

    if max_bright < DIFF_MIN_BRIGHTNESS {
        return (None, false);
    }

    (
        Some(best),
        max_bright - second_bright >= DIFF_CONFIDENT_MARGIN,
    )
}

// 선곡창 Perfect Play (100.0%) 뱃지 대표 해시
const TEMPLATE_SELECT_PERFECT_PHASH: u64 = 0xdca6ef1001714f9e;
const TEMPLATE_SELECT_PERFECT_DHASH: u64 = 0xe4a5a484b4551545;
const TEMPLATE_SELECT_PERFECT_AHASH: u64 = 0x3ffdf4600cdcdcb8;

// 선곡창 Max Combo 뱃지 대표 해시
const TEMPLATE_SELECT_MC_PHASH: u64 = 0xc25a6a8e372b67c8;
const TEMPLATE_SELECT_MC_DHASH: u64 = 0x4909a11e9266a98f;
const TEMPLATE_SELECT_MC_AHASH: u64 = 0x15f4f0073effff03;

// 결과창 Perfect Play (100.0%) 뱃지 대표 해시
const TEMPLATE_RESULT_PERFECT_PHASH: u64 = 0xdea7c998117c851e;
const TEMPLATE_RESULT_PERFECT_DHASH: u64 = 0xd455544439b5b5a5;
const TEMPLATE_RESULT_PERFECT_AHASH: u64 = 0x3fbdf4e014ddd450;

// 결과창 Max Combo 뱃지 대표 해시
const TEMPLATE_RESULT_MC_PHASH: u64 = 0xda5a52d2123b2fe8;
const TEMPLATE_RESULT_MC_DHASH: u64 = 0x2929137dd4ef210f;
const TEMPLATE_RESULT_MC_AHASH: u64 = 0xd4fce007fffffc00;

fn calculate_hash_score(
    phash: u64,
    dhash: u64,
    ahash: u64,
    t_phash: u64,
    t_dhash: u64,
    t_ahash: u64,
) -> f32 {
    let p_dist = (phash ^ t_phash).count_ones() as f32;
    let d_dist = (dhash ^ t_dhash).count_ones() as f32;
    let a_dist = (ahash ^ t_ahash).count_ones() as f32;
    0.5 * p_dist + 0.3 * d_dist + 0.2 * a_dist
}

// 뱃지 해시 매칭 허용 임계치 (빈 배경 평균 거리는 29~35이며, 1px 보간 오차 시 12~14점이 발생하므로 20.0으로 통일)
const BADGE_MATCH_THRESHOLD: f32 = 20.0;

pub fn detect_max_combo(frame: &CapturedFrame, rois: &RoiManager) -> bool {
    let hashes = rois.and_then_roi(frame, "max_combo_badge", |img| img.compute_hashes(4).ok());

    let Some((phash, dhash, ahash)) = hashes else {
        return false;
    };
    let score_perfect = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_SELECT_PERFECT_PHASH,
        TEMPLATE_SELECT_PERFECT_DHASH,
        TEMPLATE_SELECT_PERFECT_AHASH,
    );
    let score_mc = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_SELECT_MC_PHASH,
        TEMPLATE_SELECT_MC_DHASH,
        TEMPLATE_SELECT_MC_AHASH,
    );
    score_perfect <= BADGE_MATCH_THRESHOLD || score_mc <= BADGE_MATCH_THRESHOLD
}

pub fn detect_max_combo_result(frame: &CapturedFrame, rois: &RoiManager) -> bool {
    let hashes = rois.and_then_roi(frame, "max_combo_badge", |img| img.compute_hashes(4).ok());

    let Some((phash, dhash, ahash)) = hashes else {
        return false;
    };
    let score_perfect = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_RESULT_PERFECT_PHASH,
        TEMPLATE_RESULT_PERFECT_DHASH,
        TEMPLATE_RESULT_PERFECT_AHASH,
    );
    let score_mc = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_RESULT_MC_PHASH,
        TEMPLATE_RESULT_MC_DHASH,
        TEMPLATE_RESULT_MC_AHASH,
    );
    score_perfect <= BADGE_MATCH_THRESHOLD || score_mc <= BADGE_MATCH_THRESHOLD
}

/// 선곡 화면(1080p)의 rate 영역에 실제 캡처의 `0.00%`(기록 없음) 표시를 붙인다.
#[cfg(test)]
pub(crate) fn paint_song_select_rate_zero(frame: &mut CapturedFrame) {
    const PNG: &[u8] = include_bytes!("../test_fixtures/song_select_rate_zero.png");
    const X0: i32 = 172;
    const Y0: i32 = 583;
    let img = image::load_from_memory(PNG)
        .expect("rate fixture decodes")
        .to_rgb8();
    for (x, y, px) in img.enumerate_pixels() {
        let idx = (((Y0 + y as i32) * frame.width + X0 + x as i32) * 4) as usize;
        frame.bgra[idx..idx + 4].copy_from_slice(&[px[2], px[1], px[0], 255]);
    }
}

#[cfg(test)]
mod tests;
