//! Gear interior and PAUSE title reader. No OCR, scaling, retries or per-frame allocation.

use crate::capture::frame_utils::ImageView;
use crate::detector::templates::gameplay_scene as templates;
use overmax_core::SceneType;
use std::sync::OnceLock;

const TITLE_MIN: f64 = 0.95;
const INTERIOR_RANGE_MIN: u8 = 15;
const INTERIOR_STEP: usize = 32;
const INTERIOR_ROWS: usize = 9;
const INTERIOR_ROWS_MIN: usize = 7;
// CENTER, LEFT (-600), RIGHT (+640); settings preview (+480) is not gameplay.
const STRUCTURE_STARTS: [usize; 3] = [0, 2, 4];

pub(super) struct PixelReader {
    gray: Vec<u8>,
    title: &'static [ShapeTemplate],
}

impl PixelReader {
    pub fn new() -> Self {
        static TITLE: OnceLock<Vec<ShapeTemplate>> = OnceLock::new();
        Self {
            gray: Vec::with_capacity(148 * 28),
            title: TITLE.get_or_init(|| {
                templates::PAUSE_TITLE
                    .iter()
                    .map(|bytes| ShapeTemplate::new(bytes))
                    .collect()
            }),
        }
    }

    pub fn read(&mut self, crops: &[ImageView<'_>; 7]) -> SceneType {
        gray_crops(&crops[6..7], &mut self.gray);
        if self.title.iter().any(|t| t.hit(&self.gray)) {
            return SceneType::Paused;
        }
        // Gear-visible loading transitions count as Gameplay. Scene confirmation
        // remains in the pipeline, using only actual inspections as observations.
        for start in STRUCTURE_STARTS {
            if interior_stripe(&crops[start]) && interior_stripe(&crops[start + 1]) {
                return SceneType::Gameplay;
            }
        }
        SceneType::Unknown
    }
}

fn gray(pixel: &[u8]) -> u8 {
    ((77 * u32::from(pixel[2]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[0]) + 128) >> 8)
        as u8
}

fn gray_crops(crops: &[ImageView<'_>], out: &mut Vec<u8>) {
    out.clear();
    for crop in crops {
        for y in 0..crop.height {
            out.extend(crop.row(y).chunks_exact(4).map(gray));
        }
    }
}

struct ShapeTemplate {
    bytes: &'static [u8],
    sum: i64,
    centered_square: i64,
}

impl ShapeTemplate {
    fn new(bytes: &'static [u8]) -> Self {
        let sum: i64 = bytes.iter().map(|&v| i64::from(v)).sum();
        let square: i64 = bytes.iter().map(|&v| i64::from(v).pow(2)).sum();
        Self {
            bytes,
            sum,
            centered_square: bytes.len() as i64 * square - sum * sum,
        }
    }

    fn hit(&self, pixels: &[u8]) -> bool {
        let (mut sum, mut square, mut dot) = (0_i64, 0_i64, 0_i64);
        for (&x, &y) in pixels.iter().zip(self.bytes) {
            let x = i64::from(x);
            sum += x;
            square += x * x;
            dot += x * i64::from(y);
        }
        let n = pixels.len() as i64;
        let variance = n * square - sum * sum;
        if pixels.len() != self.bytes.len() || variance < n * n || self.centered_square < n * n {
            return false;
        }
        let correlation = (n * dot - sum * self.sum) as f64
            / ((variance as f64) * self.centered_square as f64).sqrt();
        correlation >= TITLE_MIN
    }
}

fn interior_stripe(crop: &ImageView<'_>) -> bool {
    // A few strong letters/effects must not compensate for an absent frame.
    // Count supporting rows instead of averaging contrast or selecting a peak.
    let mut supporting = 0;
    for row in 0..INTERIOR_ROWS {
        let mut low = [u8::MAX; 3];
        let mut high = [u8::MIN; 3];
        for pixel in crop.row(row * INTERIOR_STEP).chunks_exact(4) {
            for channel in 0..3 {
                low[channel] = low[channel].min(pixel[channel]);
                high[channel] = high[channel].max(pixel[channel]);
            }
        }
        if (0..3).any(|channel| high[channel] - low[channel] >= INTERIOR_RANGE_MIN) {
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
