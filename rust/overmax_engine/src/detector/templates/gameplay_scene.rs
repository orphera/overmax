//! Gameplay/Paused pixel templates and matching, integrated with detector templates.
use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::ImageView;
use crate::detector::atlas_translator::AtlasTranslator;
use crate::detector::roi::RoiRect;
use overmax_core::SceneType;

const TITLE_MIN: f64 = 0.95;
const INTERIOR_RANGE_MIN: u8 = 15;
const INTERIOR_STEP: usize = 32;
const INTERIOR_ROWS: usize = 9;
const INTERIOR_ROWS_MIN: usize = 7;
const STRUCTURE_STARTS: [usize; 3] = [0, 2, 4];
const ATLAS_ROI_NAMES: [&str; 7] = [
    "gp_center_left",
    "gp_center_right",
    "gp_left_left",
    "gp_left_right",
    "gp_right_left",
    "gp_right_right",
    "pause_title",
];
const RECTS: [(i32, i32, i32, i32); 7] = [
    (702, 80, 7, 257),
    (1211, 80, 7, 257),
    (102, 80, 7, 257),
    (611, 80, 7, 257),
    (1342, 80, 7, 257),
    (1851, 80, 7, 257),
    (731, 177, 148, 28),
];
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

pub(crate) fn read_scene(frame: &CapturedFrame) -> SceneType {
    if !supports_frame(frame) {
        return SceneType::Unknown;
    }
    let crops = if (frame.width, frame.height) == (512, 512) {
        let Some(crops) = atlas_crops(frame) else {
            return SceneType::Unknown;
        };
        crops
    } else {
        RECTS.map(|(x, y, w, h)| {
            RoiRect {
                x1: x,
                y1: y,
                x2: x + w,
                y2: y + h,
            }
            .crop(frame)
            .expect("fixed ROIs fit a complete 1080p frame")
        })
    };
    if PAUSE_TITLE
        .iter()
        .any(|template| title_matches(&crops[6], template))
    {
        return SceneType::Paused;
    }
    for start in STRUCTURE_STARTS {
        if interior_stripe(&crops[start]) && interior_stripe(&crops[start + 1]) {
            return SceneType::Gameplay;
        }
    }
    SceneType::Unknown
}

fn atlas_crops<'a>(frame: &'a CapturedFrame) -> Option<[ImageView<'a>; 7]> {
    Some([
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[0], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[1], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[2], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[3], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[4], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[5], SceneType::Freestyle)?,
        AtlasTranslator::crop_roi(frame, ATLAS_ROI_NAMES[6], SceneType::Freestyle)?,
    ])
}

fn gray(pixel: &[u8]) -> u8 {
    ((77 * u32::from(pixel[2]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[0]) + 128) >> 8)
        as u8
}

fn title_matches(crop: &ImageView<'_>, template: &[u8]) -> bool {
    if crop.width * crop.height != template.len() {
        return false;
    }
    let (mut sum, mut square, mut dot) = (0_i64, 0_i64, 0_i64);
    let (mut index, mut expected_sum, mut expected_square) = (0_i64, 0_i64, 0_i64);
    for y in 0..crop.height {
        for pixel in crop.row(y).chunks_exact(4) {
            let x = i64::from(gray(pixel));
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
