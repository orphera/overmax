use std::path::{Path, PathBuf};

#[cfg(windows)]
#[test]
fn test_analyze_all_hdr_snapshots() {
    let snapshot_dir = Path::new("../../scratch/hdr_snapshot");
    let fallback_dir = Path::new("scratch/hdr_snapshot");

    let target_dir = if snapshot_dir.exists() {
        snapshot_dir
    } else if fallback_dir.exists() {
        fallback_dir
    } else {
        println!("[HDR Replay Test] Notice: 'scratch/hdr_snapshot' directory not found.");
        return;
    };

    let entries = std::fs::read_dir(target_dir).expect("Failed to read snapshot dir");
    let mut raw_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("raw"))
        .collect();

    raw_files.sort();

    if raw_files.is_empty() {
        println!("[HDR Replay Test] No .raw files found in {:?}", target_dir);
        return;
    }

    // Load Image DB if exists
    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        None
    };

    let matcher_opt = actual_db.and_then(|p| {
        let mut db = overmax_data::store::image_index::ImageIndexDb::new(p, 0.60);
        db.load().ok().map(|count| {
            println!("Loaded Image DB with {} entries", count);
            db.matcher()
        })
    });

    println!("\n==================================================");
    println!(
        " [HDR SNAPSHOT DEEP ANALYSIS] Found {} dump files",
        raw_files.len()
    );
    println!("==================================================");

    for raw_path in &raw_files {
        analyze_single_snapshot(raw_path, matcher_opt.as_ref());
    }
}

#[cfg(windows)]
fn analyze_single_snapshot(
    raw_path: &Path,
    matcher_opt: Option<&overmax_data::service::jacket_matcher::JacketMatcher>,
) {
    let file_name = raw_path.file_name().unwrap().to_string_lossy();
    println!("\n>>> Analyzing: {} <<<", file_name);

    let bytes = std::fs::read(raw_path).expect("Failed to read raw file");
    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;
    const EXPECTED_SIZE: usize = WIDTH * HEIGHT * 8;

    if bytes.len() < EXPECTED_SIZE {
        println!(
            "  [ERROR] File size {} < expected {}",
            bytes.len(),
            EXPECTED_SIZE
        );
        return;
    }

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        convert_scrgb_fp16_to_bgra8, f16_to_f32, strict_srgb_oetf,
    };

    let pixel_count = WIDTH * HEIGHT;

    // 1. 전수 통계 (Whole Atlas)
    let mut min_r = f32::MAX;
    let mut max_r = f32::MIN;
    let mut min_g = f32::MAX;
    let mut max_g = f32::MIN;
    let mut min_b = f32::MAX;
    let mut max_b = f32::MIN;

    for i in 0..pixel_count {
        let offset = i * 8;
        let r = f16_to_f32(u16::from_ne_bytes([bytes[offset], bytes[offset + 1]]));
        let g = f16_to_f32(u16::from_ne_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let b = f16_to_f32(u16::from_ne_bytes([bytes[offset + 4], bytes[offset + 5]]));

        min_r = min_r.min(r);
        max_r = max_r.max(r);
        min_g = min_g.min(g);
        max_g = max_g.max(g);
        min_b = min_b.min(b);
        max_b = max_b.max(b);
    }

    println!("  [물리 휘도 통계 (scRGB 1.0 = 80 nits)]");
    println!(
        "   R: min={:>8.4}, max={:>8.4} (max nits={:.1})",
        min_r,
        max_r,
        max_r * 80.0
    );
    println!(
        "   G: min={:>8.4}, max={:>8.4} (max nits={:.1})",
        min_g,
        max_g,
        max_g * 80.0
    );
    println!(
        "   B: min={:>8.4}, max={:>8.4} (max nits={:.1})",
        min_b,
        max_b,
        max_b * 80.0
    );

    // 2. 64KB LUT vs 부동소수점 수학 연산 벤치마크 및 오차 검증
    let scale = 5.168f32;
    let mut bgra_lut = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
        let dst_row = unsafe { bgra_lut.as_mut_ptr().add(y * WIDTH * 4) };
        unsafe {
            convert_scrgb_fp16_to_bgra8(src_row, dst_row, WIDTH);
        }
    }

    // 부동소수점 직접 연산 (검증 기준)
    let mut bgra_math = vec![0u8; WIDTH * HEIGHT * 4];
    for i in 0..pixel_count {
        let offset = i * 8;
        let r = f16_to_f32(u16::from_ne_bytes([bytes[offset], bytes[offset + 1]]));
        let g = f16_to_f32(u16::from_ne_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let b = f16_to_f32(u16::from_ne_bytes([bytes[offset + 4], bytes[offset + 5]]));

        let r_lin = (r / scale).clamp(0.0, 1.0);
        let g_lin = (g / scale).clamp(0.0, 1.0);
        let b_lin = (b / scale).clamp(0.0, 1.0);

        let r_srgb = strict_srgb_oetf(r_lin);
        let g_srgb = strict_srgb_oetf(g_lin);
        let b_srgb = strict_srgb_oetf(b_lin);

        let dst_offset = i * 4;
        bgra_math[dst_offset] = (b_srgb * 255.0 + 0.5) as u8;
        bgra_math[dst_offset + 1] = (g_srgb * 255.0 + 0.5) as u8;
        bgra_math[dst_offset + 2] = (r_srgb * 255.0 + 0.5) as u8;
        bgra_math[dst_offset + 3] = 255;
    }

    let mut max_diff = 0i32;
    for i in 0..WIDTH * HEIGHT * 4 {
        let diff = (bgra_lut[i] as i32 - bgra_math[i] as i32).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    println!(
        "  [LUT 정밀도 검증] LUT vs 직접 계산 최대 픽셀 오차: {} (0 = 비트 완벽)",
        max_diff
    );
    assert_eq!(
        max_diff, 0,
        "LUT conversion must match mathematical sRGB formula with 0 error"
    );

    // 3. 자켓 매칭 및 디텍션 파이프라인 검증
    if let Some(matcher) = matcher_opt {
        let extract_rect = |buf: &[u8], x: usize, y: usize, w: usize, h: usize| -> Vec<u8> {
            let mut j = Vec::with_capacity(w * h * 4);
            for row in 0..h {
                let row_start = (y + row) * WIDTH * 4 + x * 4;
                j.extend_from_slice(&buf[row_start..row_start + w * 4]);
            }
            j
        };

        let fs_jacket = extract_rect(&bgra_lut, 340, 94, 60, 60);
        let res_jacket = extract_rect(&bgra_lut, 75, 395, 60, 60);

        let match_fs = matcher.match_jacket(&fs_jacket, 60, 60, 4);
        let match_res = matcher.match_jacket(&res_jacket, 60, 60, 4);

        println!(
            "  [자켓 매칭] Freestyle(340,94)={:?} | Result(75,395)={:?}",
            match_fs
                .as_ref()
                .map(|m| format!("id={}, sim={:.4}", m.image_id, m.similarity)),
            match_res
                .as_ref()
                .map(|m| format!("id={}, sim={:.4}", m.image_id, m.similarity)),
        );

        // 4. Detection Pipeline 전체 씬 감지
        use overmax_core::{Difficulty, Mode, SceneType};
        use overmax_engine::capture::frame::CapturedFrame;
        use overmax_engine::detector::detection_pipeline::detect_static_scene;
        use overmax_engine::detector::roi::RoiManager;
        use overmax_engine::detector::templates;

        let frame = CapturedFrame {
            width: WIDTH as i32,
            height: HEIGHT as i32,
            bgra: bgra_lut,
        };

        let rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
        let detected_scene = detect_static_scene(&frame, &rois, matcher);
        println!("  [씬 판정 (detect_static_scene)] {:?}", detected_scene);

        // ResultFreestyle ROI 추출
        let mut rois_res = rois.clone();
        rois_res.set_scene(SceneType::ResultFreestyle);
        let res_mode =
            rois_res.and_then_roi(&frame, "mode_digit", templates::detect_freestyle_mode);
        let res_diff =
            rois_res.and_then_roi(&frame, "diff_panel", templates::detect_result_difficulty);
        let res_score = rois_res.and_then_roi(&frame, "score", templates::detect_score);
        let res_rate = rois_res.and_then_roi(&frame, "rate", |img| templates::detect_rate(img));

        // Freestyle ROI 추출
        let mut rois_fs = rois.clone();
        rois_fs.set_scene(SceneType::Freestyle);
        let fs_mode = overmax_engine::detector::play_state::detect_button_mode(&frame, &rois_fs);
        let fs_score = rois_fs.and_then_roi(&frame, "score", templates::detect_score);
        let fs_rate = rois_fs.and_then_roi(&frame, "rate", |img| templates::detect_rate(img));

        println!(
            "  [인식 결과] Result: mode={:?}, diff={:?}, score={:?}, rate={:?}",
            res_mode, res_diff, res_score, res_rate
        );
        println!(
            "  [인식 결과] Freestyle: mode={:?}, score={:?}, rate={:?}",
            fs_mode, fs_score, fs_rate
        );

        // 스냅샷별 정밀 Assertion 검증
        if file_name.contains("hdr_snapshot1") {
            assert_eq!(detected_scene, SceneType::Freestyle);
            assert_eq!(match_fs.as_ref().map(|m| m.image_id.as_str()), Some("733"));
            assert_eq!(fs_mode, Some(Mode::B6));
            assert_eq!(fs_score, Some(982342));
            assert_eq!(fs_rate, Some(98.23));
            assert_eq!(res_diff, Some(Difficulty::SC));
        } else if file_name.contains("hdr_snapshot2") {
            assert_eq!(detected_scene, SceneType::Freestyle);
            assert_eq!(match_fs.as_ref().map(|m| m.image_id.as_str()), Some("733"));
            assert_eq!(fs_mode, Some(Mode::B4));
            assert_eq!(fs_score, Some(989966));
            assert_eq!(fs_rate, Some(98.99));
            assert_eq!(res_diff, Some(Difficulty::SC));
        } else if file_name.contains("acidinvasion") {
            assert_eq!(detected_scene, SceneType::ResultFreestyle);
            assert_eq!(match_res.as_ref().map(|m| m.image_id.as_str()), Some("817"));
            assert_eq!(res_mode, Some(Mode::B8));
            assert_eq!(res_diff, Some(Difficulty::SC));
            assert_eq!(res_score, Some(995923));
            assert_eq!(res_rate, Some(99.59));
        }
    }
}
