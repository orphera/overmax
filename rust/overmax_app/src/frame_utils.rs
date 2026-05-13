use crate::roi::RoiRect;
use crate::screen_capture::CapturedFrame;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageRegion {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

pub fn crop_roi(frame: &CapturedFrame, roi: RoiRect) -> Option<ImageRegion> {
    let x1 = roi.x1.clamp(0, frame.width);
    let y1 = roi.y1.clamp(0, frame.height);
    let x2 = roi.x2.clamp(0, frame.width);
    let y2 = roi.y2.clamp(0, frame.height);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }

    let width = x2 - x1;
    let height = y2 - y1;
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for y in y1..y2 {
        let start = ((y * frame.width + x1) * 4) as usize;
        let end = start + (width * 4) as usize;
        bgra.extend_from_slice(&frame.bgra[start..end]);
    }
    Some(ImageRegion { width, height, bgra })
}

pub fn region_mean_bgr(frame: &CapturedFrame, roi: RoiRect) -> (u8, u8, u8) {
    let Some(region) = crop_roi(frame, roi) else {
        return (0, 0, 0);
    };
    mean_bgr(&region.bgra)
}

pub fn make_thumbnail(region: &ImageRegion) -> Option<Vec<u8>> {
    overmax_cv::make_thumbnail_bgra_32(&region.bgra, region.width as usize, region.height as usize)
        .ok()
}

pub fn thumbnail_changed(current: &[u8], previous: Option<&[u8]>, threshold: f32) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    if current.len() != previous.len() || current.is_empty() {
        return true;
    }
    mean_abs_diff(current, previous) >= threshold
}

fn mean_bgr(bgra: &[u8]) -> (u8, u8, u8) {
    if bgra.len() < 4 {
        return (0, 0, 0);
    }
    let mut b = 0u64;
    let mut g = 0u64;
    let mut r = 0u64;
    let mut count = 0u64;
    for pixel in bgra.chunks_exact(4) {
        b += u64::from(pixel[0]);
        g += u64::from(pixel[1]);
        r += u64::from(pixel[2]);
        count += 1;
    }
    ((b / count) as u8, (g / count) as u8, (r / count) as u8)
}

fn mean_abs_diff(current: &[u8], previous: &[u8]) -> f32 {
    let sum = current
        .iter()
        .zip(previous)
        .map(|(a, b)| (*a as f32 - *b as f32).abs())
        .sum::<f32>();
    sum / current.len() as f32
}

#[cfg(test)]
mod tests {
    use super::{crop_roi, region_mean_bgr, thumbnail_changed};
    use crate::roi::RoiRect;
    use crate::screen_capture::CapturedFrame;

    #[test]
    fn crops_bgra_region() {
        let frame = CapturedFrame {
            width: 2,
            height: 2,
            bgra: vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0],
        };
        let crop = crop_roi(&frame, RoiRect { x1: 1, y1: 0, x2: 2, y2: 2 }).unwrap();
        assert_eq!(crop.bgra, vec![4, 5, 6, 0, 10, 11, 12, 0]);
    }

    #[test]
    fn computes_region_mean_bgr() {
        let frame = CapturedFrame {
            width: 1,
            height: 2,
            bgra: vec![10, 20, 30, 0, 30, 40, 50, 0],
        };
        assert_eq!(
            region_mean_bgr(&frame, RoiRect { x1: 0, y1: 0, x2: 1, y2: 2 }),
            (20, 30, 40)
        );
    }

    #[test]
    fn detects_thumbnail_changes_by_mean_difference() {
        assert!(thumbnail_changed(&[10, 20], Some(&[0, 0]), 10.0));
        assert!(!thumbnail_changed(&[10, 20], Some(&[9, 19]), 2.5));
    }
}
