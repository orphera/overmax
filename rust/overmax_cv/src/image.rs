use crate::color::Bgr;
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
        return Err(CvError::new(format!(
            "{name} received unexpected byte length"
        )));
    }
    Ok(())
}

pub fn to_gray(data: &[u8], channels: usize) -> Vec<u8> {
    if channels == 1 {
        return data.to_vec();
    }

    data.chunks_exact(channels)
        .map(Bgr::from_bgra_slice)
        .map(|bgr| bgr.luma(LumaMethod::Weighted))
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

pub fn detect_rect_edges(data: &[u8], width: usize, height: usize, margin: usize) -> f32 {
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
        let diff_bottom =
            (f32::from(gray[idx_bottom + width]) - f32::from(gray[idx_bottom - width])).abs();
        sum_diff += diff_bottom;
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        sum_diff / count as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CvTemplate<'a> {
    pub char_val: char,
    pub width: usize,
    pub height: usize,
    pub mask: &'a [u8],
}

pub fn resize_binary_nearest_into(
    src: &[u8],
    sw: usize,
    sh: usize,
    dst: &mut [u8],
    dw: usize,
    dh: usize,
) {
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || dst.len() < dw * dh || src.len() < sw * sh {
        return;
    }
    for dy in 0..dh {
        let sy = (dy * sh) / dh;
        let sy_clamped = sy.min(sh - 1);
        let src_row = sy_clamped * sw;
        let dst_row = dy * dw;
        for dx in 0..dw {
            let sx = (dx * sw) / dw;
            let sx_clamped = sx.min(sw - 1);
            dst[dst_row + dx] = src[src_row + sx_clamped];
        }
    }
}

pub fn segment_characters(binary: &[u8], width: usize, height: usize) -> Vec<(usize, usize)> {
    if width == 0 || height == 0 || binary.len() < width * height {
        return Vec::new();
    }

    const MAX_STACK_WIDTH: usize = 512;
    let mut stack_proj = [0u32; MAX_STACK_WIDTH];
    let mut heap_proj;

    let col_proj: &mut [u32] = if width <= MAX_STACK_WIDTH {
        &mut stack_proj[..width]
    } else {
        heap_proj = vec![0u32; width];
        &mut heap_proj[..]
    };

    for x in 0..width {
        let mut sum = 0u32;
        for y in 0..height {
            if binary[y * width + x] == 255 {
                sum += 1;
            }
        }
        col_proj[x] = sum;
    }

    let mut segments = Vec::with_capacity(8);
    let mut in_char = false;
    let mut start_x = 0;

    // 켜진 픽셀 임계값 (노이즈 방지를 위해 1열당 높이에 비례한 최소 픽셀 활성화하여 배경 잔여 노이즈 컷)
    // 숫자 '1'의 하단 가로 받침대(Base Serif, 높이 ~6px)가 잘려나가지 않도록 height / 25 (최소 1px) 적용
    let col_threshold = ((height / 25).max(1)) as u32;

    for (x, &col_sum) in col_proj.iter().enumerate().take(width) {
        let active = col_sum >= col_threshold;
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
    if char_w == 0 || char_h == 0 || templates.is_empty() || char_bin.len() < char_w * char_h {
        return None;
    }

    let target_h = 32usize;
    let target_w = ((char_w as f32 * target_h as f32 / char_h as f32).round()) as usize;
    if target_w == 0 || target_w > 32 {
        return None;
    }

    // 세그먼트를 32px 높이 표준 크기로 리사이즈하여 행 단위 u32 비트마스크에 패킹 (Zero-allocation)
    let mut resized_bin_bits = [0u32; 32];
    for (dy, row_bits) in resized_bin_bits.iter_mut().enumerate() {
        let sy = (dy * char_h) / target_h;
        let sy_clamped = sy.min(char_h - 1);
        let mut bits = 0u32;
        for dx in 0..target_w {
            let sx = (dx * char_w) / target_w;
            let sx_clamped = sx.min(char_w - 1);
            if char_bin[sy_clamped * char_w + sx_clamped] > 0 {
                bits |= 1 << dx;
            }
        }
        *row_bits = bits;
    }

    let mut best_char = None;
    let mut best_score = 0.0f32;

    for t in templates {
        // 폭이 너무 크게 차이나는 템플릿 배제 (오인식 억제 필터)
        let diff_w = (t.width as isize - target_w as isize).abs();
        if diff_w > 6 || t.width == 0 || t.height == 0 || t.mask.len() < t.width * t.height {
            continue;
        }

        // 템플릿의 가로 폭을 세그먼트 가로 폭(target_w)으로 실시간 샘플링하며 비트 XOR 해밍 거리 누적
        let mut diff_pixels = 0usize;
        for (dy, &bin_row_bits) in resized_bin_bits.iter().enumerate() {
            let sy = (dy * t.height) / target_h;
            let sy_clamped = sy.min(t.height - 1);
            let mut t_row_bits = 0u32;
            for dx in 0..target_w {
                let sx = (dx * t.width) / target_w;
                let sx_clamped = sx.min(t.width - 1);
                if t.mask[sy_clamped * t.width + sx_clamped] > 0 {
                    t_row_bits |= 1 << dx;
                }
            }
            diff_pixels += (bin_row_bits ^ t_row_bits).count_ones() as usize;
        }

        let total_pixels = target_w * target_h;
        let match_rate = (total_pixels - diff_pixels) as f32 / total_pixels as f32;

        if match_rate > best_score {
            best_score = match_rate;
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
    Weighted, // BT.601: ((77 * r + 150 * g + 29 * b) >> 8)
    Average,  // (R + G + B) / 3
    MaxRGB,   // max(R, G, B)
}

impl LumaMethod {
    #[inline]
    pub fn calculate_luma(self, bgr: Bgr) -> u8 {
        match self {
            LumaMethod::Weighted => {
                ((29 * u16::from(bgr.b) + 150 * u16::from(bgr.g) + 77 * u16::from(bgr.r) + 128)
                    >> 8) as u8
            }
            LumaMethod::Average => bgr.average(),
            LumaMethod::MaxRGB => bgr.r.max(bgr.g).max(bgr.b),
        }
    }

    #[inline]
    pub fn calculate_luma_f64(self, bgr: Bgr<f64>) -> f64 {
        match self {
            LumaMethod::Weighted => 0.114 * bgr.b + 0.587 * bgr.g + 0.299 * bgr.r,
            LumaMethod::Average => bgr.average(),
            LumaMethod::MaxRGB => bgr.max_channel(),
        }
    }
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
            let luma = Bgr::from_bgra_slice(&bgra[idx..]).luma(method);

            luma_vals[y * width + x] = luma;
            if luma > max_y {
                max_y = luma;
            }
            if luma < min_y {
                min_y = luma;
            }
        }
    }

    let threshold = threshold_calc(max_y, min_y);
    let mut binary = vec![0u8; total];
    for i in 0..total {
        binary[i] = if luma_vals[i] >= threshold {
            foreground_value
        } else {
            0
        };
    }
    (binary, threshold, max_y)
}

/// 전역 대비(Global Contrast) 기반 유동 임계치를 사용하여 이미지의 휘도를 이진화합니다.
pub fn binarize_by_global_contrast(
    bgra: &[u8],
    width: usize,
    height: usize,
    method: LumaMethod,
    foreground_value: u8,
) -> (Vec<u8>, u8, u8) {
    binarize_by_luminance(
        bgra,
        width,
        height,
        method,
        |max, min| {
            if max > 40 && max.saturating_sub(min) > 15 {
                let contrast = max.saturating_sub(min) as f32;
                let calculated = (min as f32 + contrast * 0.65) as u8;
                calculated.max(min + 5)
            } else {
                180
            }
        },
        foreground_value,
    )
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
            luma_vals[y * width + x] = Bgr::from_bgra_slice(&bgra[idx..]).luma(method);
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

pub fn stretch_contrast(gray: &mut [u8], _width: usize, _height: usize) {
    if gray.is_empty() {
        return;
    }
    let mut min = 255u8;
    let mut max = 0u8;
    for &val in gray.iter() {
        if val < min {
            min = val;
        }
        if val > max {
            max = val;
        }
    }

    let range = max.saturating_sub(min);
    if range > 15 {
        let range_f = range as f32;
        for val in gray.iter_mut() {
            let stretched = ((*val as f32 - min as f32) / range_f * 255.0).round();
            *val = stretched.clamp(0.0, 255.0) as u8;
        }
    }
}

/// 60x60 자켓 영역에 대해 외곽 노이즈를 제어하기 위한 비균일 3구역 마스킹 적용
pub fn apply_non_uniform_mask(gray: &mut [u8], width: usize, height: usize) {
    if width != 60 || height != 60 || gray.len() != 3600 {
        return; // DJMAX 자켓 규격이 아닐 경우 스킵
    }

    let cx = 30isize;
    let cy = 30isize;

    for y in 0..60 {
        for x in 0..60 {
            let idx = y * 60 + x;

            // 중심부(30, 30)로부터의 Chebyshev 거리 계산
            let dist_x = (x as isize - cx).abs();
            let dist_y = (y as isize - cy).abs();
            let dist = dist_x.max(dist_y);

            if dist >= 27 {
                // Zone 3: 데드 존 (외곽 3px 영역) -> 중성 회색(128)으로 덮어 씌움
                gray[idx] = 128;
            } else if dist >= 20 {
                // Zone 2: 버퍼 존 (경계면 완충 지대) -> 대비 완화 및 스무딩 효과
                let original = gray[idx] as f32;
                let weight = (27 - dist) as f32 / 7.0; // 1.0 (dist=20) ~ 0.0 (dist=27)
                gray[idx] = (original * weight + 128.0 * (1.0 - weight)).round() as u8;
            }
            // Zone 1 (dist < 20): 코어 존 -> 원본 픽셀 100% 보존
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_character_perfect_match() {
        // 16x32 pattern: vertical bar
        let mut mask = [0u8; 16 * 32];
        for y in 0..32 {
            for x in 6..10 {
                mask[y * 16 + x] = 1;
            }
        }
        let template = CvTemplate {
            char_val: '1',
            width: 16,
            height: 32,
            mask: &mask,
        };

        let result = match_character(&mask, 16, 32, &[template]);
        assert!(result.is_some());
        let (ch, score) = result.unwrap();
        assert_eq!(ch, '1');
        assert!((score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_match_character_rescaled() {
        // 16x32 pattern scaled down to 8x16 input
        let mut mask = [0u8; 16 * 32];
        for y in 0..32 {
            for x in 6..10 {
                mask[y * 16 + x] = 1;
            }
        }
        let template = CvTemplate {
            char_val: '1',
            width: 16,
            height: 32,
            mask: &mask,
        };

        let mut input_8x16 = [0u8; 8 * 16];
        resize_binary_nearest_into(&mask, 16, 32, &mut input_8x16, 8, 16);
        let result = match_character(&input_8x16, 8, 16, &[template]);
        assert!(result.is_some());
        let (ch, score) = result.unwrap();
        assert_eq!(ch, '1');
        assert!(score >= 0.95);
    }

    #[test]
    fn test_match_character_edge_cases() {
        let template = CvTemplate {
            char_val: 'A',
            width: 8,
            height: 8,
            mask: &[1u8; 64],
        };

        // Empty / zero dimension cases
        assert_eq!(match_character(&[], 0, 0, &[template]), None);
        assert_eq!(match_character(&[1], 1, 0, &[template]), None);
        assert_eq!(match_character(&[1], 0, 1, &[template]), None);
        assert_eq!(match_character(&[1; 64], 8, 8, &[]), None);

        // Insufficient buffer length
        assert_eq!(match_character(&[1; 10], 8, 8, &[template]), None);

        // Unmatched character returns None if score < 0.65
        let zero_input = [0u8; 64];
        assert_eq!(match_character(&zero_input, 8, 8, &[template]), None);
    }

    #[test]
    fn test_resize_binary_nearest_into() {
        let src = [1, 0, 1, 0, 0, 1, 0, 1]; // 4x2
        let mut dst = [0u8; 8 * 4];
        resize_binary_nearest_into(&src, 4, 2, &mut dst, 8, 4);
        assert_eq!(&dst[0..8], &[1, 1, 0, 0, 1, 1, 0, 0]);
        assert_eq!(&dst[8..16], &[1, 1, 0, 0, 1, 1, 0, 0]);
        assert_eq!(&dst[16..24], &[0, 0, 1, 1, 0, 0, 1, 1]);
        assert_eq!(&dst[24..32], &[0, 0, 1, 1, 0, 0, 1, 1]);
    }

    #[test]
    fn test_segment_characters_stack_and_large_inputs() {
        // Small input (<= 512px stack path)
        let mut binary_small = vec![0u8; 100 * 20];
        // Paint 2 characters of width 10 each
        for y in 0..20 {
            for x in 10..20 {
                binary_small[y * 100 + x] = 255;
            }
            for x in 40..50 {
                binary_small[y * 100 + x] = 255;
            }
        }
        let segs_small = segment_characters(&binary_small, 100, 20);
        assert_eq!(segs_small, vec![(10, 20), (40, 50)]);

        // Large input (> 512px heap fallback path)
        let mut binary_large = vec![0u8; 600 * 10];
        for y in 0..10 {
            for x in 550..570 {
                binary_large[y * 600 + x] = 255;
            }
        }
        let segs_large = segment_characters(&binary_large, 600, 10);
        assert_eq!(segs_large, vec![(550, 570)]);
    }
}
