//! Gameplay/Paused pixel templates and matching, integrated with detector templates.
use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::ImageView;
use crate::detector::roi::RoiManager;
use overmax_core::SceneType;

const TITLE_MIN: f64 = 0.95;
const INTERIOR_RANGE_MIN: u8 = 15;
const INTERIOR_STEP: usize = 32;
const INTERIOR_ROWS: usize = 9;
const INTERIOR_ROWS_MIN: usize = 7;

pub(crate) const PAUSE_TITLE: &[&[u8]] = &[
    include_bytes!("gameplay_scene/pause_title-play-092.gray"),
    include_bytes!("gameplay_scene/pause_title-play-115.gray"),
];

pub(crate) fn supports_frame(frame: &CapturedFrame) -> bool {
    let expected = usize::try_from(frame.width)
        .ok()
        .zip(usize::try_from(frame.height).ok())
        .and_then(|(w, h)| w.checked_mul(h)?.checked_mul(4));
    expected == Some(frame.bgra.len())
        && matches!((frame.width, frame.height), (512, 512) | (1920, 1080))
}

pub(crate) fn read_scene(frame: &CapturedFrame, rois: &RoiManager) -> SceneType {
    if !supports_frame(frame) {
        return SceneType::Unknown;
    }
    let Some(pause_title) = roi(frame, rois, "pause_title") else {
        return SceneType::Unknown;
    };
    if PAUSE_TITLE
        .iter()
        .any(|template| title_matches(&pause_title, template))
    {
        return SceneType::Paused;
    }
    for (left, right) in [
        ("gp_center_left", "gp_center_right"),
        ("gp_left_left", "gp_left_right"),
        ("gp_right_left", "gp_right_right"),
    ] {
        let left = roi(frame, rois, left);
        let right = roi(frame, rois, right);
        if left.as_ref().is_some_and(interior_stripe) && right.as_ref().is_some_and(interior_stripe)
        {
            return SceneType::Gameplay;
        }
    }
    SceneType::Unknown
}

fn roi<'a>(frame: &'a CapturedFrame, rois: &RoiManager, name: &str) -> Option<ImageView<'a>> {
    rois.get_roi_for_scene(name, SceneType::Freestyle)?
        .crop(frame)
}

fn title_matches(crop: &ImageView<'_>, template: &[u8]) -> bool {
    if crop.width * crop.height != template.len() {
        return false;
    }
    let (mut sum, mut square, mut dot) = (0_i64, 0_i64, 0_i64);
    let (mut index, mut expected_sum, mut expected_square) = (0_i64, 0_i64, 0_i64);
    for y in 0..crop.height {
        for pixel in crop.row(y).chunks_exact(4) {
            let x = i64::from(
                overmax_cv::Bgr::from_bgra_slice(pixel).luma(overmax_cv::LumaMethod::Weighted),
            );
            let t = i64::from(template[index as usize]);
            sum += x;
            square += x * x;
            dot += x * t;
            expected_sum += t;
            expected_square += t * t;
            index += 1;
        }
    }
    let n = index;
    let variance = n * square - sum * sum;
    let centered = n * expected_square - expected_sum * expected_sum;
    if variance < n * n || centered < n * n {
        return false;
    }
    (n * dot - sum * expected_sum) as f64 / ((variance as f64) * centered as f64).sqrt()
        >= TITLE_MIN
}

fn interior_stripe(crop: &ImageView<'_>) -> bool {
    let mut supporting = 0;
    for row in 0..INTERIOR_ROWS {
        let mut low = [u8::MAX; 3];
        let mut high = [u8::MIN; 3];
        for pixel in crop.row(row * INTERIOR_STEP).chunks_exact(4) {
            for c in 0..3 {
                low[c] = low[c].min(pixel[c]);
                high[c] = high[c].max(pixel[c]);
            }
        }
        if (0..3).any(|c| high[c] - low[c] >= INTERIOR_RANGE_MIN) {
            supporting += 1;
        }
        if supporting >= INTERIOR_ROWS_MIN {
            return true;
        }
        if supporting + INTERIOR_ROWS - row - 1 < INTERIOR_ROWS_MIN {
            return false;
        }
    }
    false
}
