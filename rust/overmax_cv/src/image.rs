pub fn validate_image(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
    name: &str,
) -> Result<(), String> {
    if width == 0 || height == 0 || !matches!(channels, 1 | 3 | 4) {
        return Err(format!("{name} received invalid image shape"));
    }
    if data.len() != width * height * channels {
        return Err(format!("{name} received unexpected byte length"));
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

pub fn compute_hashes(gray: &[u8], width: usize, height: usize) -> (String, String, String) {
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

fn ahash(gray: &[u8], width: usize, height: usize) -> String {
    let resized = resize_area_f32(gray, width, height, 8, 8);
    let mean = resized.iter().sum::<f32>() / resized.len() as f32;
    bits_to_hex(resized.iter().map(|value| *value > mean))
}

fn dhash(gray: &[u8], width: usize, height: usize) -> String {
    let resized = resize_area_f32(gray, width, height, 9, 8);
    let mut bits = Vec::with_capacity(64);
    for y in 0..8 {
        let row = y * 9;
        for x in 0..8 {
            bits.push(resized[row + x + 1] > resized[row + x]);
        }
    }
    bits_to_hex(bits.into_iter())
}

fn phash(gray: &[u8], width: usize, height: usize) -> String {
    let resized = resize_area_f32(gray, width, height, 32, 32);
    let coeffs = dct_2d_32(&resized);
    let low = low_dct_values(&coeffs);
    let median = median_without_dc(&low);
    bits_to_hex(low.iter().map(|value| *value > median))
}

fn bits_to_hex(bits: impl Iterator<Item = bool>) -> String {
    let mut out = String::new();
    let mut byte = 0u8;
    for (idx, bit) in bits.enumerate() {
        byte = (byte << 1) | u8::from(bit);
        if idx % 8 == 7 {
            out.push_str(&format!("{byte:02x}"));
            byte = 0;
        }
    }
    out
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
    interpolate_2d(src, sw, x0, y0, x1, y1, fx, fy)
}

fn interpolate_2d(
    src: &[u8],
    sw: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    fx: f32,
    fy: f32,
) -> u8 {
    let top = lerp(
        f32::from(src[y0 * sw + x0]),
        f32::from(src[y0 * sw + x1]),
        fx,
    );
    let bottom = lerp(
        f32::from(src[y1 * sw + x0]),
        f32::from(src[y1 * sw + x1]),
        fx,
    );
    lerp(top, bottom, fy).round().clamp(0.0, 255.0) as u8
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
