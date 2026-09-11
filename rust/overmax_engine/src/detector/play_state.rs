use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{compute_pixel_checksum, region_mean_bgr};
use crate::detector::roi::RoiManager;
use crate::detector::templates;
use overmax_core::{
    Changed, Difficulty, GameSessionState, Mode, PatternRecord, PlayContext, RecordKey,
    VerifiedPlayEvent,
};
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
    ) -> PatternRecord {
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
            self.rate_cache
                .get()
                .copied()
                .unwrap_or(PatternRecord::Unplayed)
        } else {
            self.result_rate_window.clear();
            let record = self.rate_cache.get_or_detect(checksums, now, || {
                let detected_score = rois.and_then_roi(frame, "score", templates::detect_score);
                let direct_rate =
                    rois.and_then_roi(frame, "rate", |img| templates::detect_rate(img));

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
                Some(PatternRecord::Unplayed)
            });

            record.unwrap_or(PatternRecord::Unplayed)
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
        }
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.mode_diff_cache.clear();
        self.rate_cache.clear();
        // 결과창 진입 시 복구용 래치(result_mode_diff)는 reset 시에도 보존합니다.
        self.result_rate_window.clear();
        self.last_emitted_event = None;
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

        // 1. 현재 프레임의 플레이 컨텍스트 관측 (모드, 난이도, 점수)
        let current_context = self.observe_current_context(frame, rois, song_id, scene, now);

        // 2. 5프레임 연속 일치(Hysteresis) 이력 갱신 및 안정화 상태 판독
        let stable_context = self.commit_history_and_get_stable(current_context.clone());

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

#[cfg(test)]
mod tests {
    use super::{detect_button_mode, PlayStateDetector, RateInputChecksums};
    use crate::capture::frame::CapturedFrame;
    use crate::detector::roi::RoiManager;
    use overmax_core::{PatternRecord, SceneType};
    use overmax_cv::Bgr;

    #[test]
    fn detects_button_mode_from_reference_color() {
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);
        assert_eq!(detect_button_mode(&frame, &rois), Some(super::Mode::B4));
    }

    #[test]
    fn marks_state_stable_after_repeated_valid_frames() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // #DCDCDC
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        assert!(!detector.detect(&frame, &rois, Some(7), 1.0).0.is_stable);
        assert!(!detector.detect(&frame, &rois, Some(7), 2.0).0.is_stable);
        assert!(detector.detect(&frame, &rois, Some(7), 3.0).0.is_stable);
    }

    #[test]
    fn result_mode_diff_remains_none_without_match() {
        let mut detector = PlayStateDetector::new(3);
        detector.result_mode_diff.clear();

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);

        // 결과창에서 mode_digit/diff_panel ROI가 없어 인식에 실패하면 None이어야 함
        let (state, event) = detector.detect(&frame, &rois, Some(7), 1.0);
        assert!(state.context.is_none());
        assert!(event.is_none());
    }

    #[test]
    fn mode_diff_checksum_cache_only_refreshes_on_input_change() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NORMAL diff
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(detector.mode_diff_cache.last_detect_ts, 1.0);
        assert!(detector.mode_diff_cache.last_checksum.is_some());

        // 동일 프레임에서 시간만 지나도 체크섬이 안 바뀌면 timestamp가 1.0으로 유지됨 (재계산 스킵)
        let (state2, _) = detector.detect(&frame, &rois, Some(1), 2.0);
        assert_eq!(detector.mode_diff_cache.last_detect_ts, 1.0);
        assert_eq!(
            state1.context.as_ref().map(|c| c.mode),
            state2.context.as_ref().map(|c| c.mode)
        );

        // 난이도 카드 하이라이트 변경 -> checksum 변경으로 3.0s 시점에 갱신됨
        paint_rect(&mut frame, 218, 488, 328, 516, Bgr::from_rgb_hex(0xFFFFFF));
        let (_state3, _) = detector.detect(&frame, &rois, Some(1), 3.0);
        assert_eq!(detector.mode_diff_cache.last_detect_ts, 3.0);
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    fn paint_rect(frame: &mut CapturedFrame, x1: i32, y1: i32, x2: i32, y2: i32, bgr: Bgr) {
        for y in y1..y2 {
            for x in x1..x2 {
                let idx = ((y * frame.width + x) * 4) as usize;
                bgr.write_to_bgra(&mut frame.bgra[idx..idx + 4], 255);
            }
        }
    }

    #[test]
    fn rate_checksum_cache_only_refreshes_on_meaningful_input_change() {
        let previous = RateInputChecksums {
            score: 2_000,
            badge: 500,
        };

        assert_eq!(
            previous,
            RateInputChecksums {
                score: 2_049,
                badge: 549,
            }
        );
        assert_ne!(
            previous,
            RateInputChecksums {
                score: 2_051,
                badge: 500,
            }
        );
        assert_ne!(
            previous,
            RateInputChecksums {
                score: 2_000,
                badge: 551,
            }
        );
    }

    #[test]
    fn test_verified_play_event_emits_once_per_result_session() {
        let mut detector = PlayStateDetector::new(3);
        detector.result_mode_diff.mode.update(Some(super::Mode::B4));
        detector
            .result_mode_diff
            .diff
            .update(Some(super::Difficulty::NM));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 99.5,
                is_max_combo: false,
            }),
            None,
            0.0,
        );

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);

        // Frame 1: Detecting (History 1/3) -> Event None
        let (state1, event1) = detector.detect(&frame, &rois, Some(10), 1.0);
        assert!(!state1.is_stable);
        assert!(event1.is_none());

        // Frame 2: Detecting (History 2/3) -> Event None
        let (state2, event2) = detector.detect(&frame, &rois, Some(10), 1.1);
        assert!(!state2.is_stable);
        assert!(event2.is_none());

        // Frame 3: Stable 확정 (History 3/3) -> Event 단 1회 방출!
        let (state3, event3) = detector.detect(&frame, &rois, Some(10), 1.2);
        assert!(state3.is_stable);
        assert!(event3.is_some());
        let event = event3.unwrap();
        assert_eq!(event.song_id, 10);
        assert_eq!(event.mode, super::Mode::B4);
        assert_eq!(event.diff, super::Difficulty::NM);
        assert_eq!(event.rate, 99.5);
        assert!(event.is_result_screen);

        // Frame 4: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
        let (state4, event4) = detector.detect(&frame, &rois, Some(10), 1.3);
        assert!(state4.is_stable);
        assert!(
            event4.is_none(),
            "결과창 체류 중 중복 이벤트가 방출되면 안 됨"
        );

        // Frame 5: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
        let (state5, event5) = detector.detect(&frame, &rois, Some(10), 1.4);
        assert!(state5.is_stable);
        assert!(event5.is_none());

        // 선곡 화면으로 복귀 -> 래치 자동 리셋
        rois.set_scene(SceneType::Freestyle);
        detector.clear_detected_cache();
        let (state_select, event_select) = detector.detect(&frame, &rois, Some(10), 2.0);
        assert!(event_select.is_none());
        assert!(!state_select.scene.is_result());

        // 다음 곡 플레이 후 새 결과창 진입
        rois.set_scene(SceneType::ResultFreestyle);
        detector.result_mode_diff.mode.update(Some(super::Mode::B6));
        detector
            .result_mode_diff
            .diff
            .update(Some(super::Difficulty::HD));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 98.0,
                is_max_combo: false,
            }),
            None,
            2.0,
        );

        let _ = detector.detect(&frame, &rois, Some(20), 3.0);
        let _ = detector.detect(&frame, &rois, Some(20), 3.1);
        let (state_new_res, event_new_res) = detector.detect(&frame, &rois, Some(20), 3.2);

        assert!(state_new_res.is_stable);
        assert!(
            event_new_res.is_some(),
            "새 결과창 진입 시 래치가 리셋되어 새 이벤트가 방출되어야 함"
        );
        let new_event = event_new_res.unwrap();
        assert_eq!(new_event.song_id, 20);
        assert_eq!(new_event.mode, super::Mode::B6);
        assert_eq!(new_event.diff, super::Difficulty::HD);
        assert_eq!(new_event.rate, 98.0);
    }

    #[test]
    fn test_verified_play_event_re_emits_when_max_combo_or_rate_improves() {
        let mut detector = PlayStateDetector::new(1);
        detector.result_mode_diff.mode.update(Some(super::Mode::B4));
        detector
            .result_mode_diff
            .diff
            .update(Some(super::Difficulty::NM));

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);

        // 1. 점수 롤링 중 95.0% 감지 (MC=false)
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 95.0,
                is_max_combo: false,
            }),
            None,
            0.0,
        );
        let (state1, event1) = detector.detect(&frame, &rois, Some(10), 1.0);
        assert!(state1.is_stable);
        assert!(event1.is_some());
        let e1 = event1.unwrap();
        assert_eq!(e1.rate, 95.0);
        assert!(!e1.is_max_combo);

        // 2. 동일 상태 유지 -> 이벤트 미방출
        let (_, event2) = detector.detect(&frame, &rois, Some(10), 1.1);
        assert!(event2.is_none());

        // 3. 점수 연출 완료로 99.5% 도달 (rate 향상) -> 이벤트 재방출!
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 99.5,
                is_max_combo: false,
            }),
            None,
            0.0,
        );
        let (_, event3) = detector.detect(&frame, &rois, Some(10), 1.2);
        assert!(event3.is_some());
        let e3 = event3.unwrap();
        assert_eq!(e3.rate, 99.5);
        assert!(!e3.is_max_combo);

        // 4. MAX COMBO 뱃지 연출 등장 (MC: false -> true 개선) -> 이벤트 재방출!
        let stable_mc = Some(overmax_core::PlayContext {
            song_id: 10,
            mode: super::Mode::B4,
            diff: super::Difficulty::NM,
            rate: 99.5,
            is_max_combo: true,
        });
        let (_, event4) = detector.build_session_state_and_event(
            SceneType::ResultFreestyle,
            stable_mc.clone(),
            stable_mc.clone(),
        );
        assert!(event4.is_some());
        let e4 = event4.unwrap();
        assert_eq!(e4.rate, 99.5);
        assert!(e4.is_max_combo);

        // 5. MAX COMBO 상태 유지 -> 이벤트 미방출
        let (_, event5) =
            detector.build_session_state_and_event(SceneType::ResultFreestyle, None, stable_mc);
        assert!(event5.is_none());
    }

    #[test]
    fn unplayed_song_after_recorded_song_does_not_retain_previous_rate() {
        let mut detector = PlayStateDetector::new(1);
        let mut frame = blank_frame();
        // 4B Mode Color & NM Diff Card
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        // 1. 곡 1 (기록 있음: 98.5%)
        let checksums = PlayStateDetector::rate_input_checksums(&frame, &rois);
        detector
            .rate_cache
            .sync_key(Some((1, super::Mode::B4, super::Difficulty::NM)));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 98.5,
                is_max_combo: false,
            }),
            checksums,
            1.0,
        );

        let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(state1.context.as_ref().unwrap().rate, 98.5);

        // 2. 곡 2 (기록 없음 0.00% / 미플레이)로 변경
        let (state2, _) = detector.detect(&frame, &rois, Some(2), 2.0);
        assert_eq!(
            state2.context.as_ref().unwrap().rate,
            0.0,
            "이전 곡의 기록(98.5%)이 새 미플레이 곡(0.00%)에 잔류하면 안 됨"
        );
        assert_eq!(detector.rate_cache.get(), Some(&PatternRecord::Unplayed));
    }

    #[test]
    fn rate_input_change_without_valid_score_clears_cached_rate() {
        let mut detector = PlayStateDetector::new(1);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        // 임의의 이전 rate 캐시와 체크섬 설정
        detector
            .rate_cache
            .sync_key(Some((1, super::Mode::B4, super::Difficulty::NM)));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 95.0,
                is_max_combo: false,
            }),
            Some(RateInputChecksums {
                score: 100,
                badge: 0,
            }),
            0.0,
        );

        // score 영역에 체크섬 변경 유발 (하지만 빈 화면이라 유효한 점수가 검출되지 않음)
        paint_rect(
            &mut frame,
            1000,
            100,
            1050,
            120,
            Bgr::from_rgb_hex(0xFFFFFF),
        );

        let (state, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(
            state.context.as_ref().unwrap().rate,
            0.0,
            "유효한 점수가 없는 새로운 입력 감지 시 캐시된 점수가 0.0으로 클리어되어야 함"
        );
        assert_eq!(detector.rate_cache.get(), Some(&PatternRecord::Unplayed));
    }

    #[test]
    fn test_verified_play_event_emits_on_song_select_screen() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        // 곡 1에 대해 99.63% (MC=true) 점수 주입
        let checksums = PlayStateDetector::rate_input_checksums(&frame, &rois);
        detector
            .rate_cache
            .sync_key(Some((100, super::Mode::B4, super::Difficulty::NM)));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 99.63,
                is_max_combo: true,
            }),
            checksums,
            1.0,
        );

        // Frame 1, 2: 안정화 중 (히스토리 3개 필요)
        let (_, e1) = detector.detect(&frame, &rois, Some(100), 1.0);
        let (_, e2) = detector.detect(&frame, &rois, Some(100), 1.1);
        assert!(e1.is_none());
        assert!(e2.is_none());

        // Frame 3: 3연속 일치로 안정화 완료 -> 선곡 화면 1회 이벤트 방출!
        let (state3, e3) = detector.detect(&frame, &rois, Some(100), 1.2);
        assert!(state3.is_stable);
        assert!(
            e3.is_some(),
            "선곡 화면에서 첫 안정화 시 이벤트가 방출되어야 함"
        );
        let event = e3.unwrap();
        assert_eq!(event.song_id, 100);
        assert_eq!(event.mode, super::Mode::B4);
        assert_eq!(event.diff, super::Difficulty::NM);
        assert_eq!(event.rate, 99.63);
        assert!(
            event.is_max_combo,
            "주입된 맥스 콤보 상태가 정상 보존되어야 함"
        );
        assert!(
            !event.is_result_screen,
            "선곡 화면이므로 is_result_screen = false 여야 함 (무조건 Update 정책)"
        );

        // Frame 4: 동일 곡 선곡 화면 체류 -> 중복 방출 0회
        let (_, e4) = detector.detect(&frame, &rois, Some(100), 1.3);
        assert!(e4.is_none(), "동일 곡 체류 중에는 중복 방출되지 않아야 함");

        // 곡 2로 전환 및 95.0% 주입
        detector
            .rate_cache
            .sync_key(Some((200, super::Mode::B4, super::Difficulty::NM)));
        detector.rate_cache.set(
            Some(PatternRecord::Played {
                rate: 95.0,
                is_max_combo: false,
            }),
            checksums,
            2.0,
        );

        let _ = detector.detect(&frame, &rois, Some(200), 2.0);
        let _ = detector.detect(&frame, &rois, Some(200), 2.1);
        let (state2_3, e2_3) = detector.detect(&frame, &rois, Some(200), 2.2);

        assert!(state2_3.is_stable);
        assert!(e2_3.is_some(), "새 곡 전환 시 새 이벤트가 방출되어야 함");
        let event2 = e2_3.unwrap();
        assert_eq!(event2.song_id, 200);
        assert_eq!(event2.rate, 95.0);
        assert!(!event2.is_result_screen);
    }
}
