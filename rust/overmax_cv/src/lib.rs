pub mod color;
pub mod error;
pub mod hog;
pub mod image;

pub use color::Bgr;

pub fn compute_image_hashes(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(u64, u64, u64), error::CvError> {
    image::validate_image(data, width, height, channels, "compute_image_hashes")?;
    let mut gray = image::to_gray(data, channels);
    image::stretch_contrast(&mut gray, width, height);
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

pub fn match_character_soft(
    char_luma: &[u8],
    char_w: usize,
    char_h: usize,
    templates: &[CvTemplate],
) -> Result<Option<(char, f32)>, error::CvError> {
    Ok(image::match_character_soft(
        char_luma, char_w, char_h, templates,
    ))
}

pub fn binarize_by_global_contrast(
    data: &[u8],
    width: usize,
    height: usize,
    method: LumaMethod,
    foreground_value: u8,
) -> Result<(Vec<u8>, u8, u8), error::CvError> {
    image::validate_image(data, width, height, 4, "binarize_by_global_contrast")?;
    Ok(image::binarize_by_global_contrast(
        data,
        width,
        height,
        method,
        foreground_value,
    ))
}

pub fn binarize_by_global_contrast_with_luma(
    data: &[u8],
    width: usize,
    height: usize,
    method: LumaMethod,
    foreground_value: u8,
) -> Result<(Vec<u8>, u8, u8, Vec<u8>), error::CvError> {
    image::validate_image(
        data,
        width,
        height,
        4,
        "binarize_by_global_contrast_with_luma",
    )?;
    Ok(image::binarize_by_global_contrast_with_luma(
        data,
        width,
        height,
        method,
        foreground_value,
    ))
}

pub use image::{
    adaptive_threshold_bradley_roth, binarize_by_luminance, resize_binary_nearest_into,
    stretch_contrast, to_gray, LumaMethod,
};

/// 4x4 그리드 × RGB 3채널 × 8-bin 히스토그램 (총 384바이트).
///
/// - 입력: BGRA 4채널 이미지 데이터 (channels=4)
/// - 출력: `[u8; 384]` — 각 셀(16개)에 대해 [B_bins(8), G_bins(8), R_bins(8)] 순서로 배열
/// - 정규화: 각 채널별 8개 bin의 합이 64가 되도록 L1 정규화
/// - L1 매칭 시 최대 이론 거리: 64 × 384 = 24576, 실용 정규화 상수: 3072
pub fn compute_grid_histogram(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
) -> [u8; 384] {
    let mut grid_hist = [0u8; 384];
    if width < 4 || height < 4 || channels < 3 {
        return grid_hist;
    }

    let cell_w = width / 4;
    let cell_h = height / 4;

    for gy in 0..4 {
        for gx in 0..4 {
            let start_x = gx * cell_w;
            let end_x = if gx == 3 { width } else { start_x + cell_w };
            let start_y = gy * cell_h;
            let end_y = if gy == 3 { height } else { start_y + cell_h };

            let mut b_bins = [0u32; 8];
            let mut g_bins = [0u32; 8];
            let mut r_bins = [0u32; 8];
            let mut count = 0u32;

            for y in start_y..end_y {
                let row_offset = y * width;
                for x in start_x..end_x {
                    let px = (row_offset + x) * channels;
                    let color = crate::color::Bgr::from_bgra_slice(&data[px..]);
                    let b = color.b as usize;
                    let g = color.g as usize;
                    let r = color.r as usize;
                    b_bins[(b / 32).min(7)] += 1;
                    g_bins[(g / 32).min(7)] += 1;
                    r_bins[(r / 32).min(7)] += 1;
                    count += 1;
                }
            }

            let cell_idx = (gy * 4 + gx) * 24; // 24 = 3ch × 8bins
            #[allow(clippy::manual_checked_ops)]
            if count > 0 {
                for i in 0..8 {
                    grid_hist[cell_idx + i] = ((b_bins[i] * 64) / count) as u8;
                    grid_hist[cell_idx + 8 + i] = ((g_bins[i] * 64) / count) as u8;
                    grid_hist[cell_idx + 16 + i] = ((r_bins[i] * 64) / count) as u8;
                }
            }
        }
    }
    grid_hist
}
