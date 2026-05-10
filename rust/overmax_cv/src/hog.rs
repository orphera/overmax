use pyo3::exceptions::PyValueError;
use pyo3::PyResult;

use crate::image::resize_area_u8;

const SIZE: usize = 64;
const CELL: usize = 8;
const BLOCKS: usize = 7;
const BINS: usize = 9;
const HOG_LEN: usize = BLOCKS * BLOCKS * 4 * BINS;
const BLOCK_SIGMA: f32 = 4.0;

pub fn hog_gray_64(data: &[u8]) -> PyResult<Vec<f32>> {
    if data.len() != SIZE * SIZE {
        return Err(PyValueError::new_err("hog_gray_64 expects 4096 grayscale bytes"));
    }
    Ok(hog_from_resized_gray(data))
}

pub fn hog_gray(data: &[u8], width: usize, height: usize) -> Vec<f32> {
    let resized = resize_area_u8(data, width, height, SIZE, SIZE);
    hog_from_resized_gray(&resized)
}

fn hog_from_resized_gray(data: &[u8]) -> Vec<f32> {
    let src = data.iter().map(|value| f32::from(*value)).collect::<Vec<_>>();
    let (gx, gy) = gradients(&src);
    block_features(&gx, &gy)
}

fn gradients(src: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut gx = vec![0.0; SIZE * SIZE];
    let mut gy = vec![0.0; SIZE * SIZE];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = index(x, y);
            gx[idx] = sample_x(src, x, y);
            gy[idx] = sample_y(src, x, y);
        }
    }

    (gx, gy)
}

fn sample_x(src: &[f32], x: usize, y: usize) -> f32 {
    if x == 0 || x == SIZE - 1 {
        return 0.0;
    }
    src[index(x + 1, y)] - src[index(x - 1, y)]
}

fn sample_y(src: &[f32], x: usize, y: usize) -> f32 {
    if y == 0 || y == SIZE - 1 {
        return 0.0;
    }
    src[index(x, y + 1)] - src[index(x, y - 1)]
}

fn block_features(gx: &[f32], gy: &[f32]) -> Vec<f32> {
    let mut features = Vec::with_capacity(HOG_LEN);
    for block_x in 0..BLOCKS {
        for block_y in 0..BLOCKS {
            let mut block = collect_block(gx, gy, block_x, block_y);
            normalize_block(&mut block);
            features.extend(block);
        }
    }
    features
}

fn collect_block(gx: &[f32], gy: &[f32], block_x: usize, block_y: usize) -> Vec<f32> {
    let mut block = vec![0.0; 4 * BINS];
    let start_x = block_x * CELL;
    let start_y = block_y * CELL;
    for y in start_y..start_y + 2 * CELL {
        for x in start_x..start_x + 2 * CELL {
            let idx = index(x, y);
            vote_pixel(&mut block, x - start_x, y - start_y, gx[idx], gy[idx]);
        }
    }
    block
}

fn vote_pixel(block: &mut [f32], x: usize, y: usize, gx: f32, gy: f32) {
    let mag = (gx * gx + gy * gy).sqrt() * gaussian_weight(x, y);
    if mag == 0.0 {
        return;
    }

    let angle = gy.atan2(gx).to_degrees().rem_euclid(180.0);
    let cell_x = (x as f32 + 0.5) / CELL as f32 - 0.5;
    let cell_y = (y as f32 + 0.5) / CELL as f32 - 0.5;
    vote_pixel_cells(block, mag, angle, cell_x, cell_y);
}

fn vote_pixel_cells(block: &mut [f32], mag: f32, angle: f32, cell_x: f32, cell_y: f32) {
    let left = cell_x.floor() as isize;
    let top = cell_y.floor() as isize;
    let frac_x = cell_x - left as f32;
    let frac_y = cell_y - top as f32;
    vote_cell(block, left, top, mag * (1.0 - frac_x) * (1.0 - frac_y), angle);
    vote_cell(block, left + 1, top, mag * frac_x * (1.0 - frac_y), angle);
    vote_cell(block, left, top + 1, mag * (1.0 - frac_x) * frac_y, angle);
    vote_cell(block, left + 1, top + 1, mag * frac_x * frac_y, angle);
}

fn gaussian_weight(x: usize, y: usize) -> f32 {
    let dx = x as f32 + 0.5 - CELL as f32;
    let dy = y as f32 + 0.5 - CELL as f32;
    (-(dx * dx + dy * dy) / (2.0 * BLOCK_SIGMA * BLOCK_SIGMA)).exp()
}

fn vote_cell(block: &mut [f32], cx: isize, cy: isize, mag: f32, angle: f32) {
    if cx < 0 || cy < 0 || cx >= 2 || cy >= 2 {
        return;
    }

    let bin = (angle - 10.0) / 20.0;
    let low_floor = bin.floor();
    let low = low_floor.rem_euclid(BINS as f32) as usize;
    let high = (low + 1) % BINS;
    let frac = bin - low_floor;
    let base = block_cell_index(cx as usize, cy as usize, 0);
    block[base + low] += mag * (1.0 - frac);
    block[base + high] += mag * frac;
}

fn normalize_block(block: &mut [f32]) {
    normalize_l2(block);
    for value in block.iter_mut() {
        *value = value.min(0.2);
    }
    normalize_l2(block);
}

fn normalize_l2(values: &mut [f32]) {
    let sum = values.iter().map(|value| value * value).sum::<f32>();
    let denom = (sum + 1e-6).sqrt();
    for value in values.iter_mut() {
        *value /= denom;
    }
}

fn index(x: usize, y: usize) -> usize {
    y * SIZE + x
}

fn block_cell_index(x: usize, y: usize, bin: usize) -> usize {
    (x * 2 + y) * BINS + bin
}
