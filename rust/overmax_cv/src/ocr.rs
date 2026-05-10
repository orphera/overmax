use crate::image::{resize_bilinear_u8, to_gray};

pub fn preprocess_bgra(data: &[u8], width: usize, height: usize, force_invert: bool) -> Vec<u8> {
    let gray = to_gray(data, 4);
    let upscaled = resize_bilinear_u8(&gray, width, height, width * 3, height * 3);
    let threshold = otsu_threshold(&upscaled);
    let bg_mean = mean_u8(&upscaled);
    let normal_is_dark = bg_mean < 128.0;
    let use_invert = if force_invert { normal_is_dark } else { !normal_is_dark };
    let binary = threshold_image(&upscaled, threshold, use_invert);
    let padded = pad_gray(&binary, width * 3, height * 3, 10);
    encode_bmp_gray(&padded, width * 3 + 20, height * 3 + 20)
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
