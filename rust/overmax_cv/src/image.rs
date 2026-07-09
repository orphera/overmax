use crate::error::CvError;

pub fn validate_image(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
    name: &str,
) -> Result<(), CvError> {
    if width == 0 || height == 0 || !matches!(channels, 1 | 3 | 4) {
        return Err(CvError::new(format!("{name} received invalid image shape")));
    }
    if data.len() != width * height * channels {
        return Err(CvError::new(format!("{name} received unexpected byte length")));
    }
    Ok(())
}

pub fn to_gray(data: &[u8], channels: usize) -> Vec<u8> {
    if channels == 1 {
        return data.to_vec();
    }

    data.chunks_exact(channels)
        .map(|pixel| bgr_to_gray(pixel[0], pixel[1], pixel[2]))
        .collect()
}

pub fn compute_hashes(gray: &[u8], width: usize, height: usize) -> (u64, u64, u64) {
    (
        phash(gray, width, height),
        dhash(gray, width, height),
        ahash(gray, width, height),
    )
}

pub fn resize_area_u8(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    resize_area_f32(src, sw, sh, dw, dh)
        .iter()
        .map(|value| value.round().clamp(0.0, 255.0) as u8)
        .collect()
}

pub fn resize_area_f32(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut dst = vec![0.0; dw * dh];
    let scale_x = sw as f32 / dw as f32;
    let scale_y = sh as f32 / dh as f32;

    for y in 0..dh {
        for x in 0..dw {
            dst[y * dw + x] = area_pixel(src, sw, x, y, scale_x, scale_y);
        }
    }
    dst
}

pub fn resize_bilinear_u8(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut dst = vec![0; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            dst[y * dw + x] = bilinear_pixel(src, sw, sh, dw, dh, x, y);
        }
    }
    dst
}

fn bgr_to_gray(b: u8, g: u8, r: u8) -> u8 {
    ((29 * u16::from(b) + 150 * u16::from(g) + 77 * u16::from(r) + 128) >> 8) as u8
}

fn ahash(gray: &[u8], width: usize, height: usize) -> u64 {
    let resized = resize_area_f32(gray, width, height, 8, 8);
    let mean = resized.iter().sum::<f32>() / resized.len() as f32;
    bits_to_u64(resized.iter().map(|value| *value > mean))
}

fn dhash(gray: &[u8], width: usize, height: usize) -> u64 {
    let resized = resize_area_f32(gray, width, height, 9, 8);
    let mut bits = Vec::with_capacity(64);
    for y in 0..8 {
        let row = y * 9;
        for x in 0..8 {
            bits.push(resized[row + x + 1] > resized[row + x]);
        }
    }
    bits_to_u64(bits.into_iter())
}

fn phash(gray: &[u8], width: usize, height: usize) -> u64 {
    let resized = resize_area_f32(gray, width, height, 32, 32);
    let coeffs = dct_2d_32(&resized);
    let low = low_dct_values(&coeffs);
    let median = median_without_dc(&low);
    bits_to_u64(low.iter().map(|value| *value > median))
}

fn bits_to_u64(bits: impl Iterator<Item = bool>) -> u64 {
    let mut val = 0u64;
    for bit in bits {
        val = (val << 1) | u64::from(bit);
    }
    val
}

fn area_pixel(src: &[u8], sw: usize, dx: usize, dy: usize, sx: f32, sy: f32) -> f32 {
    let (x0, x1) = (dx as f32 * sx, (dx + 1) as f32 * sx);
    let (y0, y1) = (dy as f32 * sy, (dy + 1) as f32 * sy);
    let mut sum = 0.0;
    let mut area = 0.0;

    for y in y0.floor() as usize..y1.ceil() as usize {
        for x in x0.floor() as usize..x1.ceil() as usize {
            let weight = overlap(x0, x1, x as f32) * overlap(y0, y1, y as f32);
            sum += f32::from(src[y * sw + x]) * weight;
            area += weight;
        }
    }
    sum / area.max(f32::EPSILON)
}

fn overlap(start: f32, end: f32, idx: f32) -> f32 {
    end.min(idx + 1.0) - start.max(idx)
}

struct BilinearCoords {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    fx: f32,
    fy: f32,
}

fn bilinear_pixel(
    src: &[u8],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    dx: usize,
    dy: usize,
) -> u8 {
    let sx = (dx as f32 + 0.5) * sw as f32 / dw as f32 - 0.5;
    let sy = (dy as f32 + 0.5) * sh as f32 / dh as f32 - 0.5;
    let x0 = sx.floor().max(0.0) as usize;
    let y0 = sy.floor().max(0.0) as usize;
    let x1 = (x0 + 1).min(sw - 1);
    let y1 = (y0 + 1).min(sh - 1);
    let fx = sx - sx.floor();
    let fy = sy - sy.floor();
    interpolate_2d(
        src,
        sw,
        BilinearCoords {
            x0,
            y0,
            x1,
            y1,
            fx,
            fy,
        },
    )
}

fn interpolate_2d(src: &[u8], sw: usize, c: BilinearCoords) -> u8 {
    let top = lerp(
        f32::from(src[c.y0 * sw + c.x0]),
        f32::from(src[c.y0 * sw + c.x1]),
        c.fx,
    );
    let bottom = lerp(
        f32::from(src[c.y1 * sw + c.x0]),
        f32::from(src[c.y1 * sw + c.x1]),
        c.fx,
    );
    lerp(top, bottom, c.fy).round().clamp(0.0, 255.0) as u8
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn dct_2d_32(src: &[f32]) -> Vec<f32> {
    let mut tmp = vec![0.0; 32 * 32];
    let mut out = vec![0.0; 32 * 32];
    for y in 0..32 {
        dct_1d_32(&src[y * 32..y * 32 + 32], &mut tmp[y * 32..y * 32 + 32]);
    }
    for x in 0..32 {
        copy_dct_column(&tmp, &mut out, x);
    }
    out
}

fn copy_dct_column(tmp: &[f32], out: &mut [f32], x: usize) {
    let col = column_32(tmp, x);
    let mut coeffs = [0.0; 32];
    dct_1d_32(&col, &mut coeffs);
    for y in 0..32 {
        out[y * 32 + x] = coeffs[y];
    }
}

fn dct_1d_32(src: &[f32], out: &mut [f32]) {
    for (k, value) in out.iter_mut().enumerate().take(32) {
        let alpha = if k == 0 { 0.176_776_69 } else { 0.25 };
        *value = alpha * dct_sum(src, k);
    }
}

fn dct_sum(src: &[f32], k: usize) -> f32 {
    (0..32)
        .map(|n| {
            let angle = std::f32::consts::PI * (2 * n + 1) as f32 * k as f32 / 64.0;
            src[n] * angle.cos()
        })
        .sum()
}

fn column_32(src: &[f32], x: usize) -> [f32; 32] {
    let mut col = [0.0; 32];
    for y in 0..32 {
        col[y] = src[y * 32 + x];
    }
    col
}

fn low_dct_values(coeffs: &[f32]) -> Vec<f32> {
    (0..8)
        .flat_map(|y| (0..8).map(move |x| coeffs[y * 32 + x]))
        .collect()
}

fn median_without_dc(values: &[f32]) -> f32 {
    let mut sorted = values[1..].to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted[sorted.len() / 2]
}

pub fn detect_rect_edges(
    data: &[u8],
    width: usize,
    height: usize,
    margin: usize,
) -> f32 {
    if width <= margin * 2 + 4 || height <= margin * 2 + 4 {
        return 0.0;
    }
    let gray = to_gray(data, 4);

    let mut sum_diff = 0.0;
    let mut count = 0;

    let x_left = margin;
    let x_right = width - margin;
    let y_top = margin;
    let y_bottom = height - margin;

    // 1. 좌측 및 우측 수직 경계선 엣지 감지
    for y in y_top..y_bottom {
        // 좌측 경계선
        let idx_left = y * width + x_left;
        let diff_left = (f32::from(gray[idx_left + 1]) - f32::from(gray[idx_left - 1])).abs();
        sum_diff += diff_left;
        count += 1;

        // 우측 경계선
        let idx_right = y * width + x_right;
        let diff_right = (f32::from(gray[idx_right + 1]) - f32::from(gray[idx_right - 1])).abs();
        sum_diff += diff_right;
        count += 1;
    }

    // 2. 상단 및 하단 수평 경계선 엣지 감지
    for x in x_left..x_right {
        // 상단 경계선
        let idx_top = y_top * width + x;
        let diff_top = (f32::from(gray[idx_top + width]) - f32::from(gray[idx_top - width])).abs();
        sum_diff += diff_top;
        count += 1;

        // 하단 경계선
        let idx_bottom = y_bottom * width + x;
        let diff_bottom = (f32::from(gray[idx_bottom + width]) - f32::from(gray[idx_bottom - width])).abs();
        sum_diff += diff_bottom;
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        sum_diff / count as f32
    }
}

pub struct CvTemplate<'a> {
    pub char_val: char,
    pub width: usize,
    pub height: usize,
    pub mask: &'a [u8],
}

pub fn resize_binary_nearest(
    src: &[u8],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; dw * dh];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return dst;
    }
    for dy in 0..dh {
        let sy = (dy * sh) / dh;
        let sy_clamped = sy.min(sh - 1);
        for dx in 0..dw {
            let sx = (dx * sw) / dw;
            let sx_clamped = sx.min(sw - 1);
            dst[dy * dw + dx] = src[sy_clamped * sw + sx_clamped];
        }
    }
    dst
}

pub fn segment_characters(binary: &[u8], width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut col_proj = vec![0u32; width];
    for x in 0..width {
        let mut sum = 0u32;
        for y in 0..height {
            if binary[y * width + x] == 255 {
                sum += 1;
            }
        }
        col_proj[x] = sum;
    }

    let mut segments = Vec::new();
    let mut in_char = false;
    let mut start_x = 0;
    
    // 켜진 픽셀 임계값 (노이즈 방지를 위해 1열당 높이에 비례한 최소 픽셀 활성화하여 배경 잔여 노이즈 컷)
    let col_threshold = ((height / 10).max(1)) as u32;

    for x in 0..width {
        let active = col_proj[x] >= col_threshold;
        if active && !in_char {
            start_x = x;
            in_char = true;
        } else if !active && in_char {
            let end_x = x;
            if end_x - start_x >= 2 {
                segments.push((start_x, end_x));
            }
            in_char = false;
        }
    }
    
    if in_char {
        let end_x = width;
        if end_x - start_x >= 2 {
            segments.push((start_x, end_x));
        }
    }
    
    segments
}

pub fn match_character(
    char_bin: &[u8],
    char_w: usize,
    char_h: usize,
    templates: &[CvTemplate],
) -> Option<(char, f32)> {
    if char_w == 0 || char_h == 0 || templates.is_empty() {
        return None;
    }

    let target_h = 32usize;
    let target_w = ((char_w as f32 * target_h as f32 / char_h as f32).round()) as usize;
    if target_w == 0 {
        return None;
    }

    // 입력받은 세그먼트를 템플릿 비교 표준인 32px 높이로 리사이징
    let resized_bin = resize_binary_nearest(char_bin, char_w, char_h, target_w, target_h);

    let mut best_char = None;
    let mut best_score = 0.0f32;

    for t in templates {
        // 폭이 너무 크게 차이나는 템플릿 배제 (오인식 억제 필터 - 자간 뭉개짐 편차를 고려하여 6px로 완화)
        let diff_w = (t.width as isize - target_w as isize).abs();
        if diff_w > 6 {
            continue;
        }

        // 템플릿의 가로 폭을 세그먼트 가로 폭(target_w)으로 1대1 일치화
        let scaled_template = resize_binary_nearest(t.mask, t.width, t.height, target_w, target_h);

        // 해밍 거리 (XOR 차이 픽셀 카운트 - 255와 1의 채널 스케일 불일치 규격화 해결)
        let mut diff_pixels = 0usize;
        let total_pixels = target_w * target_h;
        for i in 0..total_pixels {
            let a = if resized_bin[i] > 0 { 1u8 } else { 0u8 };
            let b = if scaled_template[i] > 0 { 1u8 } else { 0u8 };
            if a != b {
                diff_pixels += 1;
            }
        }

        let match_rate = (total_pixels - diff_pixels) as f32 / total_pixels as f32;
        
        let score = match_rate;
        
        if score > best_score {
            best_score = score;
            best_char = Some(t.char_val);
        }
    }

    // 최소 매칭 한계선인 65% 이상일 때만 정상 분류 값으로 통과
    if best_score >= 0.65 {
        best_char.map(|c| (c, best_score))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LumaMethod {
    Weighted,  // BT.601: ((77 * r + 150 * g + 29 * b) >> 8)
    Average,   // (R + G + B) / 3
    MaxRGB,    // max(R, G, B)
}

pub fn binarize_by_luminance(
    bgra: &[u8],
    width: usize,
    height: usize,
    method: LumaMethod,
    threshold_calc: impl FnOnce(u8, u8) -> u8,
    foreground_value: u8,
) -> (Vec<u8>, u8, u8) {
    let total = width * height;
    let mut max_y = 0u8;
    let mut min_y = 255u8;
    let mut luma_vals = vec![0u8; total];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let b = bgra[idx];
            let g = bgra[idx + 1];
            let r = bgra[idx + 2];
            
            let luma = match method {
                LumaMethod::Weighted => ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8,
                LumaMethod::Average => ((r as u32 + g as u32 + b as u32) / 3) as u8,
                LumaMethod::MaxRGB => r.max(g).max(b),
            };
            
            luma_vals[y * width + x] = luma;
            if luma > max_y { max_y = luma; }
            if luma < min_y { min_y = luma; }
        }
    }

    let threshold = threshold_calc(max_y, min_y);
    let mut binary = vec![0u8; total];
    for i in 0..total {
        binary[i] = if luma_vals[i] >= threshold { foreground_value } else { 0 };
    }
    (binary, threshold, max_y)
}

pub fn diff_panel_threshold(max: u8, min: u8) -> u8 {
    if max - min > 30 {
        (min as f32 + (max - min) as f32 * 0.55) as u8
    } else {
        120
    }
}

pub fn adaptive_threshold_bradley_roth(
    bgra: &[u8],
    width: usize,
    height: usize,
    method: LumaMethod,
    block_size: usize,
    t: f32,
    foreground_value: u8,
) -> Vec<u8> {
    let total = width * height;
    let mut luma_vals = vec![0u8; total];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let b = bgra[idx];
            let g = bgra[idx + 1];
            let r = bgra[idx + 2];
            
            let luma = match method {
                LumaMethod::Weighted => ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8,
                LumaMethod::Average => ((r as u32 + g as u32 + b as u32) / 3) as u8,
                LumaMethod::MaxRGB => r.max(g).max(b),
            };
            luma_vals[y * width + x] = luma;
        }
    }

    // 1. 적분 이미지 계산 (Integral Image)
    let mut integral = vec![0u64; total];
    for y in 0..height {
        let mut sum = 0u64;
        for x in 0..width {
            sum += luma_vals[y * width + x] as u64;
            if y == 0 {
                integral[y * width + x] = sum;
            } else {
                integral[y * width + x] = integral[(y - 1) * width + x] + sum;
            }
        }
    }

    // 2. 임계값 비교 및 이진화
    let mut binary = vec![0u8; total];
    let half_s = (block_size / 2) as isize;
    let factor = 1.0 - t;

    for y in 0..height {
        for x in 0..width {
            let x1 = (x as isize - half_s).max(0) as usize;
            let x2 = (x as isize + half_s).min(width as isize - 1) as usize;
            let y1 = (y as isize - half_s).max(0) as usize;
            let y2 = (y as isize + half_s).min(height as isize - 1) as usize;

            let count = (x2 - x1 + 1) * (y2 - y1 + 1);

            let mut sum = integral[y2 * width + x2] as i64;
            if x1 > 0 {
                sum -= integral[y2 * width + (x1 - 1)] as i64;
            }
            if y1 > 0 {
                sum -= integral[(y1 - 1) * width + x2] as i64;
            }
            if x1 > 0 && y1 > 0 {
                sum += integral[(y1 - 1) * width + (x1 - 1)] as i64;
            }

            let luma = luma_vals[y * width + x] as f32;
            let avg = sum.max(0) as f32 / count as f32;

            binary[y * width + x] = if luma >= avg * factor {
                foreground_value
            } else {
                0
            };
        }
    }

    binary
}
