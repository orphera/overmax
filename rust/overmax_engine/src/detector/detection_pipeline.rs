use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{crop_roi, make_thumbnail, thumbnail_changed};
use crate::detector::hysteresis::HysteresisBuffer;
use crate::detector::ocr_engine::{OcrDetector, OcrTelemetry};
use crate::detector::play_state::PlayStateDetector;
use crate::detector::roi::RoiManager;
use overmax_core::{GameSessionState, SceneType};
use overmax_data::ImageIndexDb;

const JACKET_MATCH_INTERVAL: f64 = 0.25;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const JACKET_FORCE_RECHECK_LONG_SEC: f64 = 30.0;
const JACKET_EDGE_THRESHOLD: f32 = 15.0;

#[derive(Clone, Debug, PartialEq)]
pub struct DetectionOutput {
    pub logo_detected: bool,
    pub is_song_select: bool,
    pub is_result: bool,
    pub is_leaving: bool,
    pub confidence: f32,
    pub state: GameSessionState,
    pub current_song_id: Option<i32>,
    pub image_db_ready: bool,
    pub jacket_status: JacketMatchStatus,
    pub game_rect: Option<crate::capture::window_tracker::WindowRect>,
    pub ocr_telemetry: Option<OcrTelemetry>,
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
    image_db: ImageIndexDb,
    jacket_matcher: overmax_data::JacketMatcher,
    rois: RoiManager,
    hysteresis: HysteresisBuffer,
    play_state: PlayStateDetector,
    ocr: OcrDetector,
    current_song_id: Option<i32>,
    last_logo_ocr_ts: f64,
    last_logo_scene: SceneType,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
    result_scene_streak: u32,
    last_detected_result_scene: SceneType,
    unknown_since: Option<f64>,
}

impl DetectionPipeline {
    pub fn new(image_db: ImageIndexDb) -> Self {
        let jacket_matcher = image_db.matcher();
        Self {
            image_db,
            jacket_matcher,
            rois: RoiManager::new(1920, 1080),
            hysteresis: HysteresisBuffer::new(4, 0.5, 2, 0.25, 2),
            play_state: PlayStateDetector::new(5),
            ocr: OcrDetector::new(),
            current_song_id: None,
            last_logo_ocr_ts: 0.0,
            last_logo_scene: SceneType::Unknown,
            last_jacket_ts: 0.0,
            last_jacket_match_ts: 0.0,
            last_jacket_thumb: None,
            result_scene_streak: 0,
            last_detected_result_scene: SceneType::Unknown,
            unknown_since: None,
        }
    }

    pub fn ocr_available(&self) -> bool {
        self.ocr.is_available()
    }

    pub fn detect(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        if let Some(scene) = self.detect_logo_if_due(frame, now) {
            self.process_frame_with_logo(frame, scene, now)
        } else {
            self.process_frame_cached(frame, now)
        }
    }

    pub fn process_frame_with_logo(
        &mut self,
        frame: &CapturedFrame,
        scene: SceneType,
        now: f64,
    ) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);

        let logo_detected = scene != SceneType::Unknown && scene != SceneType::Online;
        if logo_detected {
            self.rois.set_scene(scene);
        }

        self.hysteresis.update(logo_detected);
        self.process_frame_shared(frame, logo_detected, now)
    }

    pub fn process_frame_cached(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);

        let logo_detected =
            self.last_logo_scene != SceneType::Unknown && self.last_logo_scene != SceneType::Online;
        self.hysteresis.update(logo_detected);
        self.process_frame_shared(frame, logo_detected, now)
    }

    fn process_frame_shared(
        &mut self,
        frame: &CapturedFrame,
        logo_detected: bool,
        now: f64,
    ) -> DetectionOutput {
        let is_result = self.last_logo_scene.is_result();
        let is_song_select = self.hysteresis.is_active || is_result;
        let is_leaving = if is_result {
            false
        } else {
            self.hysteresis.is_leaving
        };
        let confidence = self.hysteresis.confidence;

        if !is_song_select {
            self.reset_on_screen_exit();
            return self.output(
                logo_detected,
                false,
                false, // is_result
                is_leaving,
                confidence,
                GameSessionState::detecting(),
                JacketMatchStatus::NotSongSelect,
                None,
            );
        }

        if is_leaving {
            return self.output(
                logo_detected,
                true,
                false, // is_result
                true,
                confidence,
                GameSessionState::detecting(),
                JacketMatchStatus::Leaving,
                None,
            );
        }

        // 결과창에서 다시 선곡 화면으로 복귀하는 경우 결과창 캐시 리셋
        if !is_result {
            self.play_state.clear_detected_cache();
        }

        let jacket_status = self.update_song_id_from_jacket(frame, now);
        let (state, telemetry) =
            self.play_state
                .detect(frame, &self.rois, self.current_song_id, &self.ocr, now);

        self.output(
            logo_detected,
            true,
            is_result,
            false,
            confidence,
            state,
            jacket_status,
            telemetry,
        )
    }

    fn detect_logo_if_due(&mut self, frame: &CapturedFrame, now: f64) -> Option<SceneType> {
        // 씬이 Unknown인 경우(진입 대기): 빠른 인식을 위해 0.3초 주기로 감시
        // 씬이 이미 확정된 경우(유지 중): CPU 소모 최소화를 위해 2.0초 주기로 완화 (이탈은 픽셀 매칭으로 즉시 처리되므로 반응성 무관)
        if self.last_logo_scene == SceneType::Unknown {
            if self.unknown_since.is_none() {
                self.unknown_since = Some(now);
            }
        } else {
            self.unknown_since = None;
        }

        let cooldown = if self.last_logo_scene == SceneType::Unknown {
            let unknown_duration = now - self.unknown_since.unwrap_or(now);
            if unknown_duration < 3.0 {
                0.3
            } else {
                1.5
            }
        } else {
            2.0
        };

        if now - self.last_logo_ocr_ts < cooldown {
            return None;
        }

        let Some((scene, raw_text, matched_song_id)) =
            parse_static_scene(frame, &self.ocr, &self.rois, &self.jacket_matcher)
        else {
            debug_println!("    [detect_logo_if_due] logo crop failed! now={}", now);
            self.last_logo_scene = SceneType::Unknown;
            self.last_logo_ocr_ts = now;
            return Some(SceneType::Unknown);
        };

        if let Some(song_id) = matched_song_id {
            self.current_song_id = Some(song_id);
            self.last_jacket_match_ts = now;

            // process_frame_shared 에서 중복 매칭이 돌지 않도록 썸네일 캐시 갱신
            if let Some(jacket_roi) = self.rois.get_roi("jacket") {
                if let Some(jacket_img) = crop_roi(frame, jacket_roi) {
                    if let Some(thumb) = make_thumbnail(&jacket_img) {
                        self.last_jacket_thumb = Some(thumb);
                    }
                }
            }
        }

        debug_println!(
            "    [detect_logo_if_due] now={}, OCR raw='{}', static_scene={:?}",
            now,
            raw_text,
            scene
        );

        let scene_candidate = self.process_frame_fallback(scene, &raw_text);

        if scene_candidate != SceneType::Unknown && scene_candidate != SceneType::Online {
            self.rois.set_scene(scene_candidate);
        }

        let final_scene = self.commit_result_scene(frame, scene_candidate, &raw_text);
        self.last_logo_ocr_ts = now;
        Some(final_scene)
    }

    fn process_frame_fallback(&self, scene_candidate: SceneType, raw_text: &str) -> SceneType {
        let mut scene = scene_candidate;
        if scene == SceneType::Unknown {
            scene = self.try_keyword_lockin(raw_text).unwrap_or(scene);
        }
        scene
    }

    fn try_keyword_lockin(&self, raw_text: &str) -> Option<SceneType> {
        let is_prev_result = self.last_logo_scene.is_result();
        if !is_prev_result {
            return None;
        }
        let norm_logo = raw_text.to_uppercase();
        let has_logo_keyword = norm_logo.contains("BUTTON")
            || norm_logo.contains("TUNES")
            || norm_logo.contains("TIJNFS")
            || norm_logo.contains("TUNE");
        if has_logo_keyword {
            debug_println!("    [detect_logo_if_due] Lock-in: keeping previous result scene {:?} due to logo keyword match", self.last_logo_scene);
            Some(self.last_logo_scene)
        } else {
            None
        }
    }

    fn commit_result_scene(
        &mut self,
        _frame: &CapturedFrame,
        candidate: SceneType,
        _raw_text: &str,
    ) -> SceneType {
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
                self.last_logo_scene = candidate;
            }
        } else {
            self.result_scene_streak = 0;
            self.last_detected_result_scene = SceneType::Unknown;
            self.last_logo_scene = candidate;
        }

        self.last_logo_scene
    }

    fn update_song_id_from_jacket(&mut self, frame: &CapturedFrame, now: f64) -> JacketMatchStatus {
        if !self.image_db.is_ready() {
            return JacketMatchStatus::DbNotReady;
        }
        if now - self.last_jacket_ts < JACKET_MATCH_INTERVAL {
            return JacketMatchStatus::Cooldown;
        }
        self.last_jacket_ts = now;
        let Some(jacket) = self
            .rois
            .get_roi("jacket")
            .and_then(|roi| crop_roi(frame, roi))
        else {
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
        self.apply_jacket_match(&jacket)
    }

    fn apply_jacket_match(
        &mut self,
        jacket: &crate::capture::frame_utils::ImageRegion,
    ) -> JacketMatchStatus {
        let Some(result) = self.jacket_matcher.match_jacket(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        ) else {
            self.current_song_id = None;
            return JacketMatchStatus::NoMatch;
        };
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
        logo_detected: bool,
        is_song_select: bool,
        is_result: bool,
        is_leaving: bool,
        confidence: f32,
        state: GameSessionState,
        jacket_status: JacketMatchStatus,
        ocr_telemetry: Option<OcrTelemetry>,
    ) -> DetectionOutput {
        DetectionOutput {
            logo_detected,
            is_song_select,
            is_result,
            is_leaving,
            confidence,
            state,
            current_song_id: self.current_song_id,
            image_db_ready: self.image_db.is_ready(),
            jacket_status,
            game_rect: None,
            ocr_telemetry,
        }
    }
}

pub fn detect_openmatch_color_match(mean: (u8, u8, u8)) -> bool {
    let openmatch_colors = [
        (102u8, 118u8, 46u8), // 4B
        (147u8, 136u8, 95u8), // 5B
        (61u8, 137u8, 192u8), // 6B
        (153u8, 90u8, 88u8),  // 8B
    ];
    let max_dist = 60.0f32;
    for color in &openmatch_colors {
        let db = f32::from(mean.0) - f32::from(color.0);
        let dg = f32::from(mean.1) - f32::from(color.1);
        let dr = f32::from(mean.2) - f32::from(color.2);
        let dist = (db * db + dg * dg + dr * dr).sqrt();
        if dist <= max_dist {
            return true;
        }
    }
    false
}

pub fn check_open_match_badge(frame: &CapturedFrame, rois: &RoiManager) -> Option<SceneType> {
    // 5x5 BGR Color-based Fallback
    if let Some(color_roi) = rois.get_roi_for_scene("openmatch_mode", SceneType::ResultOpen3) {
        let mean = crate::capture::frame_utils::region_mean_bgr(frame, color_roi);
        if detect_openmatch_color_match(mean) {
            return Some(SceneType::ResultOpen3);
        }
    }

    if let Some(color_roi) = rois.get_roi_for_scene("openmatch_mode", SceneType::ResultOpen2) {
        let mean = crate::capture::frame_utils::region_mean_bgr(frame, color_roi);
        if detect_openmatch_color_match(mean) {
            return Some(SceneType::ResultOpen2);
        }
    }

    None
}

fn detect_result_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> Option<(SceneType, i32)> {
    // ResultFreestyle, ResultOpen3, ResultOpen2 재킷 ROI는 같은 위치를 공유함
    // 따라서 ResultFreestyle 재킷 ROI를 기준으로 엣지 디텍션을 수행하고, 추가 확인을 통해 분기함
    let jacket_roi = rois.get_roi_for_scene("jacket", SceneType::ResultFreestyle)?;
    if let Some(edge_strength) = detect_jacket_edges(frame, jacket_roi) {
        if edge_strength >= JACKET_EDGE_THRESHOLD {
            // 결과창 재킷 매칭 시도
            let mut song_id = None;
            if let Some(jacket_img) = crop_roi(frame, jacket_roi) {
                if let Some(match_res) = matcher.match_jacket(
                    &jacket_img.bgra,
                    jacket_img.width as usize,
                    jacket_img.height as usize,
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
            }

            // 재킷 매칭이 확실히 성공한 경우에만 결과창 씬으로 반환
            if let Some(id) = song_id {
                if let Some(fallback_scene) = check_open_match_badge(frame, rois) {
                    return Some((fallback_scene, id));
                } else {
                    return Some((SceneType::ResultFreestyle, id));
                }
            }
        }
    }
    None
}

fn detect_freestyle_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> Option<(SceneType, i32)> {
    let jacket_roi = rois.get_roi_for_scene("jacket", SceneType::Freestyle)?;
    if let Some(edge_strength) = detect_jacket_edges(frame, jacket_roi) {
        if edge_strength >= JACKET_EDGE_THRESHOLD {
            if let Some(jacket_img) = crop_roi(frame, jacket_roi) {
                if let Some(match_res) = matcher.match_jacket(
                    &jacket_img.bgra,
                    jacket_img.width as usize,
                    jacket_img.height as usize,
                    4,
                ) {
                    let threshold = matcher.similarity_threshold();
                    if match_res.similarity >= threshold {
                        if let Ok(song_id) = match_res.image_id.parse::<i32>() {
                            debug_println!("    [detect_freestyle_scene_via_edge] Bypassed logo OCR: Freestyle screen detected via jacket edge ({:.2}) and similarity ({:.4})!", edge_strength, match_res.similarity);
                            return Some((SceneType::Freestyle, song_id));
                        }
                    }
                }
            }
        }
    }
    None
}

fn detect_openmatch_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> Option<(SceneType, i32)> {
    let jacket_roi = rois.get_roi_for_scene("jacket", SceneType::OpenMatch)?;
    if let Some(edge_strength) = detect_jacket_edges(frame, jacket_roi) {
        if edge_strength >= JACKET_EDGE_THRESHOLD {
            if let Some(jacket_img) = crop_roi(frame, jacket_roi) {
                if let Some(match_res) = matcher.match_jacket(
                    &jacket_img.bgra,
                    jacket_img.width as usize,
                    jacket_img.height as usize,
                    4,
                ) {
                    let threshold = matcher.similarity_threshold();
                    if match_res.similarity >= threshold {
                        if let Ok(song_id) = match_res.image_id.parse::<i32>() {
                            debug_println!("    [detect_openmatch_scene_via_edge] Bypassed logo OCR: OpenMatch screen detected via jacket edge ({:.2}) and similarity ({:.4})!", edge_strength, match_res.similarity);
                            return Some((SceneType::OpenMatch, song_id));
                        }
                    }
                }
            }
        }
    }
    None
}

fn parse_static_scene(
    frame: &CapturedFrame,
    _ocr: &OcrDetector,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> Option<(SceneType, String, Option<i32>)> {
    // 1. 결과창 감지 우선 (Bypass OCR)
    if let Some((scene, song_id)) = detect_result_scene_via_edge(frame, rois, matcher) {
        return Some((scene, String::new(), Some(song_id)));
    }

    // 2. 오픈매치 대기실 감지 우선 (Bypass OCR)
    if let Some((scene, song_id)) = detect_openmatch_scene_via_edge(frame, rois, matcher) {
        return Some((scene, String::new(), Some(song_id)));
    }

    // 3. 프리스타일 선곡창 감지 우선 (Bypass OCR)
    if let Some((scene, song_id)) = detect_freestyle_scene_via_edge(frame, rois, matcher) {
        return Some((scene, String::new(), Some(song_id)));
    }

    // 4. 최종 폴백: Windows OCR을 통한 로고 감지 비활성화
    Some((SceneType::Unknown, String::new(), None))
}

pub fn detect_scene_from_logo(
    frame: &CapturedFrame,
    ocr: &OcrDetector,
    rois: &RoiManager,
    matcher: &overmax_data::JacketMatcher,
) -> SceneType {
    parse_static_scene(frame, ocr, rois, matcher)
        .map(|(scene, _, _)| scene)
        .unwrap_or(SceneType::Unknown)
}

fn detect_jacket_edges(
    frame: &CapturedFrame,
    jacket_roi: crate::detector::roi::RoiRect,
) -> Option<f32> {
    let margin = 8;
    let ext_roi = crate::detector::roi::RoiRect {
        x1: jacket_roi.x1 - margin,
        y1: jacket_roi.y1 - margin,
        x2: jacket_roi.x2 + margin,
        y2: jacket_roi.y2 + margin,
    };
    let ext_img = crop_roi(frame, ext_roi)?;
    overmax_cv::detect_rect_edges(
        &ext_img.bgra,
        ext_img.width as usize,
        ext_img.height as usize,
        margin as usize,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::{DetectionPipeline, JacketMatchStatus};
    use crate::capture::frame::CapturedFrame;
    use overmax_data::ImageIndexDb;

    #[test]
    fn stays_detecting_until_hysteresis_activates() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        let first = pipeline.process_frame_with_logo(&frame, SceneType::Freestyle, 1.0);
        let second = pipeline.process_frame_with_logo(&frame, SceneType::Freestyle, 2.0);

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
            pipeline.process_frame_with_logo(&frame, SceneType::Freestyle, idx as f64);
        }
        let output = pipeline.process_frame_with_logo(&frame, SceneType::Unknown, 10.0);

        assert!(output.is_song_select);
        assert!(output.state.context.is_none());
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
                bgra[idx] = pixel[2]; // B
                bgra[idx + 1] = pixel[1]; // G
                bgra[idx + 2] = pixel[0]; // R
                bgra[idx + 3] = pixel[3]; // A
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
                final_scene = pipeline.last_logo_scene;
            }
            println!("IMAGE: {} -> Detected Scene: {:?}", img_name, final_scene);

            // 2. Build ROI name list for current scene
            let mut roi_names = vec!["logo".to_string()];
            if let Some(cfg) = pipeline.rois.config.scenes.get(&final_scene) {
                for name in cfg.rois.keys() {
                    roi_names.push(name.clone());
                }
            }
            if final_scene == SceneType::Freestyle
                || final_scene == SceneType::OpenMatch
                || final_scene == SceneType::LadderMatch
            {
                for diff in ["NM", "HD", "MX", "SC"] {
                    roi_names.push(format!("diff_panel_{}", diff));
                }
            }

            // 3. Crop and save each ROI
            for roi_name in roi_names {
                let roi_rect = if roi_name.starts_with("diff_panel_") {
                    let diff_name = roi_name.strip_prefix("diff_panel_").unwrap();
                    pipeline
                        .rois
                        .get_diff_panel_roi_for_scene(diff_name, final_scene)
                } else {
                    pipeline.rois.get_roi_for_scene(&roi_name, final_scene)
                };

                let Some(roi) = roi_rect else {
                    continue;
                };

                let Some(cropped) = crop_roi(&frame, roi) else {
                    continue;
                };

                let mut rgba = cropped.bgra.clone();
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
