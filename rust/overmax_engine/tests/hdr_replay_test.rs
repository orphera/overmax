use std::path::Path;

#[cfg(windows)]
#[test]
fn test_hdr_snapshot_replay() {
    let raw_path = Path::new("../../cache/hdr_snapshot.raw");
    let fallback_path = Path::new("cache/hdr_snapshot.raw");

    let target_path = if raw_path.exists() {
        raw_path
    } else if fallback_path.exists() {
        fallback_path
    } else {
        println!(
            "\n[HDR Replay Test] Notice: 'cache/hdr_snapshot.raw' not found.\n\
             This test is ready to run as soon as a 1-frame HDR snapshot is dumped on an HDR PC.\n"
        );
        return;
    };

    println!(
        "\n[HDR Replay Test] Loading snapshot from {:?}",
        target_path
    );
    let bytes = std::fs::read(target_path).expect("Failed to read HDR snapshot raw file");

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;
    const EXPECTED_SIZE: usize = WIDTH * HEIGHT * 8;

    assert!(
        bytes.len() >= EXPECTED_SIZE,
        "Snapshot size {} is smaller than expected {}",
        bytes.len(),
        EXPECTED_SIZE
    );

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        convert_scrgb_fp16_to_bgra8, f16_to_f32,
    };

    // 1. FP16 통계 분석
    let mut min_r = f32::MAX;
    let mut max_r = f32::MIN;
    let mut sum_r = 0.0f64;

    let mut min_g = f32::MAX;
    let mut max_g = f32::MIN;
    let mut sum_g = 0.0f64;

    let mut min_b = f32::MAX;
    let mut max_b = f32::MIN;
    let mut sum_b = 0.0f64;

    let pixel_count = WIDTH * HEIGHT;

    for i in 0..pixel_count {
        let offset = i * 8;
        let r_bits = u16::from_ne_bytes([bytes[offset], bytes[offset + 1]]);
        let g_bits = u16::from_ne_bytes([bytes[offset + 2], bytes[offset + 3]]);
        let b_bits = u16::from_ne_bytes([bytes[offset + 4], bytes[offset + 5]]);

        let r = f16_to_f32(r_bits);
        let g = f16_to_f32(g_bits);
        let b = f16_to_f32(b_bits);

        min_r = min_r.min(r);
        max_r = max_r.max(r);
        sum_r += r as f64;

        min_g = min_g.min(g);
        max_g = max_g.max(g);
        sum_g += g as f64;

        min_b = min_b.min(b);
        max_b = max_b.max(b);
        sum_b += b as f64;
    }

    println!("--------------------------------------------------");
    println!(" [RAW FP16 scRGB Statistics]");
    println!(
        " R: min={:.4}, max={:.4}, mean={:.4}",
        min_r,
        max_r,
        sum_r / pixel_count as f64
    );
    println!(
        " G: min={:.4}, max={:.4}, mean={:.4}",
        min_g,
        max_g,
        sum_g / pixel_count as f64
    );
    println!(
        " B: min={:.4}, max={:.4}, mean={:.4}",
        min_b,
        max_b,
        sum_b / pixel_count as f64
    );
    println!("--------------------------------------------------");

    // 2. BGRA8 변환 수행
    let mut bgra8 = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
        let dst_row = unsafe { bgra8.as_mut_ptr().add(y * WIDTH * 4) };
        unsafe {
            convert_scrgb_fp16_to_bgra8(src_row, dst_row, WIDTH);
        }
    }

    // 3. 변환 결과 PNG 저장 (시각적 검증용)
    let mut rgba8 = vec![0u8; WIDTH * HEIGHT * 4];
    for i in 0..pixel_count {
        rgba8[i * 4] = bgra8[i * 4 + 2]; // R
        rgba8[i * 4 + 1] = bgra8[i * 4 + 1]; // G
        rgba8[i * 4 + 2] = bgra8[i * 4]; // B
        rgba8[i * 4 + 3] = bgra8[i * 4 + 3]; // A
    }

    let out_png_path = target_path.with_file_name("hdr_snapshot_preview.png");
    if let Some(img) = image::RgbaImage::from_raw(WIDTH as u32, HEIGHT as u32, rgba8) {
        if img.save(&out_png_path).is_ok() {
            println!(
                "[HDR Replay Test] Preview image saved successfully: {:?}",
                out_png_path
            );
        }
    }
}
