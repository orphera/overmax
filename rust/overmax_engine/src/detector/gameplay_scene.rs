//! Pixel-only Gameplay/Paused candidate reader. Scene history belongs to the pipeline.
use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::ImageView;
use crate::detector::atlas_translator::AtlasTranslator;
use crate::detector::roi::RoiRect;
use overmax_core::SceneType;
mod reader;

const ATLAS_ROI_NAMES: [&str; 7] = [
    "gp_center_left",
    "gp_center_right",
    "gp_left_left",
    "gp_left_right",
    "gp_right_left",
    "gp_right_right",
    "pause_title",
];

pub struct GameplaySceneReader {
    pixels: reader::PixelReader,
}

impl Default for GameplaySceneReader {
    fn default() -> Self {
        Self {
            pixels: reader::PixelReader::new(),
        }
    }
}

impl GameplaySceneReader {
    pub(crate) fn supports_frame(frame: &CapturedFrame) -> bool {
        let expected_bytes = usize::try_from(frame.width)
            .ok()
            .zip(usize::try_from(frame.height).ok())
            .and_then(|(width, height)| width.checked_mul(height)?.checked_mul(4));
        expected_bytes == Some(frame.bgra.len())
            && matches!((frame.width, frame.height), (512, 512) | (1920, 1080))
    }

    /// Reads gameplay evidence from the atlas, with a legacy full-frame fallback.
    pub fn read(&mut self, frame: &CapturedFrame) -> SceneType {
        if !Self::supports_frame(frame) {
            return SceneType::Unknown;
        }
        if (frame.width, frame.height) == (512, 512) {
            return self.read_atlas(frame);
        }
        self.read_legacy(frame)
    }

    fn read_atlas(&mut self, frame: &CapturedFrame) -> SceneType {
        let crops = Self::atlas_crops(frame);
        let Some(crops) = crops else {
            return SceneType::Unknown;
        };
        self.pixels.read(&crops)
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

    fn read_legacy(&mut self, frame: &CapturedFrame) -> SceneType {
        let crops = RECTS.map(|(x, y, width, height)| {
            RoiRect {
                x1: x,
                y1: y,
                x2: x + width,
                y2: y + height,
            }
            .crop(frame)
            .expect("fixed ROIs fit a complete 1080p frame")
        });
        self.pixels.read(&crops)
    }
}

// Borrowed bounding views; only nine rows at y80 + 32*i are sampled per side.
// Equal 7px widths exclude measured RESPECT V lane and exterior BGA edges.
// The lower judgment effects and gear-dependent bottom edge are not sampled.
const RECTS: [(i32, i32, i32, i32); 7] = [
    (702, 80, 7, 257),   // CENTER left
    (1211, 80, 7, 257),  // CENTER right
    (102, 80, 7, 257),   // LEFT left
    (611, 80, 7, 257),   // LEFT right
    (1342, 80, 7, 257),  // RIGHT left (+640, not settings preview +480)
    (1851, 80, 7, 257),  // RIGHT right
    (731, 177, 148, 28), // PAUSE title strokes, shared across placements
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::atlas_layout::build_virtual_atlas;
    use crate::detector::templates::gameplay_scene as templates;

    fn frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    fn paint(frame: &mut CapturedFrame, indices: &[usize], pixels: &[u8]) {
        let mut cursor = 0;
        for &i in indices {
            let (x, y, width, height) = RECTS[i];
            for row in y..y + height {
                for col in x..x + width {
                    let start = (row as usize * 1920 + col as usize) * 4;
                    frame.bgra[start..start + 3].fill(pixels[cursor]);
                    cursor += 1;
                }
            }
        }
        assert_eq!(cursor, pixels.len());
    }

    fn stripe(frame: &mut CapturedFrame, index: usize, rows: usize, contrast: u8) {
        let (x, y, width, _) = RECTS[index];
        for row in 0..rows {
            // Use red only: chromatic frame decoration need not be grayscale.
            let pixel = ((y + row as i32 * 32) * 1920 + x + width - 1) as usize * 4;
            frame.bgra[pixel + 2] = contrast;
        }
    }

    #[test]
    fn reads_gameplay_from_a_virtual_atlas_frame() {
        let mut source = frame();
        stripe(&mut source, 0, 9, 30);
        stripe(&mut source, 1, 9, 30);
        let atlas = build_virtual_atlas(&source);

        let mut reader = GameplaySceneReader::default();
        assert_eq!(reader.read(&atlas), SceneType::Gameplay);
    }

    #[test]
    fn gameplay_requires_distributed_support_on_both_sides_at_one_placement() {
        let mut reader = GameplaySceneReader::default();
        for start in [0, 2, 4] {
            let mut frame = frame();
            stripe(&mut frame, start, 9, 255);
            stripe(&mut frame, start + 1, 6, 255);
            assert_eq!(reader.read(&frame), SceneType::Unknown);
            stripe(&mut frame, start + 1, 7, 15);
            assert_eq!(reader.read(&frame), SceneType::Gameplay);
        }
        let mut frame = frame();
        stripe(&mut frame, 0, 9, 255);
        stripe(&mut frame, 5, 9, 255);
        assert_eq!(reader.read(&frame), SceneType::Unknown);
    }

    #[test]
    fn sparse_text_and_exterior_or_judgment_contrast_do_not_establish_gameplay() {
        let mut reader = GameplaySceneReader::default();
        let mut frame = frame();
        stripe(&mut frame, 0, 2, 255);
        stripe(&mut frame, 1, 2, 255);
        // Former RESPECT V outer boundary and lower ROI: neither is evidence now.
        for y in 0..1080 {
            frame.bgra[(y * 1920 + 1219) * 4..(y * 1920 + 1223) * 4].fill(255);
        }
        for x in 710..1210 {
            frame.bgra[(750 * 1920 + x) * 4..(750 * 1920 + x + 1) * 4].fill(255);
        }
        assert_eq!(reader.read(&frame), SceneType::Unknown);
        stripe(&mut frame, 0, 9, 14);
        stripe(&mut frame, 1, 9, 14);
        assert_eq!(reader.read(&frame), SceneType::Unknown);
        stripe(&mut frame, 0, 7, 15);
        stripe(&mut frame, 1, 7, 15);
        assert_eq!(reader.read(&frame), SceneType::Gameplay);
    }

    #[test]
    fn settings_preview_and_lower_structure_are_not_gameplay_evidence() {
        let mut reader = GameplaySceneReader::default();
        let mut frame = frame();
        for x in [1188, 1697] {
            for row in 0..9 {
                frame.bgra[((80 + row * 32) * 1920 + x) * 4 + 2] = 255;
            }
        }
        for x in [708, 1217] {
            for row in 9..17 {
                frame.bgra[((80 + row * 32) * 1920 + x) * 4 + 2] = 255;
            }
        }
        assert_eq!(reader.read(&frame), SceneType::Unknown);
    }

    #[test]
    fn gear_visible_loading_is_gameplay_and_title_alone_takes_priority() {
        let mut reader = GameplaySceneReader::default();
        for start in [0, 2, 4] {
            let mut frame = frame();
            // The former loading stripe no longer vetoes visible gear.
            for y in 573..577 {
                frame.bgra[y * 1920 * 4..(y + 1) * 1920 * 4].fill(255);
            }
            assert_eq!(reader.read(&frame), SceneType::Unknown);
            stripe(&mut frame, start, 9, 30);
            stripe(&mut frame, start + 1, 9, 30);
            assert_eq!(reader.read(&frame), SceneType::Gameplay);
            paint(&mut frame, &[6], templates::PAUSE_TITLE[0]);
            assert_eq!(reader.read(&frame), SceneType::Paused);
        }
        for title in templates::PAUSE_TITLE {
            let mut frame = frame();
            paint(&mut frame, &[6], title);
            assert_eq!(reader.read(&frame), SceneType::Paused);
        }
    }

    #[test]
    fn incomplete_or_unsupported_capture_never_produces_a_scene() {
        let mut reader = GameplaySceneReader::default();
        for (width, height, len) in [(512, 512, 512 * 512 * 4), (1920, 1080, 16), (0, 0, 0)] {
            let frame = CapturedFrame {
                width,
                height,
                bgra: vec![0; len],
            };
            assert_eq!(reader.read(&frame), SceneType::Unknown);
        }
    }
}
