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
}

impl DetectionPipeline {
    pub fn new(image_db: ImageIndexDb) -> Self {
        Self {
            image_db,
            rois: RoiManager::new(1920, 1080),
            hysteresis: HysteresisBuffer::new(5, 0.6, 3, 0.4, 3),
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

        let jacket_status = self.update_song_id_from_jacket(frame, now);
        let (state, telemetry) = self
            .play_state
            .detect(frame, &self.rois, self.current_song_id, &self.ocr, now);
        
        self.output(logo_detected, true, false, confidence, state, jacket_status, telemetry)
    }

    fn detect_logo_if_due(&mut self, frame: &CapturedFrame, now: f64) -> Option<SceneType> {
        let cooldown = 0.3;
        if now - self.last_logo_ocr_ts < cooldown {
            return None;
        }

        let Some(logo) = self
            .rois
            .get_roi("logo")
            .and_then(|roi| crop_roi(frame, roi))
        else {
            println!("    [detect_logo_if_due] logo crop failed! now={}", now);
            self.last_logo_scene = SceneType::Unknown;
            self.last_logo_ocr_ts = now;
            return Some(SceneType::Unknown);
        };
        
        let (scene, raw_text, _label) = self.ocr.detect_logo(&logo);
        println!("    [detect_logo_if_due] now={}, crop_size={}x{}, OCR raw='{}', scene={:?}",
                 now, logo.width, logo.height, raw_text, scene);

        let mut scene_res = scene;
        
        // If logo is Unknown, check the bottom guide bar to see if it's OpenMatch 3+ result screen
        if scene_res == SceneType::Unknown {
            if let Some(bottom_roi) = self.rois.get_roi("bottom_guide") {
                if let Some(bottom_img) = crop_roi(frame, bottom_roi) {
                    if self.ocr.detect_bottom_guide_space(&bottom_img) {
                        scene_res = SceneType::ResultOpen3;
                    }
                }
            }
        }
        
        // If still Unknown, check the bottom part of the screen as a fallback (y starting from 35%)
        if scene_res == SceneType::Unknown {
            let bottom_half_roi = crate::roi::RoiRect {
                x1: 0,
                y1: frame.height * 35 / 100,
                x2: frame.width,
                y2: frame.height,
            };
            if let Some(bottom_half_img) = crop_roi(frame, bottom_half_roi) {
                if let Some((text, rate_x_ratio)) = self.ocr.recognize_bottom_half_with_rate_x(&bottom_half_img) {
                    if let Some(s) = self.ocr.classify_fallback_scene(&text, rate_x_ratio) {
                        println!("    [detect_logo_if_due] fallback match: scene={:?}, text='{}', rate_x_ratio={:?}", s, text.trim(), rate_x_ratio);
                        scene_res = s;
                    }
                }
            }
        }
        
        let is_detected_result = matches!(
            scene_res,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );

        if is_detected_result {
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
        let Some(result) = self.image_db.search(
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
        image_changed || now - self.last_jacket_match_ts >= JACKET_FORCE_RECHECK_SEC
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
        let third = pipeline.process_frame_with_logo(&frame, SceneType::Freestyle, 3.0);

        assert!(!first.is_song_select);
        assert_eq!(first.jacket_status, JacketMatchStatus::NotSongSelect);
        assert!(!second.is_song_select);
        assert!(third.is_song_select);
        assert_eq!(third.jacket_status, JacketMatchStatus::DbNotReady);
    }

    #[test]
    fn resets_state_when_song_select_is_lost() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();
        use overmax_core::SceneType;

        for idx in 0..3 {
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
        use std::path::Path;
        use image::GenericImageView;
        use overmax_core::SceneType;
        use crate::frame_utils::crop_roi;

        let scratch_dir = Path::new(r"C:\Users\jeongwoong\dev\overmax\scratch");
        let images = [
            "6.png", "7.png", "8.png",
            "dc_1.png", "dc_2.png", "dc_3.png", "dc_4.png", "dc_5.jpg", "new_test.jpg",
            "freestyle.png", "openmatch.png", "openmatch_2p.png"
        ];

        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));

        for img_name in &images {
            let path = scratch_dir.join(img_name);
            if !path.exists() {
                println!("{}: Not found", img_name);
                continue;
            }

            let img = image::open(&path).expect("Failed to open image");
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
            let logo_roi = pipeline.rois.get_roi("logo").unwrap();
            let logo_img = crop_roi(&frame, logo_roi).unwrap();
            
            // Public logo OCR attempt
            let (logo_scene, logo_txt, _logo_label) = pipeline.ocr.detect_logo(&logo_img);

            // Entire screen OCR to see what text exists
            let entire_region = crate::frame_utils::ImageRegion {
                bgra: frame.bgra.clone(),
                width: w as i32,
                height: h as i32,
            };
            let entire_txt = pipeline.ocr.recognize_text_color(&entire_region).unwrap_or_default();

            // Public bottom guide OCR attempt (if logo is Unknown)
            let mut has_space = false;
            let mut bottom_text_opt = None;
            if let Some(bottom_roi) = pipeline.rois.get_roi("bottom_guide") {
                if let Some(bottom_img) = crop_roi(&frame, bottom_roi) {
                    has_space = pipeline.ocr.detect_bottom_guide_space(&bottom_img);
                    bottom_text_opt = pipeline.ocr.recognize_text_color(&bottom_img);
                }
            }

            println!("==================================================");
            println!("IMAGE: {}", img_name);
            println!("Resolution: {}x{}", w, h);
            println!("OCR Logo Final Match:        '{}' (Scene={:?})", logo_txt, logo_scene);
            println!("OCR Bottom Space Detected: {} (Text={:?})", has_space, bottom_text_opt);
            println!("Entire Screen OCR Text:\n{}", entire_txt);

            // Run detection
            pipeline.result_scene_streak = 0;
            pipeline.last_detected_result_scene = SceneType::Unknown;
            pipeline.last_logo_scene = SceneType::Unknown;
            pipeline.last_logo_ocr_ts = 0.0;
            pipeline.hysteresis = HysteresisBuffer::new(5, 0.6, 3, 0.4, 3);
            pipeline.play_state.reset();

            let mut final_out = None;
            for step in 0..10 {
                let t = step as f64 * 0.4;
                let out = pipeline.detect(&frame, t);
                println!("  Step {}: scene_out={:?}, logo_det={}, is_song_sel={}, streak={}, last_det_res={:?}, last_logo={:?}",
                         step,
                         pipeline.last_logo_scene,
                         out.logo_detected,
                         out.is_song_select,
                         pipeline.result_scene_streak,
                         pipeline.last_detected_result_scene,
                         pipeline.last_logo_scene);
                final_out = Some(out);
            }

            let out = final_out.unwrap();
            println!("Pipeline Detected Scene: {:?}", pipeline.last_logo_scene);
            println!("Is Song Select: {}", out.is_song_select);
            println!("Jacket status: {:?}", out.jacket_status);
            println!("PlayContext: {:?}", out.state.context);
            println!("Is Stable: {}", out.state.is_stable);
        }
    }
}
