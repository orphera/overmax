pub mod error;
pub mod hog;
pub mod image;
pub mod ocr;

pub use ocr::OcrPreprocessResult;

pub fn compute_hashes_gray(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<(u64, u64, u64), error::CvError> {
    image::validate_image(data, width, height, 1, "compute_hashes_gray")?;
    Ok(image::compute_hashes(data, width, height))
}

pub fn compute_image_features(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(u64, u64, u64, Vec<f32>), error::CvError> {
    image::validate_image(data, width, height, channels, "compute_image_features")?;
    let gray = image::to_gray(data, channels);
    let (phash, dhash, ahash) = image::compute_hashes(&gray, width, height);
    let hog = hog::hog_gray(&gray, width, height);
    Ok((phash, dhash, ahash, hog))
}

pub fn compute_image_hashes(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(u64, u64, u64), error::CvError> {
    image::validate_image(data, width, height, channels, "compute_image_hashes")?;
    let gray = image::to_gray(data, channels);
    let (phash, dhash, ahash) = image::compute_hashes(&gray, width, height);
    Ok((phash, dhash, ahash))
}

pub fn compute_image_hog(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<Vec<f32>, error::CvError> {
    image::validate_image(data, width, height, channels, "compute_image_hog")?;
    let gray = image::to_gray(data, channels);
    let hog = hog::hog_gray(&gray, width, height);
    Ok(hog)
}

pub fn make_thumbnail_bgra_32(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, error::CvError> {
    image::validate_image(data, width, height, 4, "make_thumbnail_bgra_32")?;
    let gray = image::to_gray(data, 4);
    Ok(image::resize_area_u8(&gray, width, height, 32, 32))
}

pub fn preprocess_ocr_bgra(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
    binarize: bool,
) -> Result<Vec<u8>, error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_bgra")?;
    Ok(ocr::preprocess_logo_bgra(data, width, height, force_invert, binarize))
}

pub fn preprocess_ocr_bgra_with_telemetry(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
    binarize: bool,
) -> Result<OcrPreprocessResult, error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_bgra_with_telemetry")?;
    Ok(ocr::preprocess_bgra_with_telemetry(data, width, height, force_invert, binarize))
}

pub fn preprocess_ocr_color_bgra(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_color_bgra")?;
    Ok(ocr::preprocess_color_bgra(data, width, height))
}

pub fn preprocess_ocr_color_bgra_with_telemetry(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<OcrPreprocessResult, error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_color_bgra_with_telemetry")?;
    Ok(ocr::preprocess_color_bgra_with_telemetry(data, width, height))
}

pub fn detect_rect_edges(
    data: &[u8],
    width: usize,
    height: usize,
    margin: usize,
) -> Result<f32, error::CvError> {
    image::validate_image(data, width, height, 4, "detect_rect_edges")?;
    Ok(image::detect_rect_edges(data, width, height, margin))
}

pub use image::CvTemplate;

pub fn segment_characters(
    binary: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<(usize, usize)>, error::CvError> {
    Ok(image::segment_characters(binary, width, height))
}

pub fn match_character(
    char_bin: &[u8],
    char_w: usize,
    char_h: usize,
    templates: &[CvTemplate],
) -> Result<Option<(char, f32)>, error::CvError> {
    Ok(image::match_character(char_bin, char_w, char_h, templates))
}

pub use image::{LumaMethod, binarize_by_luminance, diff_panel_threshold, adaptive_threshold_bradley_roth};
