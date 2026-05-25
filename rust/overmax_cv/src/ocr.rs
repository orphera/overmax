use crate::image::resize_bilinear_u8;

pub fn preprocess_logo_bgra(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
    binarize: bool,
) -> Vec<u8> {
    let gray = to_gray_ocr(data, 4);
    let upscaled = resize_bilinear_u8(&gray, width, height, width * 3, height * 3);
    let pixels = if binarize {
        let threshold = otsu_threshold(&upscaled);
        let bg_mean = mean_u8(&upscaled);
        let normal_is_dark = bg_mean < 128.0;
        let use_invert = if force_invert { normal_is_dark } else { !normal_is_dark };
        threshold_image(&upscaled, threshold, use_invert)
    } else {
        upscaled
    };
    let padded = pad_gray(&pixels, width * 3, height * 3, 10);
    encode_bmp_gray(&padded, width * 3 + 20, height * 3 + 20)
}

pub fn preprocess_bgra(data: &[u8], width: usize, height: usize, force_invert: bool) -> Vec<u8> {
    preprocess_bgra_with_telemetry(data, width, height, force_invert).0
}

pub fn preprocess_bgra_with_telemetry(
    data: &[u8],
    width: usize,
    height: usize,
    force_invert: bool,
) -> (Vec<u8>, u8, f32, bool, Vec<u8>, usize, usize) {
    let gray = to_gray_ocr(data, 4);
    let upscaled = resize_bilinear_u8(&gray, width, height, width * 3, height * 3);
    let blurred = box_blur_3x3(&upscaled, width * 3, height * 3);
    let threshold = otsu_threshold(&blurred);
    let bg_mean = calculate_border_mean(&blurred, width * 3, height * 3);
    
    // 배경(테두리) 밝기가 오츠 임계값 이하면 (어두운 배경 + 밝은 글씨),
    // "흰 배경에 검은 글씨"로 만들기 위해 반전(invert)을 수행합니다.
    let normal_is_dark = bg_mean <= threshold as f32;
    let use_invert = if force_invert { !normal_is_dark } else { normal_is_dark };
    
    let binary = threshold_image(&blurred, threshold, use_invert);
    let padded = pad_gray(&binary, width * 3, height * 3, 10);
    let bmp = encode_bmp_gray(&padded, width * 3 + 20, height * 3 + 20);
    let padded_width = width * 3 + 20;
    let padded_height = height * 3 + 20;
    (bmp, threshold, bg_mean, use_invert, padded, padded_width, padded_height)
}

fn to_gray_ocr(data: &[u8], channels: usize) -> Vec<u8> {
    if channels == 1 {
        return data.to_vec();
    }
    // 표준 ITU-R BT.601 가중치를 적용한 휘도(Luminance) 기반 그레이스케일 변환
    data.chunks_exact(channels)
        .map(|pixel| {
            ((77 * u16::from(pixel[2]) + 150 * u16::from(pixel[1]) + 29 * u16::from(pixel[0]) + 128) >> 8) as u8
        })
        .collect()
}

fn box_blur_3x3(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    if width < 3 || height < 3 {
        return data.to_vec();
    }
    let mut out = vec![0u8; data.len()];
    
    for y in 1..(height - 1) {
        let prev_row = (y - 1) * width;
        let curr_row = y * width;
        let next_row = (y + 1) * width;
        
        for x in 1..(width - 1) {
            let sum = data[prev_row + x - 1] as u32
                + data[prev_row + x] as u32
                + data[prev_row + x + 1] as u32
                + data[curr_row + x - 1] as u32
                + data[curr_row + x] as u32
                + data[curr_row + x + 1] as u32
                + data[next_row + x - 1] as u32
                + data[next_row + x] as u32
                + data[next_row + x + 1] as u32;
            out[curr_row + x] = (sum / 9) as u8;
        }
    }
    
    for x in 0..width {
        out[x] = data[x];
        out[(height - 1) * width + x] = data[(height - 1) * width + x];
    }
    for y in 0..height {
        out[y * width] = data[y * width];
        out[y * width + (width - 1)] = data[y * width + (width - 1)];
    }
    
    out
}

fn calculate_border_mean(data: &[u8], width: usize, height: usize) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let mut sum = 0u64;
    let mut count = 0u64;
    
    for x in 0..width {
        sum += data[x] as u64;
        sum += data[(height - 1) * width + x] as u64;
        count += 2;
    }
    for y in 1..(height - 1) {
        sum += data[y * width] as u64;
        sum += data[y * width + (width - 1)] as u64;
        count += 2;
    }
    
    if count > 0 {
        sum as f32 / count as f32
    } else {
        0.0
    }
}

fn mean_u8(data: &[u8]) -> f32 {
    data.iter().map(|value| f32::from(*value)).sum::<f32>() / data.len() as f32
}

fn otsu_threshold(data: &[u8]) -> u8 {
    let hist = histogram(data);
    let total = data.len() as f32;
    let sum = hist.iter().enumerate().map(|(i, c)| i as f32 * *c as f32).sum::<f32>();
    best_otsu_threshold(&hist, total, sum)
}

fn histogram(data: &[u8]) -> [u32; 256] {
    let mut hist = [0; 256];
    for value in data {
        hist[*value as usize] += 1;
    }
    hist
}

fn best_otsu_threshold(hist: &[u32; 256], total: f32, sum: f32) -> u8 {
    let mut sum_b = 0.0;
    let mut weight_b = 0.0;
    let mut best = (0u8, -1.0f32);
    for (idx, count) in hist.iter().enumerate() {
        weight_b += *count as f32;
        if weight_b == 0.0 || weight_b == total {
            continue;
        }
        sum_b += idx as f32 * *count as f32;
        let score = otsu_score(sum_b, weight_b, sum, total);
        if score > best.1 {
            best = (idx as u8, score);
        }
    }
    best.0
}

fn otsu_score(sum_b: f32, weight_b: f32, sum: f32, total: f32) -> f32 {
    let weight_f = total - weight_b;
    let mean_b = sum_b / weight_b;
    let mean_f = (sum - sum_b) / weight_f;
    weight_b * weight_f * (mean_b - mean_f).powi(2)
}

fn threshold_image(data: &[u8], threshold: u8, invert: bool) -> Vec<u8> {
    data.iter()
        .map(|value| if (*value > threshold) ^ invert { 255 } else { 0 })
        .collect()
}

fn pad_gray(src: &[u8], width: usize, height: usize, pad: usize) -> Vec<u8> {
    let out_w = width + pad * 2;
    let out_h = height + pad * 2;
    let mut out = vec![0; out_w * out_h];
    for y in 0..height {
        let dst = (y + pad) * out_w + pad;
        out[dst..dst + width].copy_from_slice(&src[y * width..y * width + width]);
    }
    out
}

fn encode_bmp_gray(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let row_stride = width.div_ceil(4) * 4;
    let image_size = row_stride * height;
    let pixel_offset = 14 + 40 + 256 * 4;
    let mut out = Vec::with_capacity(pixel_offset + image_size);
    write_bmp_headers(&mut out, width, height, image_size, pixel_offset);
    write_gray_palette(&mut out);
    write_bmp_pixels(&mut out, data, width, height, row_stride);
    out
}

fn write_bmp_headers(out: &mut Vec<u8>, width: usize, height: usize, image_size: usize, offset: usize) {
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((offset + image_size) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(image_size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]);
}

fn write_gray_palette(out: &mut Vec<u8>) {
    for value in 0..=255u8 {
        out.extend_from_slice(&[value, value, value, 0]);
    }
}

fn write_bmp_pixels(out: &mut Vec<u8>, data: &[u8], width: usize, height: usize, stride: usize) {
    for y in (0..height).rev() {
        out.extend_from_slice(&data[y * width..y * width + width]);
        out.extend(std::iter::repeat_n(0, stride - width));
    }
}
