use crate::frame_utils::{crop_roi, make_thumbnail, thumbnail_changed};
use crate::hysteresis::HysteresisBuffer;
use crate::ocr_engine::OcrDetector;
use crate::play_state::PlayStateDetector;
use crate::roi::RoiManager;
use crate::screen_capture::CapturedFrame;
use overmax_core::GameSessionState;
use overmax_data::ImageIndexDb;

const JACKET_MATCH_INTERVAL: f64 = 0.0;
const JACKET_CHANGE_THRESHOLD: f32 = 2.5;
const JACKET_FORCE_RECHECK_SEC: f64 = 2.0;
const JACKET_STABLE_HITS: u8 = 2;
const LOGO_OCR_COOLDOWN_SEC: f64 = 1.0;

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
    Pending { song_id: u32, similarity: f32 },
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
    last_logo_ocr_ok: bool,
    last_jacket_ts: f64,
    last_jacket_match_ts: f64,
    last_jacket_thumb: Option<Vec<u8>>,
    pending_jacket_match: Option<PendingJacketMatch>,
}

#[derive(Clone, Debug)]
struct PendingJacketMatch {
    song_id: u32,
    hits: u8,
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
            pending_jacket_match: None,
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
            return self.output(
                logo_detected,
                false,
                is_leaving,
                confidence,
                GameSessionState::detecting(),
                JacketMatchStatus::NotSongSelect,
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
            );
        }

        let jacket_status = self.update_song_id_from_jacket(frame, now);
        let state = self
            .play_state
            .detect(frame, &self.rois, self.current_song_id, &self.ocr);
        self.output(logo_detected, true, false, confidence, state, jacket_status)
    }

    fn detect_logo_if_due(&mut self, frame: &CapturedFrame, now: f64) -> bool {
        if now - self.last_logo_ocr_ts < LOGO_OCR_COOLDOWN_SEC {
            return self.last_logo_ocr_ok;
        }
        let Some(logo) = self
            .rois
            .get_roi("logo")
            .and_then(|roi| crop_roi(frame, roi))
        else {
            self.last_logo_ocr_ok = false;
            return false;
        };
        self.last_logo_ocr_ok = self.ocr.detect_logo(&logo).0;
        self.last_logo_ocr_ts = now;
        self.last_logo_ocr_ok
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
            self.pending_jacket_match = None;
            return JacketMatchStatus::NoMatch;
        };
        match result.image_id.parse::<u32>() {
            Ok(song_id) => self.stabilize_jacket_match(song_id, result.similarity),
            Err(_) => {
                self.pending_jacket_match = None;
                JacketMatchStatus::InvalidId {
                    image_id: result.image_id,
                    similarity: result.similarity,
                }
            }
        }
    }

    fn stabilize_jacket_match(&mut self, song_id: u32, similarity: f32) -> JacketMatchStatus {
        if self.current_song_id == Some(song_id) {
            self.pending_jacket_match = None;
            return JacketMatchStatus::Matched {
                song_id,
                similarity,
            };
        }

        let hits = self.next_pending_hits(song_id);
        if hits < JACKET_STABLE_HITS {
            return JacketMatchStatus::Pending {
                song_id,
                similarity,
            };
        }

        self.current_song_id = Some(song_id);
        self.pending_jacket_match = None;
        JacketMatchStatus::Matched {
            song_id,
            similarity,
        }
    }

    fn next_pending_hits(&mut self, song_id: u32) -> u8 {
        let hits = self
            .pending_jacket_match
            .as_ref()
            .filter(|pending| pending.song_id == song_id)
            .map_or(1, |pending| pending.hits.saturating_add(1));
        self.pending_jacket_match = Some(PendingJacketMatch { song_id, hits });
        hits
    }

    fn reset_on_screen_exit(&mut self) {
        self.current_song_id = None;
        self.pending_jacket_match = None;
        self.play_state.reset();
    }

    fn should_match_jacket(&self, image_changed: bool, now: f64) -> bool {
        image_changed
            || self.pending_jacket_match.is_some()
            || now - self.last_jacket_match_ts >= JACKET_FORCE_RECHECK_SEC
    }

    fn output(
        &self,
        logo_detected: bool,
        is_song_select: bool,
        is_leaving: bool,
        confidence: f32,
        state: GameSessionState,
        jacket_status: JacketMatchStatus,
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

        let first = pipeline.process_frame_with_logo(&frame, true, 1.0);
        let second = pipeline.process_frame_with_logo(&frame, true, 2.0);
        let third = pipeline.process_frame_with_logo(&frame, true, 3.0);

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

        for idx in 0..3 {
            pipeline.process_frame_with_logo(&frame, true, idx as f64);
        }
        let output = pipeline.process_frame_with_logo(&frame, false, 10.0);

        assert!(output.is_song_select);
        assert!(output.state.song_id.is_none());
    }

    #[test]
    fn jacket_match_requires_repeated_candidate_before_commit() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));

        let first = pipeline.stabilize_jacket_match(7, 0.8);
        let second = pipeline.stabilize_jacket_match(8, 0.8);
        let third = pipeline.stabilize_jacket_match(8, 0.8);

        assert_eq!(
            first,
            JacketMatchStatus::Pending {
                song_id: 7,
                similarity: 0.8
            }
        );
        assert_eq!(
            second,
            JacketMatchStatus::Pending {
                song_id: 8,
                similarity: 0.8
            }
        );
        assert_eq!(
            third,
            JacketMatchStatus::Matched {
                song_id: 8,
                similarity: 0.8
            }
        );
        assert_eq!(pipeline.current_song_id, Some(8));
    }

    #[test]
    fn pending_jacket_match_rechecks_even_when_thumbnail_is_unchanged() {
        let mut pipeline = DetectionPipeline::new(ImageIndexDb::new("missing.db", 0.6));

        pipeline.pending_jacket_match = Some(super::PendingJacketMatch {
            song_id: 7,
            hits: 1,
        });
        pipeline.last_jacket_match_ts = 1.0;

        assert!(pipeline.should_match_jacket(false, 1.12));
    }

    #[test]
    fn jacket_match_uses_active_frame_cadence() {
        assert_eq!(super::JACKET_MATCH_INTERVAL, 0.0);
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }
}
