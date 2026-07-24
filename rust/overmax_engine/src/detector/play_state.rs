use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::region_mean_bgr;
use crate::detector::ocr_engine::{OcrDetector, OcrTelemetry};
use crate::detector::roi::RoiManager;
use overmax_core::{Changed, GameSessionState, PlayContext};
use std::collections::VecDeque;

pub const MIN_VALID_RATE: f32 = 80.0;

const BTN_MODE_MAX_DIST: f32 = 60.0;
const DIFF_MIN_BRIGHTNESS: f32 = 45.0;
const DIFF_CONFIDENT_MARGIN: f32 = 15.0;
const DIFFICULTIES: [&str; 4] = ["NM", "HD", "MX", "SC"];

type ButtonColorEntry = (&'static str, &'static [(u8, u8, u8)]);

#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    context: Option<PlayContext>,
}

/// 결과창 mode·diff 인식 결과 캐시.
///
/// - `result_*`: 결과창에서 인식 시도한 값. 채워져 있으면 우선 사용되어
///   결과창 진입 후 프레임 간 흔들림 없이 유지된다.
struct ModeDiffCache {
    result_mode: Changed<Option<String>>,
    result_diff: Changed<Option<String>>,
}

impl ModeDiffCache {
    fn new() -> Self {
        Self {
            result_mode: Changed::new(None),
            result_diff: Changed::new(None),
        }
    }

    /// 결과창 -> 선곡창 복귀 시 결과창 인식값을 초기화한다.
    fn clear_result_cache(&mut self) {
        self.result_mode.update(None);
        self.result_diff.update(None);
    }
}

pub struct PlayStateDetector {
    history_size: usize,
    history: VecDeque<Option<RawPlayState>>,
    last_stable_state: Option<GameSessionState>,
    last_rate_checksum: Option<u64>,
    last_rate_result: (Option<f32>, String, Option<OcrTelemetry>),
    last_rate_ocr_ts: f64,
    cache: ModeDiffCache,
    last_song_id: Changed<Option<i32>>,
    result_rate_window: VecDeque<f32>,
}

impl PlayStateDetector {
    fn should_run_rate_ocr(&self, now: f64) -> bool {
        // 결과창과 선곡창 모두에서 캐싱 없이 실시간 수치 변경을 실시간 감지하기 위해 항상 OCR을 시도합니다.
        // 다만 불필요한 매 프레임 연산을 막기 위해 최소 200ms 간격 제한만 수행합니다.
        now - self.last_rate_ocr_ts >= 0.20
    }

    fn process_rate_ocr(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        ocr: &OcrDetector,
        scene: overmax_core::SceneType,
        is_result: bool,
        now: f64,
    ) -> (f32, Option<OcrTelemetry>) {
        if self.should_run_rate_ocr(now) {
            if let Some(rate_res) = rois.and_then_roi(frame, "rate", |rate_img| {
                let mut rate_res = ocr.detect_rate(rate_img);
                rate_res.0 = Self::cross_validate_rate_with_score(
                    ocr, frame, rois, scene, is_result, rate_res.0,
                );
                Some(rate_res)
            }) {
                self.last_rate_ocr_ts = now;

                debug_println!(
                    "    [detect] rate OCR run. rate={:?}, text='{}'",
                    rate_res.0,
                    rate_res.1
                );

                self.apply_rate_ocr_result(is_result, rate_res);
            }
        }

        let rate = self.last_rate_result.0.unwrap_or(0.0);
        let telemetry = self.last_rate_result.2.clone();
        (rate, telemetry)
    }

    fn apply_rate_ocr_result(
        &mut self,
        is_result: bool,
        mut res: (Option<f32>, String, Option<OcrTelemetry>),
    ) {
        if is_result {
            if let Some(new_r) = res.0 {
                self.push_result_rate_sample(new_r);
                res.0 = self.median_result_rate();
            }
        } else {
            self.result_rate_window.clear();
        }
        self.last_rate_result = res;
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

    pub fn new(history_size: usize) -> Self {
        Self {
            history_size: history_size.max(1),
            history: VecDeque::new(),
            last_stable_state: None,
            last_rate_checksum: None,
            last_rate_result: (None, String::new(), None),
            last_rate_ocr_ts: 0.0,
            cache: ModeDiffCache::new(),
            last_song_id: Changed::new(None),
            result_rate_window: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.last_stable_state = None;
        self.last_rate_checksum = None;
        self.last_rate_result = (None, String::new(), None);
        self.last_rate_ocr_ts = 0.0;
        // 결과창 진입 시 복구용(result_mode/diff) 캐시는 reset 시에도 보존합니다.
        self.last_song_id.update(None);
        self.result_rate_window.clear();
    }

    pub fn clear_detected_cache(&mut self) {
        self.cache.clear_result_cache();
    }

    #[cfg(test)]
    pub(crate) fn seed_detected_cache_for_test(&mut self) {
        self.cache.result_mode.update(Some("4B".into()));
        self.cache.result_diff.update(Some("NM".into()));
    }

    #[cfg(test)]
    pub(crate) fn detected_cache_is_empty_for_test(&self) -> bool {
        self.cache.result_mode.get().is_none() && self.cache.result_diff.get().is_none()
    }

    fn resolve_result_mode_diff(
        &mut self,
        scene: overmax_core::SceneType,
        frame: &CapturedFrame,
        rois: &RoiManager,
        ocr: &OcrDetector,
    ) -> (Option<String>, Option<String>) {
        let mut detected_mode = None;
        let mut detected_diff = None;

        // 1. 결과창 실시간 템플릿 매칭 우선 시도
        match scene {
            overmax_core::SceneType::ResultFreestyle => {
                detected_mode =
                    rois.and_then_roi(frame, "mode_digit", |img| ocr.detect_freestyle_mode(img));
                detected_diff =
                    rois.and_then_roi(frame, "diff_panel", |img| ocr.detect_result_difficulty(img));
            }
            overmax_core::SceneType::ResultOpen3 | overmax_core::SceneType::ResultOpen2 => {
                detected_mode = detect_button_mode_from_roi(frame, rois, "openmatch_mode");
                detected_diff = rois.and_then_roi(frame, "openmatch_diff", |img| {
                    ocr.detect_openmatch_result_difficulty(img)
                });
            }
            _ => {}
        }

        // 2. 결과창 템플릿 매칭 성공 시, 결과창 캐시를 업데이트 (자가 보정 가능)
        if detected_mode.is_some() {
            self.cache.result_mode.update(detected_mode.clone());
        }
        if detected_diff.is_some() {
            self.cache.result_diff.update(detected_diff.clone());
        }

        // 3. 최종 반환값 결정: 결과창 캐시가 존재하면 우선 사용
        let final_mode = self.cache.result_mode.get().clone();
        let final_diff = self.cache.result_diff.get().clone();

        (final_mode, final_diff)
    }

    fn cross_validate_rate_with_score(
        ocr: &OcrDetector,
        frame: &CapturedFrame,
        rois: &RoiManager,
        scene: overmax_core::SceneType,
        is_result: bool,
        detected_rate: Option<f32>,
    ) -> Option<f32> {
        let is_song_select = matches!(
            scene,
            overmax_core::SceneType::Freestyle | overmax_core::SceneType::OpenMatch
        );
        if !(is_result || is_song_select) {
            return detected_rate;
        }

        let score_val = rois.and_then_roi(frame, "score", |img| ocr.detect_score(img));
        let Some(score_val) = score_val else {
            return detected_rate;
        };

        debug_println!("    [detect] score OCR run. score={}", score_val);
        let calc_rate = score_val as f32 / 10000.0;

        // 선곡창인 경우 스코어 OCR 오인식에 대비하여 엄격한 가드 적용
        let is_valid_range = if is_song_select {
            (MIN_VALID_RATE..=100.0).contains(&calc_rate)
        } else {
            (0.0..=100.0).contains(&calc_rate)
        };

        if !is_valid_range {
            return detected_rate;
        }

        match detected_rate {
            Some(r) => resolve_most_plausible_rate(r, calc_rate, is_song_select),
            None => Some((calc_rate * 100.0).floor() / 100.0),
        }
    }

    pub fn detect(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<i32>,
        ocr: &OcrDetector,
        now: f64,
    ) -> (GameSessionState, Option<OcrTelemetry>) {
        let scene = rois.current_scene();
        let is_result = scene.is_result();

        let mode;
        let diff;
        let mut confident = true;
        let is_max_combo;

        if is_result {
            is_max_combo = detect_max_combo_result(frame, rois);
            let (m, d) = self.resolve_result_mode_diff(scene, frame, rois, ocr);
            mode = m;
            diff = d;
        } else {
            self.cache.result_mode.update(None);
            self.cache.result_diff.update(None);

            mode = detect_button_mode(frame, rois);
            let (d, conf) = detect_difficulty(frame, rois);
            diff = d;
            confident = conf;
            is_max_combo = detect_max_combo(frame, rois);
        }

        self.last_song_id.update(song_id);

        let mut telemetry = None;
        debug_println!(
            "    [detect] song_id={:?}, mode={:?}, diff={:?}, confident={}",
            song_id,
            mode,
            diff,
            confident
        );
        let context = if let (Some(sid), Some(m), Some(d)) = (song_id, mode, diff) {
            if confident {
                let (rate, tel) = self.process_rate_ocr(frame, rois, ocr, scene, is_result, now);
                telemetry = tel;

                let rate_valid = !is_result
                    || self
                        .last_rate_result
                        .0
                        .map(|r| r >= MIN_VALID_RATE)
                        .unwrap_or(false);

                Some(PlayContext {
                    song_id: sid,
                    mode: m,
                    diff: d,
                    rate: if rate_valid { rate } else { 0.0 },
                    is_max_combo: if rate_valid && rate > 0.0 {
                        is_max_combo
                    } else {
                        false
                    },
                })
            } else {
                None
            }
        } else {
            None
        };

        let raw = RawPlayState {
            context: context.clone(),
        };
        self.push_raw(raw);

        if let Some(stable) = self.stable_raw() {
            let state = GameSessionState {
                scene,
                context: stable.context.clone(),
                is_stable: true,
                is_fullscreen: false, // will be overwritten/updated by detection worker
            };
            self.last_stable_state = Some(state.clone());
            return (state, telemetry);
        }

        (
            GameSessionState {
                scene,
                context,
                is_stable: false,
                is_fullscreen: false,
            },
            telemetry,
        )
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
) -> Option<String> {
    let roi = rois.get_roi(roi_name)?;
    let mean = region_mean_bgr(frame, roi);
    let mut best = (None, f32::INFINITY);

    let colors_table = if roi_name == "openmatch_mode" {
        openmatch_button_colors()
    } else {
        button_colors()
    };

    for (mode, colors) in colors_table {
        for color in colors {
            let dist = color_dist(mean, *color);
            if dist < best.1 {
                best = (Some(mode.to_string()), dist);
            }
        }
    }
    (best.1 <= BTN_MODE_MAX_DIST).then_some(best.0).flatten()
}

pub fn detect_button_mode(frame: &CapturedFrame, rois: &RoiManager) -> Option<String> {
    detect_button_mode_from_roi(frame, rois, "btn_mode")
}

pub fn detect_difficulty(frame: &CapturedFrame, rois: &RoiManager) -> (Option<String>, bool) {
    let mut brightnesses = DIFFICULTIES
        .iter()
        .filter_map(|diff| {
            let roi = rois.get_diff_panel_roi(diff)?;
            let (b, g, r) = region_mean_bgr(frame, roi);
            Some((*diff, (f32::from(b) + f32::from(g) + f32::from(r)) / 3.0))
        })
        .collect::<Vec<_>>();
    brightnesses.sort_by(|a, b| b.1.total_cmp(&a.1));
    let Some((best, max_bright)) = brightnesses.first().copied() else {
        return (None, false);
    };
    if max_bright < DIFF_MIN_BRIGHTNESS {
        return (None, false);
    }
    let second = brightnesses.get(1).map_or(0.0, |item| item.1);
    (
        Some(best.to_string()),
        max_bright - second >= DIFF_CONFIDENT_MARGIN,
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
    score_perfect <= 10.0 || score_mc <= 10.0
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
    score_perfect <= 20.0 || score_mc <= 20.0
}

fn button_colors() -> [ButtonColorEntry; 4] {
    [
        ("4B", &[(0x55, 0x4F, 0x2D), (0x5A, 0x47, 0x0C)]),
        ("5B", &[(0xC6, 0xA9, 0x44)]),
        ("6B", &[(0x30, 0x94, 0xED)]),
        ("8B", &[(0x31, 0x14, 0x1D)]),
    ]
}

fn openmatch_button_colors() -> [ButtonColorEntry; 4] {
    [
        ("4B", &[(102, 118, 46)]),
        ("5B", &[(147, 136, 95)]),
        ("6B", &[(61, 137, 192)]),
        ("8B", &[(153, 90, 88)]),
    ]
}

fn color_dist(left: (u8, u8, u8), right: (u8, u8, u8)) -> f32 {
    let db = f32::from(left.0) - f32::from(right.0);
    let dg = f32::from(left.1) - f32::from(right.1);
    let dr = f32::from(left.2) - f32::from(right.2);
    (db * db + dg * dg + dr * dr).sqrt()
}

pub fn resolve_most_plausible_rate(
    rate_ocr: f32,
    score_rate: f32,
    _is_song_select: bool,
) -> Option<f32> {
    let score_rate_rounded = (score_rate * 100.0).floor() / 100.0;
    if (rate_ocr - score_rate_rounded).abs() >= 0.01 {
        debug_println!(
            "    [detect] Rate mismatch (Rate OCR: {:.2}%, Score Rate: {:.2}%). Prioritizing Score Rate: {:.2}% (is_song_select: {})",
            rate_ocr,
            score_rate,
            score_rate_rounded,
            _is_song_select
        );
    }
    Some(score_rate_rounded)
}

#[allow(dead_code)]
pub fn get_rate_plausibility(rate: f32) -> i32 {
    if (90.0..=100.0).contains(&rate) {
        3
    } else if (70.0..=90.0).contains(&rate) {
        2
    } else if (50.0..=70.0).contains(&rate) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_button_mode, PlayStateDetector};
    use crate::capture::frame::CapturedFrame;
    use crate::detector::roi::RoiManager;
    use overmax_core::SceneType;

    #[test]
    fn detects_button_mode_from_reference_color() {
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, (0x55, 0x4F, 0x2D));
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);
        assert_eq!(detect_button_mode(&frame, &rois), Some("4B".to_string()));
    }

    #[test]
    fn marks_state_stable_after_repeated_valid_frames() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, (0x55, 0x4F, 0x2D));
        paint_rect(&mut frame, 98, 488, 208, 516, (220, 220, 220));
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        let ocr = crate::detector::ocr_engine::OcrDetector::new();
        assert!(
            !detector
                .detect(&frame, &rois, Some(7), &ocr, 1.0)
                .0
                .is_stable
        );
        assert!(
            !detector
                .detect(&frame, &rois, Some(7), &ocr, 2.0)
                .0
                .is_stable
        );
        assert!(
            detector
                .detect(&frame, &rois, Some(7), &ocr, 3.0)
                .0
                .is_stable
        );
    }

    #[test]
    fn result_mode_diff_remains_none_without_match() {
        let mut detector = PlayStateDetector::new(3);
        detector.cache.clear_result_cache();

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);
        let ocr = crate::detector::ocr_engine::OcrDetector::new();

        // 결과창에서 mode_digit/diff_panel ROI가 없어 인식에 실패하면 None이어야 함
        let (state, _) = detector.detect(&frame, &rois, Some(7), &ocr, 1.0);
        assert!(state.context.is_none());
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    fn paint_rect(
        frame: &mut CapturedFrame,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        bgr: (u8, u8, u8),
    ) {
        for y in y1..y2 {
            for x in x1..x2 {
                let idx = ((y * frame.width + x) * 4) as usize;
                frame.bgra[idx] = bgr.0;
                frame.bgra[idx + 1] = bgr.1;
                frame.bgra[idx + 2] = bgr.2;
            }
        }
    }

    #[test]
    fn test_resolve_most_plausible_rate_prioritizes_score() {
        use super::resolve_most_plausible_rate;
        // Rate OCR: 98.85%, Score Rate: 99.85% (from score 998,500)
        // In both song select and result screen, score rate (99.85%) must be prioritized
        assert_eq!(resolve_most_plausible_rate(98.85, 99.85, true), Some(99.85));
        assert_eq!(
            resolve_most_plausible_rate(98.85, 99.85, false),
            Some(99.85)
        );

        // When matching
        assert_eq!(resolve_most_plausible_rate(99.85, 99.85, true), Some(99.85));
    }
}
