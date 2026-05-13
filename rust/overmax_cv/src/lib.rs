pub mod hog;
pub mod image;
pub mod ocr;

pub fn compute_hashes_gray(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<(String, String, String), String> {
    image::validate_image(data, width, height, 1, "compute_hashes_gray")?;
    Ok(image::compute_hashes(data, width, height))
}

pub fn compute_image_features(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(String, String, String, Vec<f32>), String> {
    image::validate_image(data, width, height, channels, "compute_image_features")?;
    let gray = image::to_gray(data, channels);
    let (phash, dhash, ahash) = image::compute_hashes(&gray, width, height);
    let hog = hog::hog_gray(&gray, width, height);
    Ok((phash, dhash, ahash, hog))
}

pub fn make_thumbnail_bgra_32(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    image::validate_image(data, width, height, 4, "make_thumbnail_bgra_32")?;
    let gray = image::to_gray(data, 4);
    Ok(image::resize_area_u8(&gray, width, height, 32, 32))
}

pub fn preprocess_ocr_bgra(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
) -> Result<Vec<u8>, String> {
    image::validate_image(data, width, height, 4, "preprocess_ocr_bgra")?;
    Ok(ocr::preprocess_bgra(data, width, height, force_invert))
}

#[cfg(feature = "python")]
use pyo3::exceptions::PyValueError;
#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(feature = "python")]
#[pyfunction]
fn hog_gray_64(data: &[u8]) -> PyResult<Vec<f32>> {
    hog::hog_gray_64(data).map_err(PyValueError::new_err)
}

#[cfg(feature = "python")]
#[pyfunction]
fn hog_gray(data: &[u8], width: usize, height: usize) -> PyResult<Vec<f32>> {
    image::validate_image(data, width, height, 1, "hog_gray").map_err(PyValueError::new_err)?;
    Ok(hog::hog_gray(data, width, height))
}

#[cfg(feature = "python")]
#[pyfunction]
fn hashes_gray(data: &[u8], width: usize, height: usize) -> PyResult<(String, String, String)> {
    compute_hashes_gray(data, width, height).map_err(PyValueError::new_err)
}

#[cfg(feature = "python")]
#[pyfunction]
fn image_features(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> PyResult<(String, String, String, Vec<f32>)> {
    compute_image_features(data, width, height, channels).map_err(PyValueError::new_err)
}

#[cfg(feature = "python")]
#[pyfunction]
fn thumbnail_bgra_32(data: &[u8], width: usize, height: usize) -> PyResult<Vec<u8>> {
    make_thumbnail_bgra_32(data, width, height).map_err(PyValueError::new_err)
}

#[cfg(feature = "python")]
#[pyfunction]
fn ocr_preprocess_bgra(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
) -> PyResult<Vec<u8>> {
    preprocess_ocr_bgra(data, width, height, force_invert).map_err(PyValueError::new_err)
}

#[cfg(feature = "python")]
#[pymodule]
fn _overmax_cv(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(version, module)?)?;
    module.add_function(wrap_pyfunction!(hog_gray_64, module)?)?;
    module.add_function(wrap_pyfunction!(hog_gray, module)?)?;
    module.add_function(wrap_pyfunction!(hashes_gray, module)?)?;
    module.add_function(wrap_pyfunction!(image_features, module)?)?;
    module.add_function(wrap_pyfunction!(thumbnail_bgra_32, module)?)?;
    module.add_function(wrap_pyfunction!(ocr_preprocess_bgra, module)?)?;
    Ok(())
}
