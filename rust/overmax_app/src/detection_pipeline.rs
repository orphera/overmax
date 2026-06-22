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
        
        let logo_detected = scene != SceneType::Unknown;
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
        
        let logo_detected = self.last_logo_scene != SceneType::Unknown;
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
            self.last_logo_scene = SceneType::Unknown;
            self.last_logo_ocr_ts = now;
            return Some(SceneType::Unknown);
        };
        
        let mut scene = self.ocr.detect_logo(&logo).0;
        
        // If logo is Unknown, check the bottom guide bar to see if it's OpenMatch 3+ result screen
        if scene == SceneType::Unknown {
            if let Some(bottom_roi) = self.rois.get_roi("bottom_guide") {
                if let Some(bottom_img) = crop_roi(frame, bottom_roi) {
                    if self.ocr.detect_bottom_guide_space(&bottom_img) {
                        scene = SceneType::ResultOpen3;
                    }
                }
            }
        }
        
        self.last_logo_scene = scene;
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
}
