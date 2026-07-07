use crate::capture::frame_utils::ImageRegion;

pub(super) struct OcrEngine;

impl OcrEngine {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn is_available(&self) -> bool {
        false
    }

    pub(super) fn recognize_logo(
        &self,
        _image: &ImageRegion,
        _force_invert: bool,
        _binarize: bool,
    ) -> Result<String, String> {
        Ok(String::new())
    }

    pub(super) fn recognize_logo_color(&self, _image: &ImageRegion) -> Result<String, String> {
        Ok(String::new())
    }

    pub(super) fn recognize_with_telemetry(
        &self,
        _image: &ImageRegion,
        _force_invert: bool,
        _binarize: bool,
    ) -> Result<(String, u8, f32, bool, Vec<u8>, usize, usize), String> {
        Ok((String::new(), 0, 0.0, false, Vec::new(), 0, 0))
    }

    pub(super) fn recognize_color_with_telemetry(
        &self,
        _image: &ImageRegion,
    ) -> Result<(String, u8, f32, bool, Vec<u8>, usize, usize), String> {
        Ok((String::new(), 0, 0.0, false, Vec::new(), 0, 0))
    }
}
