use crate::frame::CapturedFrame;
use crate::frame::RoiRect;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageRegion {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

impl ImageRegion {
    pub fn as_view(&self) -> ImageView<'_> {
        ImageView {
            data: &self.bgra,
            width: self.width as usize,
            height: self.height as usize,
            stride: (self.width * 4) as usize,
            offset: 0,
        }
    }

    pub fn compute_hashes(
        &self,
        channels: usize,
    ) -> Result<(u64, u64, u64), overmax_cv::error::CvError> {
        overmax_cv::compute_image_hashes(
            &self.bgra,
            self.width as usize,
            self.height as usize,
            channels,
        )
    }

    pub fn detect_edges(&self, margin: usize) -> Result<f32, overmax_cv::error::CvError> {
        overmax_cv::detect_rect_edges(
            &self.bgra,
            self.width as usize,
            self.height as usize,
            margin,
        )
    }
}

/// Zero-Copy image view representing a 2D rectangular slice of an image buffer.
#[derive(Clone, Copy, Debug)]
pub struct ImageView<'a> {
    pub data: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub offset: usize,
}

impl<'a> ImageView<'a> {
    pub fn from_frame(frame: &'a CapturedFrame) -> Self {
        Self {
            data: &frame.bgra,
            width: frame.width as usize,
            height: frame.height as usize,
            stride: (frame.width * 4) as usize,
            offset: 0,
        }
    }

    pub fn crop(&self, roi: RoiRect) -> Option<ImageView<'a>> {
        let x1 = roi.x1.clamp(0, self.width as i32) as usize;
        let y1 = roi.y1.clamp(0, self.height as i32) as usize;
        let x2 = roi.x2.clamp(0, self.width as i32) as usize;
        let y2 = roi.y2.clamp(0, self.height as i32) as usize;
        if x2 <= x1 || y2 <= y1 {
            return None;
        }

        let width = x2 - x1;
        let height = y2 - y1;
        let offset = self.offset + (y1 * self.stride) + (x1 * 4);

        Some(ImageView {
            data: self.data,
            width,
            height,
            stride: self.stride,
            offset,
        })
    }

    #[inline]
    pub fn row(&self, y: usize) -> &'a [u8] {
        let start = self.offset + (y * self.stride);
        let end = start + (self.width * 4);
        &self.data[start..end]
    }

    /// Converts the strided view into an owned continuous ImageRegion (heap allocation).
    pub fn to_image_region(&self) -> ImageRegion {
        let mut bgra = Vec::with_capacity(self.width * self.height * 4);
        for y in 0..self.height {
            bgra.extend_from_slice(self.row(y));
        }
        ImageRegion {
            width: self.width as i32,
            height: self.height as i32,
            bgra,
        }
    }

    pub fn region_mean_bgr(&self) -> overmax_cv::Bgr {
        if self.width == 0 || self.height == 0 {
            return overmax_cv::Bgr::new(0, 0, 0);
        }
        let mut acc = overmax_cv::Bgr::<u64>::default();
        let mut count = 0u64;
        for y in 0..self.height {
            let row = self.row(y);
            for pixel in row.chunks_exact(4) {
                acc += overmax_cv::Bgr::from_bgra_slice(pixel).to_u64();
                count += 1;
            }
        }
        if count == 0 {
            return overmax_cv::Bgr::new(0, 0, 0);
        }
        overmax_cv::Bgr::new(
            (acc.b / count) as u8,
            (acc.g / count) as u8,
            (acc.r / count) as u8,
        )
    }

    pub fn compute_hashes(
        &self,
        channels: usize,
    ) -> Result<(u64, u64, u64), overmax_cv::error::CvError> {
        let region = self.to_image_region();
        region.compute_hashes(channels)
    }

    pub fn detect_edges(&self, margin: usize) -> Result<f32, overmax_cv::error::CvError> {
        let region = self.to_image_region();
        region.detect_edges(margin)
    }
}

pub fn crop_roi<'a>(frame: &'a CapturedFrame, roi: RoiRect) -> Option<ImageView<'a>> {
    let view = ImageView::from_frame(frame);
    view.crop(roi)
}

pub fn region_mean_bgr(frame: &CapturedFrame, roi: RoiRect) -> overmax_cv::Bgr {
    let Some(view) = crop_roi(frame, roi) else {
        return overmax_cv::Bgr::new(0, 0, 0);
    };
    view.region_mean_bgr()
}

pub fn make_thumbnail(view: &ImageView) -> Option<Vec<u8>> {
    let region = view.to_image_region();
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

pub fn compute_pixel_checksum(frame: &CapturedFrame, roi: RoiRect) -> Option<u64> {
    let x1 = roi.x1.clamp(0, frame.width);
    let y1 = roi.y1.clamp(0, frame.height);
    let x2 = roi.x2.clamp(0, frame.width);
    let y2 = roi.y2.clamp(0, frame.height);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }

    let mut sum = 0u64;
    let step = 2; // 2픽셀 간격으로 샘플링
    for y in (y1..y2).step_by(step) {
        for x in (x1..x2).step_by(step) {
            let idx = ((y * frame.width + x) * 4) as usize;
            if idx + 2 < frame.bgra.len() {
                sum += overmax_cv::Bgr::from_bgra_slice(&frame.bgra[idx..])
                    .to_u64()
                    .sum_channels();
            }
        }
    }
    Some(sum)
}

pub(crate) fn mean_abs_diff(current: &[u8], previous: &[u8]) -> f32 {
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
    use crate::frame::CapturedFrame;
    use crate::frame::RoiRect;

    #[test]
    fn crops_bgra_region() {
        let frame = CapturedFrame {
            width: 2,
            height: 2,
            bgra: vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0],
        };
        let crop = crop_roi(
            &frame,
            RoiRect {
                x1: 1,
                y1: 0,
                x2: 2,
                y2: 2,
            },
        )
        .unwrap();
        assert_eq!(crop.to_image_region().bgra, vec![4, 5, 6, 0, 10, 11, 12, 0]);
    }

    #[test]
    fn computes_region_mean_bgr() {
        let frame = CapturedFrame {
            width: 1,
            height: 2,
            bgra: vec![10, 20, 30, 0, 30, 40, 50, 0],
        };
        assert_eq!(
            region_mean_bgr(
                &frame,
                RoiRect {
                    x1: 0,
                    y1: 0,
                    x2: 1,
                    y2: 2
                }
            ),
            overmax_cv::Bgr::new(20, 30, 40)
        );
    }

    #[test]
    fn detects_thumbnail_changes_by_mean_difference() {
        assert!(thumbnail_changed(&[10, 20], Some(&[0, 0]), 10.0));
        assert!(!thumbnail_changed(&[10, 20], Some(&[9, 19]), 2.5));
    }
}
