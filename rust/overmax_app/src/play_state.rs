use crate::frame_utils::region_mean_bgr;
use crate::roi::RoiManager;
use crate::screen_capture::CapturedFrame;
use overmax_core::GameSessionState;
use std::collections::VecDeque;

const BTN_MODE_MAX_DIST: f32 = 60.0;
const DIFF_MIN_BRIGHTNESS: f32 = 45.0;
const DIFF_CONFIDENT_MARGIN: f32 = 15.0;
const DIFFICULTIES: [&str; 4] = ["NM", "HD", "MX", "SC"];

#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    song_id: Option<u32>,
    mode: Option<String>,
    diff: Option<String>,
    is_max_combo: bool,
}

pub struct PlayStateDetector {
    history_size: usize,
    history: VecDeque<Option<RawPlayState>>,
    last_stable_state: Option<GameSessionState>,
}

impl PlayStateDetector {
    pub fn new(history_size: usize) -> Self {
        Self {
            history_size: history_size.max(1),
            history: VecDeque::new(),
            last_stable_state: None,
        }
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.last_stable_state = None;
    }

    pub fn detect(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<u32>,
    ) -> GameSessionState {
        let mode = detect_button_mode(frame, rois);
        let (diff, confident) = detect_difficulty(frame, rois);
        let is_max_combo = detect_max_combo(frame, rois);
        let raw = RawPlayState { song_id, mode, diff, is_max_combo };
        self.push_raw(raw.clone(), confident);

        if let Some(stable) = self.stable_raw() {
            let state = stable_state(stable);
            self.last_stable_state = Some(state.clone());
            return state;
        }

        GameSessionState {
            song_id: raw.song_id,
            mode: raw.mode,
            diff: raw.diff,
            is_stable: false,
            is_max_combo: raw.is_max_combo,
            rate: None,
        }
    }

    fn push_raw(&mut self, raw: RawPlayState, confident: bool) {
        if self.history.len() == self.history_size {
            self.history.pop_front();
        }
        let valid = raw.song_id.is_some() && raw.mode.is_some() && raw.diff.is_some() && confident;
        self.history.push_back(valid.then_some(raw));
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

pub fn detect_button_mode(frame: &CapturedFrame, rois: &RoiManager) -> Option<String> {
    let roi = rois.get_roi("btn_mode")?;
    let mean = region_mean_bgr(frame, roi);
    let mut best = (None, f32::INFINITY);
    for (mode, colors) in button_colors() {
        for color in colors {
            let dist = color_dist(mean, *color);
            if dist < best.1 {
                best = (Some(mode.to_string()), dist);
            }
        }
    }
    (best.1 <= BTN_MODE_MAX_DIST).then_some(best.0?).map(String::from)
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
    (Some(best.to_string()), max_bright - second >= DIFF_CONFIDENT_MARGIN)
}

pub fn detect_max_combo(frame: &CapturedFrame, rois: &RoiManager) -> bool {
    let Some(roi) = rois.get_roi("max_combo_badge") else {
        return false;
    };
    let (b, g, r) = region_mean_bgr(frame, roi);
    (f32::from(b) + f32::from(g) + f32::from(r)) / 3.0 >= 160.0
}

fn stable_state(raw: &RawPlayState) -> GameSessionState {
    GameSessionState {
        song_id: raw.song_id,
        mode: raw.mode.clone(),
        diff: raw.diff.clone(),
        is_stable: true,
        is_max_combo: raw.is_max_combo,
        rate: None,
    }
}

fn button_colors() -> [(&'static str, &'static [(u8, u8, u8)]); 4] {
    [
        ("4B", &[(0x55, 0x4F, 0x2D), (0x5A, 0x47, 0x0C)]),
        ("5B", &[(0xC6, 0xA9, 0x44)]),
        ("6B", &[(0x30, 0x94, 0xED)]),
        ("8B", &[(0x31, 0x14, 0x1D)]),
    ]
}

fn color_dist(left: (u8, u8, u8), right: (u8, u8, u8)) -> f32 {
    let db = f32::from(left.0) - f32::from(right.0);
    let dg = f32::from(left.1) - f32::from(right.1);
    let dr = f32::from(left.2) - f32::from(right.2);
    (db * db + dg * dg + dr * dr).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{detect_button_mode, PlayStateDetector};
    use crate::roi::RoiManager;
    use crate::screen_capture::CapturedFrame;

    #[test]
    fn detects_button_mode_from_reference_color() {
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, (0x55, 0x4F, 0x2D));
        let rois = RoiManager::new(1920, 1080);
        assert_eq!(detect_button_mode(&frame, &rois), Some("4B".to_string()));
    }

    #[test]
    fn marks_state_stable_after_repeated_valid_frames() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, (0x55, 0x4F, 0x2D));
        paint_rect(&mut frame, 98, 488, 208, 516, (220, 220, 220));
        let rois = RoiManager::new(1920, 1080);

        assert!(!detector.detect(&frame, &rois, Some(7)).is_stable);
        assert!(!detector.detect(&frame, &rois, Some(7)).is_stable);
        assert!(detector.detect(&frame, &rois, Some(7)).is_stable);
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    fn paint_rect(frame: &mut CapturedFrame, x1: i32, y1: i32, x2: i32, y2: i32, bgr: (u8, u8, u8)) {
        for y in y1..y2 {
            for x in x1..x2 {
                let idx = ((y * frame.width + x) * 4) as usize;
                frame.bgra[idx] = bgr.0;
                frame.bgra[idx + 1] = bgr.1;
                frame.bgra[idx + 2] = bgr.2;
            }
        }
    }
}
