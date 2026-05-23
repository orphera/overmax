pub mod error;
pub mod hog;
pub mod image;
pub mod ocr;

pub fn compute_hashes_gray(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<(String, String, String), error::CvError> {
    image::validate_image(data, width, height, 1, "compute_hashes_gray")?;
    Ok(image::compute_hashes(data, width, height))
}

pub fn compute_image_features(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(String, String, String, Vec<f32>), error::CvError> {
    image::validate_image(data, width, height, channels, "compute_image_features")?;
    let gray = image::to_gray(data, channels);
    let (phash, dhash, ahash) = image::compute_hashes(&gray, width, height);
    let hog = hog::hog_gray(&gray, width, height);
    Ok((phash, dhash, ahash, hog))
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
) -> Result<Vec<u8>, error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_bgra")?;
    Ok(ocr::preprocess_bgra(data, width, height, force_invert))
}

pub fn preprocess_ocr_bgra_with_telemetry(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
) -> Result<(Vec<u8>, u8, f32, bool), error::CvError> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_bgra_with_telemetry")?;
    Ok(ocr::preprocess_bgra_with_telemetry(data, width, height, force_invert))
}
