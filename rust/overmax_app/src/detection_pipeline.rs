use crate::frame_utils::{crop_roi, make_thumbnail, thumbnail_changed};
use crate::hysteresis::HysteresisBuffer;
use crate::ocr_engine::OcrDetector;
use crate::play_state::PlayStateDetector;
use crate::roi::RoiManager;
use crate::screen_capture::CapturedFrame;
use overmax_core::GameSessionState;
use overmax_data::ImageIndexDb;

const JACKET_MATCH_INTERVAL: f64 = 0.8;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const LOGO_OCR_COOLDOWN_SEC: f64 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct DetectionOutput {
    pub is_song_select: bool,
    pub is_leaving: bool,
    pub confidence: f32,
    pub state: GameSessionState,
}

pub struct DetectionPipeline {
    image_db: ImageIndexDb,
    rois: RoiManager,
    hysteresis: HysteresisBuffer,
    play_state: PlayStateDetector,
    ocr: OcrDetector,
    current_song_id: Option<u32>,
    last_logo_ocr_ts: f64,
    last_logo_ocr_ok: bool,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
}

impl DetectionPipeline {
    pub fn new(image_db: ImageIndexDb) -> Self {
        Self {
            image_db,
            rois: RoiManager::new(1920, 1080),
            hysteresis: HysteresisBuffer::new(7, 0.6, 3, 0.35, 7),
            play_state: PlayStateDetector::new(3),
            ocr: OcrDetector::new(),
            current_song_id: None,
            last_logo_ocr_ts: 0.0,
            last_logo_ocr_ok: false,
            last_jacket_ts: 0.0,
            last_jacket_match_ts: 0.0,
            last_jacket_thumb: None,
        }
    }

    pub fn ocr_available(&self) -> bool {
        self.ocr.is_available()
    }

    pub fn process_frame(&mut self, frame: &CapturedFrame, now: f64) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);
        let logo_detected = self.detect_logo_if_due(frame, now);
        self.process_frame_with_logo(frame, logo_detected, now)
    }

    pub fn process_frame_with_logo(
        &mut self,
        frame: &CapturedFrame,
        logo_detected: bool,
        now: f64,
    ) -> DetectionOutput {
        self.rois.update_window_size(frame.width, frame.height);
        let (is_song_select, is_leaving, confidence) = self.hysteresis.update(logo_detected);

        if !is_song_select {
            self.reset_on_screen_exit();
            return self.output(false, is_leaving, confidence, GameSessionState::detecting());
        }

        if is_leaving {
            return self.output(true, true, confidence, GameSessionState::detecting());
        }

        self.update_song_id_from_jacket(frame, now);
        let state = self
            .play_state
            .detect(frame, &self.rois, self.current_song_id, &self.ocr);
        self.output(true, false, confidence, state)
    }

    fn detect_logo_if_due(&mut self, frame: &CapturedFrame, now: f64) -> bool {
        if now - self.last_logo_ocr_ts < LOGO_OCR_COOLDOWN_SEC {
            return self.last_logo_ocr_ok;
        }
        let Some(logo) = self.rois.get_roi("logo").and_then(|roi| crop_roi(frame, roi)) else {
            self.last_logo_ocr_ok = false;
            return false;
        };
        self.last_logo_ocr_ok = self.ocr.detect_logo(&logo).0;
        self.last_logo_ocr_ts = now;
        self.last_logo_ocr_ok
    }

    fn update_song_id_from_jacket(&mut self, frame: &CapturedFrame, now: f64) {
        if !self.should_match_jacket(now) {
            return;
        }
        self.last_jacket_ts = now;
        let Some(jacket) = self.rois.get_roi("jacket").and_then(|roi| crop_roi(frame, roi)) else {
            return;
        };
        let Some(thumb) = make_thumbnail(&jacket) else {
            return;
        };
        let image_changed = thumbnail_changed(
            &thumb,
            self.last_jacket_thumb.as_deref(),
            JACKET_CHANGE_THRESHOLD,
        );
        let force_recheck = now - self.last_jacket_match_ts >= JACKET_FORCE_RECHECK_SEC;
        if !(image_changed || force_recheck) {
            return;
        }

        self.last_jacket_thumb = Some(thumb);
        self.last_jacket_match_ts = now;
        if image_changed {
            self.current_song_id = None;
        }
        self.current_song_id = self.search_song_id_from_jacket(&jacket);
    }

    fn should_match_jacket(&self, now: f64) -> bool {
        self.image_db.is_ready() && now - self.last_jacket_ts >= JACKET_MATCH_INTERVAL
    }

    fn search_song_id_from_jacket(&self, jacket: &crate::frame_utils::ImageRegion) -> Option<u32> {
        self.image_db
            .search(&jacket.bgra, jacket.width as usize, jacket.height as usize, 4)
            .and_then(|result| result.image_id.parse::<u32>().ok())
    }

    fn reset_on_screen_exit(&mut self) {
        self.current_song_id = None;
        self.play_state.reset();
    }

    fn output(
        &self,
        is_song_select: bool,
        is_leaving: bool,
        confidence: f32,
        state: GameSessionState,
    ) -> DetectionOutput {
        DetectionOutput { is_song_select, is_leaving, confidence, state }
    }
}

#[cfg(test)]
mod tests {
    use super::DetectionPipeline;
    use crate::screen_capture::CapturedFrame;
    use overmax_data::ImageIndexDb;

    #[test]
    fn stays_detecting_until_hysteresis_activates() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();

        let first = pipeline.process_frame_with_logo(&frame, true, 1.0);
        let second = pipeline.process_frame_with_logo(&frame, true, 2.0);
        let third = pipeline.process_frame_with_logo(&frame, true, 3.0);

        assert!(!first.is_song_select);
        assert!(!second.is_song_select);
        assert!(third.is_song_select);
    }

    #[test]
    fn resets_state_when_song_select_is_lost() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));
        let frame = blank_frame();

        for idx in 0..3 {
            pipeline.process_frame_with_logo(&frame, true, idx as f64);
        }
        let output = pipeline.process_frame_with_logo(&frame, false, 10.0);

        assert!(output.is_song_select);
        assert!(output.state.song_id.is_none());
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }
}
