use crate::frame_utils::{crop_roi, make_thumbnail, thumbnail_changed};
use crate::hysteresis::HysteresisBuffer;
use crate::ocr_engine::{OcrDetector, OcrTelemetry};
use crate::play_state::PlayStateDetector;
use crate::roi::RoiManager;
use crate::screen_capture::CapturedFrame;
use overmax_core::{GameSessionState, SceneType};
use overmax_data::ImageIndexDb;

const JACKET_MATCH_INTERVAL: f64 = 0.25;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const JACKET_FORCE_RECHECK_LONG_SEC: f64 = 30.0;

#[derive(Clone, Debug, PartialEq)]
pub struct DetectionOutput {
    pub logo_detected: bool,
    pub is_song_select: bool,
    pub is_leaving: bool,
    pub confidence: f32,
    pub state: GameSessionState,
    pub current_song_id: Option<u32>,
    pub image_db_ready: bool,
    pub jacket_status: JacketMatchStatus,
    pub game_rect: Option<crate::window_tracker::WindowRect>,
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
    Matched { song_id: u32, similarity: f32 },
}

pub struct DetectionPipeline {
    image_db: ImageIndexDb,
    jacket_matcher: overmax_data::JacketMatcher,
    rois: RoiManager,
    hysteresis: HysteresisBuffer,
    play_state: PlayStateDetector,
    ocr: OcrDetector,
    current_song_id: Option<u32>,
    last_logo_ocr_ts: f64,
    last_logo_scene: SceneType,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
    result_scene_streak: u32,
    last_detected_result_scene: SceneType,
    last_played_song_id: Option<u32>,
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
            last_played_song_id: None,
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

    pub fn process_frame_cached(
        &mut self,
        frame: &CapturedFrame,
        now: f64,
    ) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);
        
        let logo_detected = self.last_logo_scene != SceneType::Unknown && self.last_logo_scene != SceneType::Online;
        self.hysteresis.update(logo_detected);
        self.process_frame_shared(frame, logo_detected, now)
    }

    fn process_frame_shared(
        &mut self,
        frame: &CapturedFrame,
        logo_detected: bool,
        now: f64,
    ) -> DetectionOutput {
        let is_result = matches!(
            self.last_logo_scene,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );
        let is_song_select = self.hysteresis.is_active || is_result;
        let is_leaving = if is_result { false } else { self.hysteresis.is_leaving };
        let confidence = self.hysteresis.confidence;

        if !is_song_select {
            // 선곡 화면에서 플레이 진입(이탈) 시, 직전의 곡 ID를 플레이 이력으로 기록
            if self.hysteresis.is_active && self.current_song_id.is_some() {
                self.last_played_song_id = self.current_song_id;
                println!("    [process_frame_shared] Saved last_played_song_id={:?} upon gameplay entry", self.last_played_song_id);
            }
            // 결과창 상태를 완전히 빠져나가는 경우에도 플레이 이력 소멸
            if !is_result {
                self.last_played_song_id = None;
            }
            self.reset_on_screen_exit();
            return self.output(
                logo_detected,
                false,
                is_leaving,
                confidence,
                GameSessionState::detecting(),
                JacketMatchStatus::NotSongSelect,
                None,
            );
        }

        if is_leaving {
            // 선곡 화면에서 나갈 때도 플레이 이력 확보
            if self.current_song_id.is_some() {
                self.last_played_song_id = self.current_song_id;
                println!("    [process_frame_shared] Saved last_played_song_id={:?} upon leaving", self.last_played_song_id);
            }
            return self.output(
                logo_detected,
                true,
                true,
                confidence,
                GameSessionState::detecting(),
                JacketMatchStatus::Leaving,
                None,
            );
        }

        // 결과창에서 다시 선곡 화면으로 복귀하는 경우 플레이 이력 리셋
        if !is_result {
            self.last_played_song_id = None;
        }

        let jacket_status = self.update_song_id_from_jacket(frame, now);
        let (state, telemetry) = self
            .play_state
            .detect(frame, &self.rois, self.current_song_id, &self.ocr, now);
        
        self.output(logo_detected, true, false, confidence, state, jacket_status, telemetry)
    }

    fn detect_logo_if_due(&mut self, frame: &CapturedFrame, now: f64) -> Option<SceneType> {
        // 씬이 Unknown인 경우(진입 대기): 빠른 인식을 위해 0.3초 주기로 감시
        // 씬이 이미 확정된 경우(유지 중): CPU 소모 최소화를 위해 2.0초 주기로 완화 (이탈은 픽셀 매칭으로 즉시 처리되므로 반응성 무관)
        let cooldown = if self.last_logo_scene == SceneType::Unknown {
            0.3
        } else {
            2.0
        };
        
        if now - self.last_logo_ocr_ts < cooldown {
            return None;
        }

        let logo_roi = match self.rois.get_roi("logo") {
            Some(roi) => roi,
            None => return None,
        };

        let Some(logo) = crop_roi(frame, logo_roi) else {
            println!("    [detect_logo_if_due] logo crop failed! now={}", now);
            self.last_logo_scene = SceneType::Unknown;
            self.last_logo_ocr_ts = now;
            return Some(SceneType::Unknown);
        };
        
        let (scene, raw_text, _label) = self.ocr.detect_logo(&logo);
        println!("    [detect_logo_if_due] now={}, crop_size={}x{}, OCR raw='{}', scene={:?}",
                 now, logo.width, logo.height, raw_text, scene);

        let mut scene_res = scene;

        let is_prev_result = matches!(
            self.last_logo_scene,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );

        if scene_res == SceneType::Unknown {
            let mut is_result_candidate = false;
            let mut detected_badge_scene = None;

            // 1. Proactively probe the small bottom_guide ROI (lightweight)
            if let Some(bottom_roi) = self.rois.get_roi("bottom_guide") {
                if let Some(bottom_img) = crop_roi(frame, bottom_roi) {
                    if self.ocr.detect_bottom_guide_space(&bottom_img) {
                        is_result_candidate = true;
                    }
                }
            }

            // Fallback: If bottom guide didn't match, check mode_diff_badge as a fallback candidate detector.
            if !is_result_candidate {
                detected_badge_scene = self.check_open_match_badge(frame);
                if detected_badge_scene.is_some() {
                    is_result_candidate = true;
                }
            }

            // 2. Only if the candidate flag is set, classify the fallback scene.
            if is_result_candidate {
                let fallback_scene = if let Some(s) = detected_badge_scene {
                    s
                } else {
                    self.check_open_match_badge(frame).unwrap_or(SceneType::Unknown)
                };
                scene_res = fallback_scene;
                if scene_res != SceneType::Unknown {
                    self.rois.set_scene(scene_res); // Sync configurations
                }
            }
        }

        if scene_res == SceneType::Unknown && is_prev_result {
            let norm_logo = raw_text.to_uppercase();
            let has_logo_keyword = norm_logo.contains("BUTTON")
                || norm_logo.contains("TUNES")
                || norm_logo.contains("TIJNFS")
                || norm_logo.contains("TUNE");
            if has_logo_keyword {
                scene_res = self.last_logo_scene;
                println!("    [detect_logo_if_due] Lock-in: keeping previous result scene {:?} due to logo keyword match", scene_res);
            }
        }

        let is_detected_result = matches!(
            scene_res,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );

        if is_detected_result {
            // 플레이 이력(최근 플레이한 곡 ID)이 존재하는 경우에만 결과창 진입을 허용하여 COLLECTION 화면 등에서의 오인식 방지
            if self.last_played_song_id.is_none() {
                scene_res = SceneType::Unknown;
            }
        }

        let is_detected_result_final = matches!(
            scene_res,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );

        if is_detected_result_final {
            if scene_res == self.last_detected_result_scene {
                self.result_scene_streak += 1;
            } else {
                self.last_detected_result_scene = scene_res;
                self.result_scene_streak = 1;
            }

            if self.result_scene_streak >= 2 {
                self.last_logo_scene = scene_res;
            }
        } else {
            self.result_scene_streak = 0;
            self.last_detected_result_scene = SceneType::Unknown;
            self.last_logo_scene = scene_res;
        }

        self.last_logo_ocr_ts = now;
        Some(self.last_logo_scene)
    }

    fn check_open_match_badge(&self, frame: &CapturedFrame) -> Option<SceneType> {
        let open3_badge_roi = self.rois.get_roi_for_scene("mode_diff_badge", SceneType::ResultOpen3);
        let open2_badge_roi = self.rois.get_roi_for_scene("mode_diff_badge", SceneType::ResultOpen2);

        // 1. Fast Color-based OCR Pass
        let mut o3_m = false;
        let mut o2_m = false;

        if let Some(roi) = open3_badge_roi {
            if let Some(img) = crop_roi(frame, roi) {
                if let Some(txt) = self.ocr.recognize_text_color(&img) {
                    if self.ocr.contains_mode_keyword(&txt) {
                        o3_m = true;
                    }
                }
            }
        }

        if !o3_m {
            if let Some(roi) = open2_badge_roi {
                if let Some(img) = crop_roi(frame, roi) {
                    if let Some(txt) = self.ocr.recognize_text_color(&img) {
                        if self.ocr.contains_mode_keyword(&txt) {
                            o2_m = true;
                        }
                    }
                }
            }
        }

        if o3_m && !o2_m {
            return Some(SceneType::ResultOpen3);
        }
        if o2_m && !o3_m {
            return Some(SceneType::ResultOpen2);
        }

        // 2. Slow Multi-pass OCR Fallback (Only executed if fast color pass failed or was ambiguous)
        let mut o3_all = false;
        if let Some(roi) = open3_badge_roi {
            if let Some(img) = crop_roi(frame, roi) {
                if let Some(txt) = self.ocr.recognize_text_all_passes(&img) {
                    if self.ocr.contains_mode_keyword(&txt) {
                        o3_all = true;
                    }
                }
            }
        }

        let mut o2_all = false;
        if !o3_all {
            if let Some(roi) = open2_badge_roi {
                if let Some(img) = crop_roi(frame, roi) {
                    if let Some(txt) = self.ocr.recognize_text_all_passes(&img) {
                        if self.ocr.contains_mode_keyword(&txt) {
                            o2_all = true;
                        }
                    }
                }
            }
        }

        if o3_all && !o2_all {
            Some(SceneType::ResultOpen3)
        } else if o2_all && !o3_all {
            Some(SceneType::ResultOpen2)
        } else {
            None
        }
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
        jacket: &crate::frame_utils::ImageRegion,
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
        match result.image_id.parse::<u32>() {
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
        is_leaving: bool,
        confidence: f32,
        state: GameSessionState,
        jacket_status: JacketMatchStatus,
        ocr_telemetry: Option<OcrTelemetry>,
    ) -> DetectionOutput {
        DetectionOutput {
            logo_detected,
            is_song_select,
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

#[cfg(test)]
mod tests {
    use super::{DetectionPipeline, JacketMatchStatus};
    use crate::screen_capture::CapturedFrame;
    use crate::hysteresis::HysteresisBuffer;
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
        use image::GenericImageView;
        use overmax_core::SceneType;
        use crate::frame_utils::crop_roi;

        let scratch_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch");
        let images = [
            "hd_test_1.png", "hd_test_2.png", "hd_test_3.png", "hd_test_4.png", "hd_test_5.png",
            "hd_test_2p_1.png", "hd_test_2p_2.png"
        ];

        let db_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/image_index.db");
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

            let img = image::io::Reader::open(&path).expect("Failed to open file")
                .with_guessed_format().expect("Failed to guess format")
                .decode().expect("Failed to decode image");
            let (w, h) = img.dimensions();
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            
            for (x, y, pixel) in img.pixels() {
                let idx = ((y * w + x) * 4) as usize;
                bgra[idx] = pixel[2];     // B
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
            let mut roi_names = vec!["logo".to_string(), "bottom_guide".to_string()];
            if let Some(cfg) = pipeline.rois.config.scenes.get(&final_scene) {
                for name in cfg.rois.keys() {
                    roi_names.push(name.clone());
                }
            }
            if final_scene == SceneType::Freestyle || final_scene == SceneType::OpenMatch || final_scene == SceneType::LadderMatch {
                for diff in ["NM", "HD", "MX", "SC"] {
                    roi_names.push(format!("diff_panel_{}", diff));
                }
            }

            // 3. Crop and save each ROI
            for roi_name in roi_names {
                let roi_rect = if roi_name.starts_with("diff_panel_") {
                    let diff_name = roi_name.strip_prefix("diff_panel_").unwrap();
                    pipeline.rois.get_diff_panel_roi_for_scene(diff_name, final_scene)
                } else {
                    pipeline.rois.get_roi_for_scene(&roi_name, final_scene)
                };

                let Some(roi) = roi_rect else { continue; };

                let Some(cropped) = crop_roi(&frame, roi) else { continue; };

                let mut rgba = cropped.bgra.clone();
                for chunk in rgba.chunks_exact_mut(4) {
                    chunk.swap(0, 2); // BGR -> RGB
                }

                let out_filename = format!("{}_{}.png", img_name.strip_suffix(".png").unwrap_or(img_name), roi_name);
                let out_path = roi_dir.join(out_filename);
                image::save_buffer(
                    &out_path,
                    &rgba,
                    cropped.width as u32,
                    cropped.height as u32,
                    image::ColorType::Rgba8
                ).expect("Failed to save cropped image");
                println!("    Saved ROI '{}' to {:?}", roi_name, out_path);
            }
        }
    }
}
