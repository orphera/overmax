#![cfg(windows)]

use std::path::{Path, PathBuf};

fn find_snapshot_dir() -> Option<PathBuf> {
    let candidates = [
        Path::new("../../scratch/hdr_snapshot"),
        Path::new("scratch/hdr_snapshot"),
        Path::new("../../scratch/hdr"),
        Path::new("../../scratch/hdr/hdr_snapshots"),
        Path::new("scratch/hdr"),
        Path::new("scratch/hdr/hdr_snapshots"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.to_path_buf())
}

fn find_raw_file(name: &str) -> Option<PathBuf> {
    let candidates = [
        format!("../../scratch/hdr_snapshot/{}", name),
        format!("scratch/hdr_snapshot/{}", name),
        format!("../../scratch/hdr/{}", name),
        format!("../../scratch/hdr/hdr_snapshots/{}", name),
        format!("scratch/hdr/{}", name),
        format!("scratch/hdr/hdr_snapshots/{}", name),
    ];
    candidates
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

#[cfg(windows)]
#[test]
fn test_analyze_all_hdr_snapshots() {
    let target_dir = match find_snapshot_dir() {
        Some(d) => d,
        None => {
            println!("[HDR Replay Test] Notice: 'scratch/hdr' directory not found.");
            return;
        }
    };

    let entries = match std::fs::read_dir(&target_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
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

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::f16_to_f32;

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
    let scale =
        overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL;
    let mut bgra_lut = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
        let dst_row = unsafe { bgra_lut.as_mut_ptr().add(y * WIDTH * 4) };
        unsafe {
            overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3(
                src_row, dst_row, WIDTH, scale,
            );
        }
    }

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

#[cfg(windows)]
#[test]
fn test_scale_sweep() {
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        build_lut_table, convert_scrgb_fp16_to_bgra8_with_lut,
    };

    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(actual_db.unwrap(), 0.50);
    db.load().unwrap();
    let _matcher = db.matcher();

    let candidates = [
        Path::new("../../scratch/hdr/hdr_snapshot_1788930865.raw"),
        Path::new("../../scratch/hdr/hdr_snapshots/hdr_snapshot_1788930865.raw"),
        Path::new("scratch/hdr/hdr_snapshot_1788930865.raw"),
        Path::new("scratch/hdr/hdr_snapshots/hdr_snapshot_1788930865.raw"),
    ];
    let actual_raw = match candidates.iter().find(|p| p.exists()) {
        Some(p) => *p,
        None => return,
    };

    let bytes = std::fs::read(actual_raw).unwrap();
    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    println!("\n=== SCALE SWEEP TEST: hdr_snapshot_1788930865.raw ===");
    for scale_int in (10..=60).step_by(5) {
        let scale = scale_int as f32 / 10.0;
        let lut = build_lut_table(scale);
        let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];

        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_with_lut(src_row, dst_row, WIDTH, &lut);
            }
        }

        let mut fs_jacket = Vec::with_capacity(64 * 60 * 4);
        for row in 0..60 {
            let row_start = (94 + row) * WIDTH * 4 + 340 * 4;
            fs_jacket.extend_from_slice(&bgra[row_start..row_start + 64 * 4]);
        }

        let mut mask_bits: u64 = 0;
        for x in 0..8 {
            mask_bits |= 1 << x;
        }
        for y in 0..8 {
            mask_bits |= 1 << (y * 8 + 7);
        }
        mask_bits |= 1 << 8;
        let hash_mask = !mask_bits;

        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(&fs_jacket, 64, 60, 4).unwrap();
        let q_grid_hist = overmax_cv::compute_grid_histogram(&fs_jacket, 64, 60, 4);

        let entry = db.entries().iter().find(|e| e.image_id == "246").unwrap();
        let p_dist = (entry.phash ^ q_phash).count_ones();
        let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
        let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();
        let hamming = p_dist + d_dist + a_dist;

        let hist_diff: u32 = entry
            .grid_hist
            .unwrap()
            .iter()
            .zip(q_grid_hist.iter())
            .map(|(&e, &q)| e.abs_diff(q) as u32)
            .sum();

        let hist_sim = 1.0 - (hist_diff as f32 / 3072.0).clamp(0.0, 1.0);
        let hash_sim = 1.0 - (hamming as f32 / 160.0);
        let sim = 0.5 * hash_sim + 0.5 * hist_sim;
        println!(
            "Scale {:<5.2} -> Hamming: {:<2} (p={}, d={}, a={}), HistDiff: {:<4} | HashSim: {:.3}, HistSim: {:.3} => TotalSim: {:.4}",
            scale, hamming, p_dist, d_dist, a_dist, hist_diff, hash_sim, hist_sim, sim
        );
    }

    let png_candidates = [
        Path::new("../../scratch/hdr/DJMAX RESPECT V 2026-09-09 14_45_30.png"),
        Path::new("../../scratch/hdr/Captures/DJMAX RESPECT V 2026-09-09 14_45_30.png"),
        Path::new("scratch/hdr/DJMAX RESPECT V 2026-09-09 14_45_30.png"),
        Path::new("scratch/hdr/Captures/DJMAX RESPECT V 2026-09-09 14_45_30.png"),
    ];
    let actual_png = png_candidates.iter().find(|p| p.exists());

    if let Some(p) = actual_png {
        let img = image::open(p).unwrap().to_rgba8();
        let (w, _h) = (img.width() as usize, img.height() as usize);
        let mut raw_bytes = img.into_raw();
        // swap RGBA to BGRA
        for px in raw_bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        // Freestyle jacket at 1080p: x=710, y=533, w=64, h=60
        let mut fs_jacket = Vec::with_capacity(64 * 60 * 4);
        for row in 0..60 {
            let row_start = (533 + row) * w * 4 + 710 * 4;
            fs_jacket.extend_from_slice(&raw_bytes[row_start..row_start + 64 * 4]);
        }
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(&fs_jacket, 64, 60, 4).unwrap();
        let q_grid_hist = overmax_cv::compute_grid_histogram(&fs_jacket, 64, 60, 4);

        let entry = db.entries().iter().find(|e| e.image_id == "246").unwrap();
        let p_dist = (entry.phash ^ q_phash).count_ones();
        let mut mask_bits: u64 = 0;
        for x in 0..8 {
            mask_bits |= 1 << x;
        }
        for y in 0..8 {
            mask_bits |= 1 << (y * 8 + 7);
        }
        mask_bits |= 1 << 8;
        let hash_mask = !mask_bits;

        let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
        let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();
        let hamming = p_dist + d_dist + a_dist;

        let hist_diff: u32 = entry
            .grid_hist
            .unwrap()
            .iter()
            .zip(q_grid_hist.iter())
            .map(|(&e, &q)| e.abs_diff(q) as u32)
            .sum();

        let hist_sim = 1.0 - (hist_diff as f32 / 3072.0).clamp(0.0, 1.0);
        let hash_sim = 1.0 - (hamming as f32 / 160.0);
        let sim = 0.5 * hash_sim + 0.5 * hist_sim;

        println!(
            "PNG Jacket -> Hamming: {:<2} (p={}, d={}, a={}), HistDiff: {:<4} | HashSim: {:.3}, HistSim: {:.3} => TotalSim: {:.4}",
            hamming, p_dist, d_dist, a_dist, hist_diff, hash_sim, hist_sim, sim
        );

        println!("\n=== Top 5 candidates for OpenMatch jacket (15_05_39.png) ===");
        let png_p_candidates = [
            Path::new("../../scratch/hdr/DJMAX RESPECT V 2026-09-09 15_05_39.png"),
            Path::new("../../scratch/hdr/Captures/DJMAX RESPECT V 2026-09-09 15_05_39.png"),
            Path::new("scratch/hdr/DJMAX RESPECT V 2026-09-09 15_05_39.png"),
            Path::new("scratch/hdr/Captures/DJMAX RESPECT V 2026-09-09 15_05_39.png"),
        ];
        let actual_p = match png_p_candidates.iter().find(|p| p.exists()) {
            Some(p) => *p,
            None => return,
        };
        let img = image::open(actual_p).unwrap().to_rgba8();
        let w = img.width() as usize;
        let mut raw_bytes = img.into_raw();
        for px in raw_bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let mut om_jacket = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let row_start = (533 + row) * w * 4 + 664 * 4;
            om_jacket.extend_from_slice(&raw_bytes[row_start..row_start + 60 * 4]);
        }
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(&om_jacket, 60, 60, 4).unwrap();
        let q_grid_hist = overmax_cv::compute_grid_histogram(&om_jacket, 60, 60, 4);

        let mut all_sims = Vec::new();
        for entry in db.entries().iter() {
            let p_dist = (entry.phash ^ q_phash).count_ones();
            let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
            let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();
            let hamming = p_dist + d_dist + a_dist;

            let hist_diff: u32 = if let Some(h) = entry.grid_hist {
                h.iter()
                    .zip(q_grid_hist.iter())
                    .map(|(&e, &q)| e.abs_diff(q) as u32)
                    .sum()
            } else {
                3072
            };

            let hist_sim = 1.0 - (hist_diff as f32 / 3072.0).clamp(0.0, 1.0);
            let hash_sim = 1.0 - (hamming as f32 / 160.0);
            let sim = 0.5 * hash_sim + 0.5 * hist_sim;
            all_sims.push((sim, entry.image_id.clone(), hamming, hist_diff));
        }
        all_sims.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for (sim, id, ham, hdiff) in &all_sims[..5] {
            println!(
                "  ID: {:<6} Sim: {:.4} (Hamming: {}, HistDiff: {})",
                id, sim, ham, hdiff
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn test_all_captures_png() {
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::detection_pipeline::detect_static_scene;
    use overmax_engine::detector::roi::RoiManager;

    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(actual_db.unwrap(), 0.60);
    db.load().unwrap();
    let matcher = db.matcher();

    let cap_dir = Path::new("../../scratch/captures_client");
    let fallback_cap_dir = Path::new("scratch/captures_client");
    let actual_dir = if cap_dir.exists() {
        cap_dir
    } else if fallback_cap_dir.exists() {
        fallback_cap_dir
    } else {
        return;
    };

    let mut png_files: Vec<PathBuf> = std::fs::read_dir(actual_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .collect();
    png_files.sort();

    println!("\n==================================================");
    println!(" [TEST ALL CAPTURES PNG] Total: {}", png_files.len());
    println!("==================================================");

    for p in &png_files {
        let fname = p.file_name().unwrap().to_string_lossy();
        let img = image::open(p).unwrap().to_rgba8();
        let (w, h) = (img.width() as i32, img.height() as i32);
        let mut raw_bytes = img.into_raw();
        for px in raw_bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let frame = CapturedFrame {
            width: w,
            height: h,
            bgra: raw_bytes,
        };

        let rois = RoiManager::new(w, h);

        // Check freestyle jacket directly
        if let Some(j_roi) = rois.get_roi_for_scene("jacket", overmax_core::SceneType::Freestyle) {
            if let Some(j_img) = j_roi.crop(&frame) {
                let region = j_img.to_image_region();
                let m = matcher.match_jacket(
                    &region.bgra,
                    region.width as usize,
                    region.height as usize,
                    4,
                );
                print!("  [FS Jacket] ");
                if let Some(res) = m {
                    print!("ID={}, Sim={:.4} ", res.image_id, res.similarity);
                } else {
                    print!("None ");
                }
            }
        }

        // Check result jacket directly
        if let Some(j_roi) =
            rois.get_roi_for_scene("jacket", overmax_core::SceneType::ResultFreestyle)
        {
            if let Some(j_img) = j_roi.crop(&frame) {
                let region = j_img.to_image_region();
                let m = matcher.match_jacket(
                    &region.bgra,
                    region.width as usize,
                    region.height as usize,
                    4,
                );
                print!("| [RES Jacket] ");
                if let Some(res) = m {
                    print!("ID={}, Sim={:.4} ", res.image_id, res.similarity);
                } else {
                    print!("None ");
                }
            }
        }
        println!();

        let scene = detect_static_scene(&frame, &rois, &matcher);
        println!("{:40} -> Scene: {:?}", fname, scene);
    }
}

#[cfg(windows)]
#[test]
fn test_sdr_baseline() {
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::detection_pipeline::detect_static_scene;
    use overmax_engine::detector::roi::RoiManager;

    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(actual_db.unwrap(), 0.60);
    db.load().unwrap();
    let matcher = db.matcher();

    println!("\n==================================================");
    println!(" [SDR BASELINE TEST]");
    println!("==================================================");

    for idx in 1..=5 {
        let p_str = format!("../../scratch/hd_test_{}.png", idx);
        let fb_str = format!("scratch/hd_test_{}.png", idx);
        let p = if Path::new(&p_str).exists() {
            Path::new(&p_str)
        } else if Path::new(&fb_str).exists() {
            Path::new(&fb_str)
        } else {
            continue;
        };

        let img = image::open(p).unwrap().to_rgba8();
        let (w, h) = (img.width() as i32, img.height() as i32);
        let mut raw_bytes = img.into_raw();
        for px in raw_bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let frame = CapturedFrame {
            width: w,
            height: h,
            bgra: raw_bytes,
        };

        let rois = RoiManager::new(w, h);
        let scene = detect_static_scene(&frame, &rois, &matcher);

        // Check jacket
        let mut fs_match_str = "None".to_string();
        if let Some(j_roi) = rois.get_roi_for_scene("jacket", overmax_core::SceneType::Freestyle) {
            if let Some(j_img) = j_roi.crop(&frame) {
                let region = j_img.to_image_region();
                if let Some(res) = matcher.match_jacket(
                    &region.bgra,
                    region.width as usize,
                    region.height as usize,
                    4,
                ) {
                    fs_match_str = format!("ID={}, Sim={:.4}", res.image_id, res.similarity);
                }
            }
        }

        println!(
            "hd_test_{}.png -> Scene: {:?}, FS Jacket: {}",
            idx, scene, fs_match_str
        );
        if let Some(mode_roi) =
            rois.get_roi_for_scene("openmatch_mode", overmax_core::SceneType::ResultOpen3)
        {
            let mean = overmax_engine::capture::frame_utils::region_mean_bgr(&frame, mode_roi);
            println!(
                "  [ResultOpen3 openmatch_mode] BGR=[{}, {}, {}]",
                mean.b, mean.g, mean.r
            );
        }
        if let Some(mode_roi) =
            rois.get_roi_for_scene("openmatch_mode", overmax_core::SceneType::ResultOpen2)
        {
            let mean = overmax_engine::capture::frame_utils::region_mean_bgr(&frame, mode_roi);
            println!(
                "  [ResultOpen2 openmatch_mode] BGR=[{}, {}, {}]",
                mean.b, mean.g, mean.r
            );
        }
    }
}

#[test]
#[cfg(windows)]
fn test_diagnose_unknown_snapshots() {
    let base_dir = Path::new("tests/fixtures");
    let base_dir_alt = Path::new("rust/overmax_engine/tests/fixtures");
    let _fixtures = if base_dir.exists() {
        base_dir
    } else {
        base_dir_alt
    };

    let image_db = if Path::new("../../cache/image_index.db").exists() {
        Some(Path::new("../../cache/image_index.db"))
    } else if Path::new("cache/image_index.db").exists() {
        Some(Path::new("cache/image_index.db"))
    } else {
        None
    };

    let Some(image_db) = image_db else {
        println!(
            "[HDR Replay Test] Skipping test_diagnose_unknown_snapshots: image_index.db not found"
        );
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(image_db, 0.0);
    if db.load().is_err() {
        println!("[HDR Replay Test] Skipping test_diagnose_unknown_snapshots: failed to load DB");
        return;
    }
    let matcher = db.matcher();

    let unknown_files = [
        "hdr_snapshot_1788930903.raw",
        "hdr_snapshot_1788931091.raw",
        "hdr_snapshot_1788931134.raw",
        "hdr_snapshot_1788931261.raw",
    ];

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8;

    let _ = std::fs::create_dir_all("scratch/unknown_jackets");

    for fname in unknown_files {
        let Some(p) = find_raw_file(fname) else {
            println!("[HDR Replay Test] Skipping {}, file not found", fname);
            continue;
        };

        let bytes = std::fs::read(p).expect("Failed to read raw file");
        const WIDTH: usize = 512;
        const HEIGHT: usize = 512;

        let mut bgra_lut = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra_lut.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8(src_row, dst_row, WIDTH);
            }
        }

        // Extract FS Jacket (340, 94, 60, 60)
        let mut fs_jacket = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let row_start = (94 + row) * WIDTH * 4 + 340 * 4;
            fs_jacket.extend_from_slice(&bgra_lut[row_start..row_start + 60 * 4]);
        }

        // Save jacket png for visual inspection
        let mut rgba = fs_jacket.clone();
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let _ = image::save_buffer(
            format!("scratch/unknown_jackets/{}.png", fname.replace(".raw", "")),
            &rgba,
            60,
            60,
            image::ColorType::Rgba8,
        );

        let centroid_pass = matcher.check_centroid_kernel(&fs_jacket, 60, 60, 4);
        let res = matcher.match_jacket(&fs_jacket, 60, 60, 4);

        println!("\n--- Diagnostic for {} ---", fname);
        println!("  Centroid kernel pass: {}", centroid_pass);
        if let Some(m) = res {
            println!(
                "  Best Match (threshold=0.0): ID={}, Similarity={:.4}",
                m.image_id, m.similarity
            );
        } else {
            println!("  Best Match: None (Early exit before scoring)");
        }
    }

    // Compare test_j_cur vs test_j_p3 for Dreamscape
    let cur_res = image::open("../../scratch/test_j_cur.png")
        .or_else(|_| image::open("scratch/test_j_cur.png"));
    let p3_res = image::open("../../scratch/test_j_p3.png")
        .or_else(|_| image::open("scratch/test_j_p3.png"));

    if let (Ok(cur_img), Ok(p3_img)) = (cur_res, p3_res) {
        let cur_img = cur_img.to_rgba8();
        let p3_img = p3_img.to_rgba8();

        let mut cur_bgra = cur_img.into_raw();
        for px in cur_bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let mut p3_bgra = p3_img.into_raw();
        for px in p3_bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let match_cur = matcher.match_jacket(&cur_bgra, 60, 60, 4);
        let match_p3 = matcher.match_jacket(&p3_bgra, 60, 60, 4);

        println!("\n=== DREAMSCAPE 802 JACKET RECOVERY COMPARISON ===");
        println!(
            "  Current method match: {:?}",
            match_cur.map(|m| format!("ID={}, sim={:.4}", m.image_id, m.similarity))
        );
        println!(
            "  DCI-P3 Gamut match:   {:?}",
            match_p3.map(|m| format!("ID={}, sim={:.4}", m.image_id, m.similarity))
        );
    }

    // Compare Away (625) and Brain Storm (52)
    let away_res = image::open("../../scratch/test_j_away_p3.png")
        .or_else(|_| image::open("scratch/test_j_away_p3.png"));
    let bs_res = image::open("../../scratch/test_j_brainstorm_p3.png")
        .or_else(|_| image::open("scratch/test_j_brainstorm_p3.png"));

    if let (Ok(away_img), Ok(bs_img)) = (away_res, bs_res) {
        let away_img = away_img.to_rgba8();
        let mut away_bgra = away_img.into_raw();
        for px in away_bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let match_away = matcher.match_jacket(&away_bgra, 60, 60, 4);

        let bs_img = bs_img.to_rgba8();
        let mut bs_bgra = bs_img.into_raw();
        for px in bs_bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let match_bs = matcher.match_jacket(&bs_bgra, 60, 60, 4);

        println!("\n=== AWAY (625) & BRAIN STORM (52) P3 RECOVERY ===");
        println!(
            "  Away P3 match:        {:?}",
            match_away.map(|m| format!("ID={}, sim={:.4}", m.image_id, m.similarity))
        );
        println!(
            "  Brain Storm P3 match: {:?}",
            match_bs.map(|m| format!("ID={}, sim={:.4}", m.image_id, m.similarity))
        );
    }
}

#[test]
#[cfg(windows)]
fn test_diagnose_result_scenes() {
    let result_files = [
        ("30 KICK IT", "hdr_snapshot_1788932716.raw"),
        ("31 BlueWhite", "hdr_snapshot_1788932936.raw"),
        ("32 Apparition", "hdr_snapshot_1788933159.raw"),
        ("33 Don't Fight", "hdr_snapshot_1788933535.raw"),
        ("34 Don't Die", "hdr_snapshot_1788933694.raw"),
        ("35 Dreamscape", "hdr_snapshot_1788933902.raw"),
    ];

    use overmax_core::SceneType;
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::detection_pipeline::{
        check_open_match_badge, detect_freestyle_result_colorbar_match,
    };
    use overmax_engine::detector::roi::RoiManager;

    for (label, fname) in result_files {
        let Some(p) = find_raw_file(fname) else {
            println!(
                "[HDR Replay Test] Skipping {} ({}), file not found",
                label, fname
            );
            continue;
        };

        let bytes = std::fs::read(p).expect("Failed to read raw file");
        const WIDTH: usize = 512;
        const HEIGHT: usize = 512;

        let mut bgra_lut = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra_lut.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3(
                    src_row,
                    dst_row,
                    WIDTH,
                    overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL,
                );
            }
        }

        let frame = CapturedFrame {
            width: WIDTH as i32,
            height: HEIGHT as i32,
            bgra: bgra_lut,
        };

        let rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
        let colorbar_roi = rois
            .get_roi_for_scene("mode_colorbar", SceneType::ResultFreestyle)
            .unwrap();
        let mean = overmax_engine::capture::frame_utils::region_mean_bgr(&frame, colorbar_roi);
        let is_fs_colorbar = detect_freestyle_result_colorbar_match(mean);

        let open_badge = check_open_match_badge(&frame, &rois);

        println!("\n=== Diagnostic for {} ({}) ===", label, fname);
        println!(
            "  Colorbar mean BGR: [B={}, G={}, R={}]",
            mean.b, mean.g, mean.r
        );
        println!("  is_fs_colorbar: {}", is_fs_colorbar);
        println!("  check_open_match_badge: {:?}", open_badge);

        if fname.contains("1788933902") {
            // Dreamscape deep inspection
            let cb_rect = colorbar_roi;
            println!(
                "  [Dreamscape Colorbar Detailed Pixels (Atlas x={}, y={}, w={}, h={})]",
                cb_rect.x1,
                cb_rect.y1,
                cb_rect.width(),
                cb_rect.height()
            );
            let mut sum_r = 0.0f32;
            let mut sum_g = 0.0f32;
            let mut sum_b = 0.0f32;
            let count = (cb_rect.width() * cb_rect.height()) as usize;
            for cy in 0..cb_rect.height() {
                for cx in 0..cb_rect.width() {
                    let ax = (cb_rect.x1 + cx) as usize;
                    let ay = (cb_rect.y1 + cy) as usize;
                    let off = (ay * WIDTH + ax) * 8;
                    let r_raw =
                        overmax_engine::capture::capture_engine::windows::hdr_pipeline::f16_to_f32(
                            u16::from_ne_bytes([bytes[off], bytes[off + 1]]),
                        );
                    let g_raw =
                        overmax_engine::capture::capture_engine::windows::hdr_pipeline::f16_to_f32(
                            u16::from_ne_bytes([bytes[off + 2], bytes[off + 3]]),
                        );
                    let b_raw =
                        overmax_engine::capture::capture_engine::windows::hdr_pipeline::f16_to_f32(
                            u16::from_ne_bytes([bytes[off + 4], bytes[off + 5]]),
                        );
                    sum_r += r_raw;
                    sum_g += g_raw;
                    sum_b += b_raw;
                }
            }
            println!(
                "  Mean RAW scRGB: R={:.4}, G={:.4}, B={:.4}",
                sum_r / count as f32,
                sum_g / count as f32,
                sum_b / count as f32
            );
        }
    }

    let detected = overmax_engine::capture::capture_engine::windows::hdr_pipeline::detect_monitor_sdr_white_level(None);
    println!(
        "\n>>> [System Check] detect_monitor_sdr_white_level(None) = {:?}",
        detected
    );
}

#[test]
#[cfg(windows)]
fn test_benchmark_p3_conversion_speed() {
    let Some(p) = find_raw_file("hdr_snapshot_1788930865.raw") else {
        println!("[HDR Replay Test] Benchmark skipped: raw file not found");
        return;
    };
    let bytes = std::fs::read(p).expect("Failed to read raw file");

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;
    let mut dst = vec![0u8; WIDTH * HEIGHT * 4];

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        convert_scrgb_fp16_to_bgra8_p3, DEFAULT_SDR_WHITE_LEVEL,
    };
    use std::time::Instant;

    // Warm up
    for y in 0..HEIGHT {
        let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
        let dst_row = unsafe { dst.as_mut_ptr().add(y * WIDTH * 4) };
        unsafe {
            convert_scrgb_fp16_to_bgra8_p3(src_row, dst_row, WIDTH, DEFAULT_SDR_WHITE_LEVEL);
        }
    }

    // Benchmark 1,000 iterations
    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { dst.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_p3(src_row, dst_row, WIDTH, DEFAULT_SDR_WHITE_LEVEL);
            }
        }
    }
    let elapsed = start.elapsed();
    let per_frame_us = elapsed.as_micros() as f64 / iterations as f64;
    let per_frame_ms = per_frame_us / 1000.0;

    println!("\n==================================================");
    println!(" [BENCHMARK] convert_scrgb_fp16_to_bgra8_p3 (512x512 Atlas)");
    println!("  Total iterations: {}", iterations);
    println!(
        "  Average per-frame time: {:.3} ms ({:.1} us)",
        per_frame_ms, per_frame_us
    );
    println!("  Max possible FPS: {:.1} FPS", 1000.0 / per_frame_ms);
    println!("==================================================");
}

#[test]
#[cfg(windows)]
fn test_all_snapshots_summary() {
    let Some(target_dir) = find_snapshot_dir() else {
        println!("[HDR Replay Test] Summary skipped: snapshot dir not found");
        return;
    };

    let Ok(read_dir) = std::fs::read_dir(&target_dir) else {
        return;
    };

    let mut entries: Vec<PathBuf> = read_dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("raw"))
        .collect();
    entries.sort();

    let db_path = if Path::new("../../cache/image_index.db").exists() {
        Some(Path::new("../../cache/image_index.db"))
    } else if Path::new("cache/image_index.db").exists() {
        Some(Path::new("cache/image_index.db"))
    } else {
        None
    };
    let Some(db_path) = db_path else {
        println!("[HDR Replay Test] Summary skipped: image_index.db not found");
        return;
    };
    let mut db = overmax_data::store::image_index::ImageIndexDb::new(db_path, 0.0);
    if db.load().is_err() {
        println!("[HDR Replay Test] Summary skipped: failed to load ImageIndexDb");
        return;
    }
    let matcher = db.matcher();

    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        convert_scrgb_fp16_to_bgra8_p3, DEFAULT_SDR_WHITE_LEVEL,
    };
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::detection_pipeline::detect_static_scene;
    use overmax_engine::detector::roi::RoiManager;

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    println!(
        "\n{:<28} | {:<16} | {:<20} | {:<20}",
        "File", "Detected Scene", "FS Jacket Match", "Result Jacket Match"
    );
    println!("{:-<28}-|-{:-<16}-|-{:-<20}-|-{:-<20}", "", "", "", "");

    let mut out_lines = Vec::new();
    out_lines.push(format!(
        "{:<28} | {:<16} | {:<20} | {:<20}",
        "File", "Detected Scene", "FS Jacket Match", "Result Jacket Match"
    ));

    for p in &entries {
        let fname = p.file_name().unwrap().to_string_lossy();
        let bytes = std::fs::read(p).unwrap();
        if bytes.len() < WIDTH * HEIGHT * 8 {
            continue;
        }

        let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_p3(src_row, dst_row, WIDTH, DEFAULT_SDR_WHITE_LEVEL);
            }
        }

        let frame = CapturedFrame {
            width: WIDTH as i32,
            height: HEIGHT as i32,
            bgra: bgra.clone(),
        };
        let rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
        let scene = detect_static_scene(&frame, &rois, &matcher);

        // FS Jacket (340, 94, 60, 60)
        let mut fs_j = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (94 + row) * WIDTH * 4 + 340 * 4;
            fs_j.extend_from_slice(&bgra[start..start + 60 * 4]);
        }
        let fs_match = matcher.match_jacket(&fs_j, 60, 60, 4);
        let fs_cpass = matcher.check_centroid_kernel(&fs_j, 60, 60, 4);
        let fs_str = if let Some(m) = fs_match {
            format!(
                "ID={}, s={:.3}{}",
                m.image_id,
                m.similarity,
                if fs_cpass { "" } else { " (C-FAIL)" }
            )
        } else {
            "None".to_string()
        };

        // Result Jacket (75, 395, 60, 60)
        let mut res_j = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (395 + row) * WIDTH * 4 + 75 * 4;
            res_j.extend_from_slice(&bgra[start..start + 60 * 4]);
        }
        let res_match = matcher.match_jacket(&res_j, 60, 60, 4);
        let res_cpass = matcher.check_centroid_kernel(&res_j, 60, 60, 4);
        let res_str = if let Some(m) = res_match {
            format!(
                "ID={}, s={:.3}{}",
                m.image_id,
                m.similarity,
                if res_cpass { "" } else { " (C-FAIL)" }
            )
        } else {
            "None".to_string()
        };

        eprintln!(
            "{:<28} | {:<16?} | {:<20} | {:<20}",
            fname, scene, fs_str, res_str
        );
        out_lines.push(format!(
            "{:<28} | {:<16?} | {:<20} | {:<20}",
            fname, scene, fs_str, res_str
        ));
    }
    let _ = std::fs::write("scratch/all_snapshots_result.txt", out_lines.join("\n"));
}

#[cfg(windows)]
#[test]
fn test_captures_and_snapshots_play_state() {
    use overmax_core::SceneType;
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3;
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::detection_pipeline::detect_static_scene;
    use overmax_engine::detector::play_state::{detect_button_mode, detect_difficulty};
    use overmax_engine::detector::roi::RoiManager;
    use overmax_engine::detector::templates;

    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(actual_db.unwrap(), 0.50);
    db.load().unwrap();
    let matcher = db.matcher();

    // 0. SDR Baseline (hd_test_*.png) 710 vs 709 comparison
    println!("\n=======================================================");
    println!(" [0. SDR BASELINE] Freestyle Jacket ROI: 710 vs 709");
    println!("=======================================================");
    for idx in 1..=5 {
        let p_str = format!("../../scratch/hd_test_{}.png", idx);
        let fb_str = format!("scratch/hd_test_{}.png", idx);
        let p = if Path::new(&p_str).exists() {
            Path::new(&p_str)
        } else if Path::new(&fb_str).exists() {
            Path::new(&fb_str)
        } else {
            continue;
        };

        let img = image::open(p).unwrap().to_rgba8();
        let (w, _h) = (img.width() as usize, img.height() as usize);
        let mut raw_bytes = img.into_raw();
        for px in raw_bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let mut j710 = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (533 + row) * w * 4 + 710 * 4;
            j710.extend_from_slice(&raw_bytes[start..start + 60 * 4]);
        }
        let m710 = matcher.match_jacket(&j710, 60, 60, 4);

        let mut j709 = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (533 + row) * w * 4 + 709 * 4;
            j709.extend_from_slice(&raw_bytes[start..start + 60 * 4]);
        }
        let m709 = matcher.match_jacket(&j709, 60, 60, 4);

        println!(
            "hd_test_{}.png | x=710: {:<20} | x=709: {:<20}",
            idx,
            m710.map(|m| format!("ID={}, s={:.4}", m.image_id, m.similarity))
                .unwrap_or("-".into()),
            m709.map(|m| format!("ID={}, s={:.4}", m.image_id, m.similarity))
                .unwrap_or("-".into()),
        );
    }

    // 1. Captures/*.png (Windowed vs Client area 710 vs 709)
    println!("\n=======================================================");
    println!(" [1. CAPTURES PNG] (Client area y=31, x=1) 710 vs 709");
    println!("=======================================================");
    let cap_dir = Path::new("../../scratch/hdr/Captures");
    let fallback_cap_dir = Path::new("scratch/hdr/Captures");
    let actual_cap = if cap_dir.exists() {
        Some(cap_dir)
    } else if fallback_cap_dir.exists() {
        Some(fallback_cap_dir)
    } else {
        None
    };

    if let Some(actual_cap) = actual_cap {
        let mut png_files: Vec<PathBuf> = std::fs::read_dir(actual_cap)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("png"))
            .collect();
        png_files.sort();

        for p in &png_files {
            let fname = p.file_name().unwrap().to_string_lossy();
            let img = image::open(p).unwrap().to_rgba8();
            let (w, _h) = (img.width() as usize, img.height() as usize);
            let mut raw_bytes = img.into_raw();
            for px in raw_bytes.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            // Direct crop (x=710, y=533)
            let mut j_direct_710 = Vec::with_capacity(60 * 60 * 4);
            for row in 0..60 {
                let start = (533 + row) * w * 4 + 710 * 4;
                j_direct_710.extend_from_slice(&raw_bytes[start..start + 60 * 4]);
            }
            let m_d710 = matcher.match_jacket(&j_direct_710, 60, 60, 4);

            // Client area offset (x=710+1=711, y=533+31=564)
            let mut j_offset_710 = Vec::with_capacity(60 * 60 * 4);
            for row in 0..60 {
                let start = (564 + row) * w * 4 + 711 * 4;
                j_offset_710.extend_from_slice(&raw_bytes[start..start + 60 * 4]);
            }
            let m_o710 = matcher.match_jacket(&j_offset_710, 60, 60, 4);

            // Client area offset with x-1 (x=709+1=710, y=533+31=564)
            let mut j_offset_709 = Vec::with_capacity(60 * 60 * 4);
            for row in 0..60 {
                let start = (564 + row) * w * 4 + 710 * 4;
                j_offset_709.extend_from_slice(&raw_bytes[start..start + 60 * 4]);
            }
            let m_o709 = matcher.match_jacket(&j_offset_709, 60, 60, 4);

            println!(
                "{:<28} | direct(710): {:<18} | offset(710): {:<18} | offset(709): {:<18}",
                &fname[fname.len().saturating_sub(28)..],
                m_d710
                    .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
                    .unwrap_or("-".into()),
                m_o710
                    .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
                    .unwrap_or("-".into()),
                m_o709
                    .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
                    .unwrap_or("-".into()),
            );
        }
    }

    // 2. hdr_snapshots/*.raw Atlas ROI: 340 (x=710) vs 339 (x=709) vs 341 (x=711)
    println!("\n===============================================================================");
    println!(" [2. HDR SNAPSHOTS RAW] Atlas Crop: 340 (x=710) vs 339 (x=709) vs 341 (x=711)");
    println!("===============================================================================");
    let actual_snap = match find_snapshot_dir() {
        Some(d) => d,
        None => return,
    };

    let mut raw_files: Vec<PathBuf> = std::fs::read_dir(actual_snap)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("raw"))
        .collect();
    raw_files.sort();

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    let mut raw_out = Vec::new();
    raw_out.push(format!(
        "{:<28} | {:<15} | {:<20} | {:<20} | {:<20} | {:<4} | {:<4} | {:<7} | {:<6}",
        "File",
        "Scene",
        "x=710 (atlas 340)",
        "x=709 (atlas 339)",
        "x=711 (atlas 341)",
        "Mode",
        "Diff",
        "Score",
        "Rate"
    ));

    for p in &raw_files {
        let fname = p.file_name().unwrap().to_string_lossy();
        let bytes = std::fs::read(p).unwrap();
        if bytes.len() < WIDTH * HEIGHT * 8 {
            continue;
        }

        let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            unsafe {
                let dst_row = bgra.as_mut_ptr().add(y * WIDTH * 4);
                convert_scrgb_fp16_to_bgra8_p3(
                    src_row,
                    dst_row,
                    WIDTH,
                    overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL,
                );
            }
        }

        let frame = CapturedFrame {
            width: WIDTH as i32,
            height: HEIGHT as i32,
            bgra: bgra.clone(),
        };
        let rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
        let scene = detect_static_scene(&frame, &rois, &matcher);

        // Crop atlas 340 (x=710)
        let mut j340 = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (94 + row) * WIDTH * 4 + 340 * 4;
            j340.extend_from_slice(&bgra[start..start + 60 * 4]);
        }
        let m340 = matcher.match_jacket(&j340, 60, 60, 4);

        // Crop atlas 339 (x=709)
        let mut j339 = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (94 + row) * WIDTH * 4 + 339 * 4;
            j339.extend_from_slice(&bgra[start..start + 60 * 4]);
        }
        let m339 = matcher.match_jacket(&j339, 60, 60, 4);

        // Crop atlas 341 (x=711)
        let mut j341 = Vec::with_capacity(60 * 60 * 4);
        for row in 0..60 {
            let start = (94 + row) * WIDTH * 4 + 341 * 4;
            j341.extend_from_slice(&bgra[start..start + 60 * 4]);
        }
        let m341 = matcher.match_jacket(&j341, 60, 60, 4);

        let str340 = m340
            .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
            .unwrap_or_else(|| "-".to_string());
        let str339 = m339
            .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
            .unwrap_or_else(|| "-".to_string());
        let str341 = m341
            .map(|m| format!("ID={}, s={:.3}", m.image_id, m.similarity))
            .unwrap_or_else(|| "-".to_string());

        // PlayState
        let mut rois_active = rois.clone();
        rois_active.set_scene(scene);

        let (mode_str, diff_str, score_str, rate_str) = match scene {
            SceneType::ResultFreestyle => {
                let m = rois_active.and_then_roi(
                    &frame,
                    "mode_digit",
                    templates::detect_freestyle_mode,
                );
                let d = rois_active.and_then_roi(
                    &frame,
                    "diff_panel",
                    templates::detect_result_difficulty,
                );
                let sc = rois_active.and_then_roi(&frame, "score", templates::detect_score);
                let r = rois_active.and_then_roi(&frame, "rate", |img| templates::detect_rate(img));
                (
                    m.map(|v| format!("{:?}", v)).unwrap_or("-".into()),
                    d.map(|v| format!("{:?}", v)).unwrap_or("-".into()),
                    sc.map(|v| v.to_string()).unwrap_or("-".into()),
                    r.map(|v| format!("{:.2}%", v)).unwrap_or("-".into()),
                )
            }
            SceneType::Freestyle => {
                let m = detect_button_mode(&frame, &rois_active);
                let (d, _) = detect_difficulty(&frame, &rois_active);
                let sc = rois_active.and_then_roi(&frame, "score", templates::detect_score);
                let r = rois_active.and_then_roi(&frame, "rate", |img| templates::detect_rate(img));
                (
                    m.map(|v| format!("{:?}", v)).unwrap_or("-".into()),
                    d.map(|v| format!("{:?}", v)).unwrap_or("-".into()),
                    sc.map(|v| v.to_string()).unwrap_or("-".into()),
                    r.map(|v| format!("{:.2}%", v)).unwrap_or("-".into()),
                )
            }
            _ => ("-".into(), "-".into(), "-".into(), "-".into()),
        };

        eprintln!(
            "{:<28} | {:<15?} | {:<20} | {:<20} | {:<20} | {:<4} | {:<4} | {:<7} | {:<6}",
            fname, scene, str340, str339, str341, mode_str, diff_str, score_str, rate_str
        );
        raw_out.push(format!(
            "{:<28} | {:<15?} | {:<20} | {:<20} | {:<20} | {:<4} | {:<4} | {:<7} | {:<6}",
            fname, scene, str340, str339, str341, mode_str, diff_str, score_str, rate_str
        ));
    }
    let _ = std::fs::write(
        "scratch/compare_709_vs_710_and_play_state.txt",
        raw_out.join("\n"),
    );
}

#[cfg(windows)]
#[test]
fn test_export_snapshots_to_png() {
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3;

    let Some(actual_snap) = find_snapshot_dir() else {
        return;
    };

    let out_dir = actual_snap.join("hdr_snapshots_png");
    std::fs::create_dir_all(&out_dir).expect("Failed to create out dir");

    let Ok(read_dir) = std::fs::read_dir(&actual_snap) else {
        return;
    };
    let mut raw_files: Vec<PathBuf> = read_dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("raw"))
        .collect();
    raw_files.sort();

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    println!(
        "\nConverting {} raw snapshots to PNG in {:?}...",
        raw_files.len(),
        out_dir
    );

    for p in &raw_files {
        let stem = p.file_stem().unwrap().to_string_lossy();
        let bytes = std::fs::read(p).unwrap();
        if bytes.len() < WIDTH * HEIGHT * 8 {
            continue;
        }

        let mut rgba = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { rgba.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_p3(
                    src_row,
                    dst_row,
                    WIDTH,
                    overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL,
                );
            }
        }

        // BGRA to RGBA for PNG saving
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let out_file = out_dir.join(format!("{}.png", stem));
        image::save_buffer(
            &out_file,
            &rgba,
            WIDTH as u32,
            HEIGHT as u32,
            image::ExtendedColorType::Rgba8,
        )
        .expect("Failed to save png");
        println!("  Saved: {}", out_file.display());
    }
    println!("Done! All {} snapshots converted to PNG.", raw_files.len());
}

#[cfg(windows)]
#[test]
fn test_diagnose_score_rate_anomalies() {
    use overmax_core::SceneType;
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3;
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::roi::RoiManager;
    use overmax_engine::detector::templates;

    let target_files = [
        ("snapshot1", "hdr_snapshot1.raw"),
        ("snapshot2", "hdr_snapshot2.raw"),
        ("acidinvasion", "hdr_snapshot_fr_8b_sc_acidinvasion.raw"),
    ];

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    for (label, fname) in target_files {
        let Some(p) = find_raw_file(fname) else {
            println!("File {} not found", fname);
            continue;
        };
        let bytes = std::fs::read(p).unwrap();
        let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            unsafe {
                let dst_row = bgra.as_mut_ptr().add(y * WIDTH * 4);
                convert_scrgb_fp16_to_bgra8_p3(
                    src_row,
                    dst_row,
                    WIDTH,
                    overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL,
                );
            }
        }

        let frame = CapturedFrame {
            width: WIDTH as i32,
            height: HEIGHT as i32,
            bgra,
        };

        let mut rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
        rois.set_scene(SceneType::Freestyle);

        println!("\n=== DIAGNOSTIC FOR {} ({}) ===", label, fname);

        // 1. Rate Diagnostic
        if let Some(rate_roi) = rois.get_roi("rate") {
            if let Some(img) = rate_roi.crop(&frame) {
                let reg = img.to_image_region();
                let b_res = overmax_cv::binarize_by_global_contrast(
                    &reg.bgra,
                    img.width,
                    img.height,
                    overmax_cv::LumaMethod::Average,
                    255,
                );
                if let Ok((bin, thresh, max_y)) = b_res {
                    let segs = overmax_cv::segment_characters(&bin, img.width, img.height);
                    let det = templates::detect_rate(&img);
                    println!(
                        "  [Rate] detect_rate: {:?}, thresh: {}, max_y: {}",
                        det, thresh, max_y
                    );
                    if let Ok(s) = segs {
                        println!("  [Rate] segments count: {}, ranges: {:?}", s.len(), s);
                        for (i, &(x1, x2)) in s.iter().enumerate() {
                            let cw = x2 - x1;
                            let mut cbin = vec![0u8; cw * img.height];
                            for y in 0..img.height {
                                for x in 0..cw {
                                    cbin[y * cw + x] = bin[y * img.width + (x1 + x)];
                                }
                            }
                            let m = overmax_cv::match_character(
                                &cbin,
                                cw,
                                img.height,
                                templates::digit::DIGIT_TEMPLATES_RATE,
                            );
                            println!(
                                "    rate char #{}: [{}..{}] (w={}) -> {:?}",
                                i, x1, x2, cw, m
                            );
                        }
                    }
                }
            }
        }

        for mode_desc in [
            ("Linear 4.88", ToneMapMode::Linear(4.88)),
            ("Linear 4.00", ToneMapMode::Linear(4.00)),
            (
                "2-Stage (4.0, 3.6)",
                ToneMapMode::TwoStageRational {
                    scale_mid: 4.0,
                    v_knee: 3.6,
                },
            ),
            (
                "2-Stage (4.0, 2.5)",
                ToneMapMode::TwoStageRational {
                    scale_mid: 4.0,
                    v_knee: 2.5,
                },
            ),
            (
                "2-Stage (4.0, 2.0)",
                ToneMapMode::TwoStageRational {
                    scale_mid: 4.0,
                    v_knee: 2.0,
                },
            ),
        ] {
            let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];
            for y in 0..HEIGHT {
                let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
                let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
                unsafe {
                    convert_scrgb_fp16_to_bgra8_custom(src_row, dst_row, WIDTH, mode_desc.1);
                }
            }
            let frame = CapturedFrame {
                width: WIDTH as i32,
                height: HEIGHT as i32,
                bgra,
            };
            if let Some(score_roi) = rois.get_roi("score") {
                if let Some(img) = score_roi.crop(&frame) {
                    let reg = img.to_image_region();
                    let b_res = overmax_cv::binarize_by_global_contrast(
                        &reg.bgra,
                        img.width,
                        img.height,
                        overmax_cv::LumaMethod::Average,
                        255,
                    );
                    let det = templates::detect_score(&img);
                    if let Ok((bin, thresh, max_y)) = b_res {
                        let segs = overmax_cv::segment_characters(&bin, img.width, img.height);
                        let seg_cnt = segs.as_ref().map(|s| s.len()).unwrap_or(0);
                        let seg_chars: Vec<Option<char>> = segs
                            .ok()
                            .map(|s| {
                                s.iter()
                                    .map(|&(x1, x2)| {
                                        let cw = x2 - x1;
                                        let mut cbin = vec![0u8; cw * img.height];
                                        for y in 0..img.height {
                                            for x in 0..cw {
                                                cbin[y * cw + x] = bin[y * img.width + (x1 + x)];
                                            }
                                        }
                                        overmax_cv::image::match_character(
                                            &cbin,
                                            cw,
                                            img.height,
                                            templates::digit::DIGIT_TEMPLATES_SCORE,
                                        )
                                        .map(|m| m.0)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        println!(
                            "    [{}] Score={:?}, thresh={}, max_y={}, segs={}, chars={:?}",
                            mode_desc.0, det, thresh, max_y, seg_cnt, seg_chars
                        );
                    }
                }
            }
        }
    }
}

#[inline(always)]
fn tone_map_2stage_rational(v: f32, scale_mid: f32, v_knee: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let inv_scale = 1.0 / scale_mid;
    let l_knee = (v_knee * inv_scale).clamp(0.0, 1.0);
    if v <= v_knee {
        (v * inv_scale).clamp(0.0, 1.0)
    } else {
        let delta_v = v - v_knee;
        let m = inv_scale;
        let rem = 1.0 - l_knee;
        if rem <= 1e-6 {
            1.0
        } else {
            let k = rem / m;
            let shoulder = rem * (delta_v / (k + delta_v));
            (l_knee + shoulder).clamp(0.0, 1.0)
        }
    }
}

unsafe fn convert_scrgb_fp16_to_bgra8_custom(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
    mode: ToneMapMode,
) {
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::{
        f16_to_f32, strict_srgb_oetf,
    };

    let src = src_row as *const u16;

    for x in 0..pixel_count {
        let r_bits = std::ptr::read_unaligned(src.add(x * 4));
        let g_bits = std::ptr::read_unaligned(src.add(x * 4 + 1));
        let b_bits = std::ptr::read_unaligned(src.add(x * 4 + 2));

        let r = f16_to_f32(r_bits);
        let g = f16_to_f32(g_bits);
        let b = f16_to_f32(b_bits);

        // DCI-P3 역변환 행렬 곱
        let r_p3 = (0.822475 * r + 0.177378 * g).max(0.0);
        let g_p3 = (0.033155 * r + 0.966935 * g).max(0.0);
        let b_p3 = (0.017052 * r + 0.072371 * g + 0.910581 * b).max(0.0);

        let (r_lin, g_lin, b_lin) = match mode {
            ToneMapMode::Linear(scale) => {
                let inv = 1.0 / scale;
                (
                    (r_p3 * inv).clamp(0.0, 1.0),
                    (g_p3 * inv).clamp(0.0, 1.0),
                    (b_p3 * inv).clamp(0.0, 1.0),
                )
            }
            ToneMapMode::TwoStageRational { scale_mid, v_knee } => (
                tone_map_2stage_rational(r_p3, scale_mid, v_knee),
                tone_map_2stage_rational(g_p3, scale_mid, v_knee),
                tone_map_2stage_rational(b_p3, scale_mid, v_knee),
            ),
        };

        let r_srgb = strict_srgb_oetf(r_lin);
        let g_srgb = strict_srgb_oetf(g_lin);
        let b_srgb = strict_srgb_oetf(b_lin);

        let dst = dst_row.add(x * 4);
        *dst.add(0) = (b_srgb * 255.0 + 0.5) as u8;
        *dst.add(1) = (g_srgb * 255.0 + 0.5) as u8;
        *dst.add(2) = (r_srgb * 255.0 + 0.5) as u8;
        *dst.add(3) = 255;
    }
}

#[derive(Clone, Copy, Debug)]
enum ToneMapMode {
    Linear(f32),
    TwoStageRational { scale_mid: f32, v_knee: f32 },
}

#[cfg(windows)]
#[test]
fn test_compare_2stage_vs_linear_jackets() {
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::roi::RoiManager;
    use overmax_engine::detector::templates;

    let target_dir = match find_snapshot_dir() {
        Some(d) => d,
        None => return,
    };

    let entries = match std::fs::read_dir(&target_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut raw_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("raw"))
        .collect();
    raw_files.sort();

    if raw_files.is_empty() {
        return;
    }

    let db_path = Path::new("../../cache/image_index.db");
    let fallback_db_path = Path::new("cache/image_index.db");
    let actual_db = if db_path.exists() {
        Some(db_path)
    } else if fallback_db_path.exists() {
        Some(fallback_db_path)
    } else {
        None
    };

    let Some(actual_db) = actual_db else {
        println!("[HDR Replay Test] Skipping test_experiment_tonemapping_comparison: image_index.db not found");
        return;
    };

    let mut db = overmax_data::store::image_index::ImageIndexDb::new(
        actual_db, 0.50, // 0.50 이상 매칭 결과도 추적
    );
    if db.load().is_err() {
        println!(
            "[HDR Replay Test] Skipping test_experiment_tonemapping_comparison: failed to load DB"
        );
        return;
    }
    let matcher = db.matcher();

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    let test_modes = [
        ("Linear 4.88 (Current)", ToneMapMode::Linear(4.88)),
        ("Linear 4.00 (Mid-focus)", ToneMapMode::Linear(4.00)),
        (
            "2-Stage (mid=4.0, knee=3.6)",
            ToneMapMode::TwoStageRational {
                scale_mid: 4.0,
                v_knee: 3.6,
            },
        ),
        (
            "2-Stage (mid=4.0, knee=2.5)",
            ToneMapMode::TwoStageRational {
                scale_mid: 4.0,
                v_knee: 2.5,
            },
        ),
        (
            "2-Stage (mid=4.0, knee=2.2)",
            ToneMapMode::TwoStageRational {
                scale_mid: 4.0,
                v_knee: 2.2,
            },
        ),
        (
            "2-Stage (mid=4.0, knee=2.0)",
            ToneMapMode::TwoStageRational {
                scale_mid: 4.0,
                v_knee: 2.0,
            },
        ),
        (
            "2-Stage (mid=3.8, knee=2.0)",
            ToneMapMode::TwoStageRational {
                scale_mid: 3.8,
                v_knee: 2.0,
            },
        ),
    ];

    println!("\n=========================================================================================");
    println!(
        " [2-STAGE TONE MAPPING vs LINEAR COMPREHENSIVE BENCHMARK] Total Files: {}",
        raw_files.len()
    );
    println!(
        "========================================================================================="
    );

    struct ModeStat {
        name: &'static str,
        total_fs_sim: f32,
        fs_count: usize,
        fs_below_65: usize,
        scene_success: usize,
        btn_mode_success: usize,
        score_success: usize,
        rate_success: usize,
    }

    let mut stats: Vec<ModeStat> = test_modes
        .iter()
        .map(|(name, _)| ModeStat {
            name,
            total_fs_sim: 0.0,
            fs_count: 0,
            fs_below_65: 0,
            scene_success: 0,
            btn_mode_success: 0,
            score_success: 0,
            rate_success: 0,
        })
        .collect();

    for raw_path in &raw_files {
        let fname = raw_path.file_name().unwrap().to_string_lossy();
        let bytes = std::fs::read(raw_path).unwrap();
        if bytes.len() < WIDTH * HEIGHT * 8 {
            continue;
        }

        print!("{:<28} |", fname);

        for (m_idx, &(mode_name, mode)) in test_modes.iter().enumerate() {
            let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];
            for y in 0..HEIGHT {
                let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
                let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
                unsafe {
                    convert_scrgb_fp16_to_bgra8_custom(src_row, dst_row, WIDTH, mode);
                }
            }

            // Extract FS Jacket (340, 94, 60, 60)
            let mut fs_jacket = Vec::with_capacity(60 * 60 * 4);
            for row in 0..60 {
                let start = (94 + row) * WIDTH * 4 + 340 * 4;
                fs_jacket.extend_from_slice(&bgra[start..start + 60 * 4]);
            }
            let m_fs = matcher.match_jacket(&fs_jacket, 60, 60, 4);

            let frame = CapturedFrame {
                width: WIDTH as i32,
                height: HEIGHT as i32,
                bgra,
            };
            let rois = RoiManager::new(WIDTH as i32, HEIGHT as i32);
            let scene = overmax_engine::detector::detection_pipeline::detect_static_scene(
                &frame, &rois, &matcher,
            );
            if scene != overmax_core::SceneType::Unknown {
                stats[m_idx].scene_success += 1;
            }

            let mut rois_active = rois.clone();
            rois_active.set_scene(scene);
            let btn_mode =
                overmax_engine::detector::play_state::detect_button_mode(&frame, &rois_active);
            if btn_mode.is_some() {
                stats[m_idx].btn_mode_success += 1;
            }

            let score = rois_active.and_then_roi(&frame, "score", templates::detect_score);
            let rate = rois_active.and_then_roi(&frame, "rate", |img| templates::detect_rate(img));

            if let Some(ref m) = m_fs {
                stats[m_idx].total_fs_sim += m.similarity;
                stats[m_idx].fs_count += 1;
                if m.similarity < 0.65 {
                    stats[m_idx].fs_below_65 += 1;
                }
            }
            if score.is_some() {
                stats[m_idx].score_success += 1;
            }
            if rate.is_some() {
                stats[m_idx].rate_success += 1;
            }

            let sim_str = m_fs
                .map(|m| format!("{:.3}", m.similarity))
                .unwrap_or_else(|| "NONE".to_string());
            print!(
                " {}: sim={} Sc={} R={} |",
                &mode_name[..4],
                sim_str,
                if score.is_some() { "O" } else { "X" },
                if rate.is_some() { "O" } else { "X" }
            );
        }
        println!();
    }

    println!("\n=========================================================================================");
    println!(" [SUMMARY STATISTICS]");
    println!(
        " {:<30} | {:<9} | {:<11} | {:<10} | {:<9} | {:<9} | {:<9}",
        "Mode", "Avg Sim", "Sim < 0.65", "Scene Ok", "Btn Ok", "Score Ok", "Rate Ok"
    );
    println!(
        "-----------------------------------------------------------------------------------------"
    );
    for s in stats {
        let avg_sim = if s.fs_count > 0 {
            s.total_fs_sim / s.fs_count as f32
        } else {
            0.0
        };
        println!(
            " {:<30} | {:<9.4} | {:<11} | {:<10} | {:<9} | {:<9} | {:<9}",
            s.name,
            avg_sim,
            s.fs_below_65,
            s.scene_success,
            s.btn_mode_success,
            s.score_success,
            s.rate_success
        );
    }
    println!("=========================================================================================\n");
}

#[cfg(windows)]
#[test]
fn test_benchmark_2stage_latency() {
    let raw_file = match find_raw_file("hdr_snapshot_1788956698.raw") {
        Some(f) => f,
        None => return,
    };
    let bytes = std::fs::read(raw_file).unwrap();
    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;
    let mut bgra = vec![0u8; WIDTH * HEIGHT * 4];

    // Warm-up
    for _ in 0..10 {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_custom(
                    src_row,
                    dst_row,
                    WIDTH,
                    ToneMapMode::TwoStageRational {
                        scale_mid: 4.0,
                        v_knee: 2.2,
                    },
                );
            }
        }
    }

    let iterations = 200;

    // 1. Current Linear P3
    let start_p3 = std::time::Instant::now();
    for _ in 0..iterations {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3(
                    src_row,
                    dst_row,
                    WIDTH,
                    overmax_engine::capture::capture_engine::windows::hdr_pipeline::DEFAULT_SDR_WHITE_LEVEL,
                );
            }
        }
    }
    let p3_ms = start_p3.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    // 2. 64KB LUT
    let lut = overmax_engine::capture::capture_engine::windows::hdr_pipeline::build_lut_table(4.88);
    let start_lut = std::time::Instant::now();
    for _ in 0..iterations {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                overmax_engine::capture::capture_engine::windows::hdr_pipeline::convert_scrgb_fp16_to_bgra8_with_lut(
                    src_row, dst_row, WIDTH, &lut,
                );
            }
        }
    }
    let lut_ms = start_lut.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    // 3. 2-Stage Rational Tone Mapping (Naive)
    let start_2s = std::time::Instant::now();
    for _ in 0..iterations {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_custom(
                    src_row,
                    dst_row,
                    WIDTH,
                    ToneMapMode::TwoStageRational {
                        scale_mid: 4.0,
                        v_knee: 2.2,
                    },
                );
            }
        }
    }
    let s2_ms = start_2s.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    // 4. 2-Stage Rational + Fast OETF LUT (1KB)
    let oetf_table = {
        let mut t = [0u8; 1025];
        for (i, item) in t.iter_mut().enumerate() {
            let lin = i as f32 / 1024.0;
            let srgb =
                overmax_engine::capture::capture_engine::windows::hdr_pipeline::strict_srgb_oetf(
                    lin,
                );
            *item = (srgb * 255.0 + 0.5) as u8;
        }
        t
    };

    let start_fast = std::time::Instant::now();
    for _ in 0..iterations {
        for y in 0..HEIGHT {
            let src_row = unsafe { bytes.as_ptr().add(y * WIDTH * 8) };
            let dst_row = unsafe { bgra.as_mut_ptr().add(y * WIDTH * 4) };
            unsafe {
                convert_scrgb_fp16_to_bgra8_fast(src_row, dst_row, WIDTH, &oetf_table, 4.0, 2.2);
            }
        }
    }
    let fast_ms = start_fast.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    println!("\n[LATENCY BENCHMARK (512x512 Atlas)]");
    println!(
        "  Current Linear P3 (convert_scrgb_fp16_to_bgra8_p3): {:.4} ms",
        p3_ms
    );
    println!(
        "  64KB Fast LUT (convert_scrgb_fp16_to_bgra8_with_lut): {:.4} ms",
        lut_ms
    );
    println!("  2-Stage Rational Naive (powf in loop): {:.4} ms", s2_ms);
    println!(
        "  2-Stage Rational Fast (1KB OETF Table): {:.4} ms",
        fast_ms
    );
}

#[inline(always)]
unsafe fn convert_scrgb_fp16_to_bgra8_fast(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
    oetf_table: &[u8; 1025],
    scale_mid: f32,
    v_knee: f32,
) {
    use overmax_engine::capture::capture_engine::windows::hdr_pipeline::f16_to_f32;

    let src = src_row as *const u16;

    for x in 0..pixel_count {
        let r_bits = std::ptr::read_unaligned(src.add(x * 4));
        let g_bits = std::ptr::read_unaligned(src.add(x * 4 + 1));
        let b_bits = std::ptr::read_unaligned(src.add(x * 4 + 2));

        let r = f16_to_f32(r_bits);
        let g = f16_to_f32(g_bits);
        let b = f16_to_f32(b_bits);

        // DCI-P3 역변환 행렬 곱
        let r_p3 = (0.822475 * r + 0.177378 * g).max(0.0);
        let g_p3 = (0.033155 * r + 0.966935 * g).max(0.0);
        let b_p3 = (0.017052 * r + 0.072371 * g + 0.910581 * b).max(0.0);

        let r_lin = tone_map_2stage_rational(r_p3, scale_mid, v_knee);
        let g_lin = tone_map_2stage_rational(g_p3, scale_mid, v_knee);
        let b_lin = tone_map_2stage_rational(b_p3, scale_mid, v_knee);

        let r_idx = ((r_lin * 1024.0) as usize).min(1024);
        let g_idx = ((g_lin * 1024.0) as usize).min(1024);
        let b_idx = ((b_lin * 1024.0) as usize).min(1024);

        let dst = dst_row.add(x * 4);
        *dst.add(0) = oetf_table[b_idx];
        *dst.add(1) = oetf_table[g_idx];
        *dst.add(2) = oetf_table[r_idx];
        *dst.add(3) = 255;
    }
}

#[cfg(windows)]
#[test]
fn test_analyze_user_capture_9858() {
    use overmax_core::SceneType;
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::roi::RoiManager;
    use overmax_engine::detector::templates;

    let candidates = [
        "scratch/capture_174804.png",
        "../../scratch/capture_174804.png",
        "scratch/capture_175546.png",
        "../../scratch/capture_175546.png",
        "scratch/capture_9858.png",
        "../../scratch/capture_9858.png",
    ];
    let path = match candidates.iter().find(|p| std::path::Path::new(p).exists()) {
        Some(p) => *p,
        None => {
            println!("User capture not found at {:?}", candidates);
            return;
        }
    };

    let dynamic_img = image::open(path).expect("failed to open capture image");
    let rgba = dynamic_img.to_rgba8();
    let (w, h) = rgba.dimensions();
    println!("\n=== ANALYZING USER CAPTURE 9858: {}x{} ===", w, h);

    let mut bgra = vec![0u8; (w * h * 4) as usize];
    for (i, p) in rgba.pixels().enumerate() {
        bgra[i * 4] = p[2]; // B
        bgra[i * 4 + 1] = p[1]; // G
        bgra[i * 4 + 2] = p[0]; // R
        bgra[i * 4 + 3] = p[3]; // A
    }

    let frame = CapturedFrame {
        width: w as i32,
        height: h as i32,
        bgra,
    };

    let mut rois = RoiManager::new(w as i32, h as i32);
    rois.set_scene(SceneType::Freestyle);

    // 1. Rate Analysis (Native Resolution)
    if let Some(rate_roi) = rois.get_roi("rate") {
        if let Some(img) = rate_roi.crop(&frame) {
            let det = templates::detect_rate(&img);
            assert_eq!(det, Some(98.58), "Native Rate must be 98.58%");
        }
    }

    // 2. Score Analysis (Native Resolution)
    if let Some(score_roi) = rois.get_roi("score") {
        if let Some(img) = score_roi.crop(&frame) {
            let det = templates::detect_score(&img);
            assert_eq!(det, Some(985869), "Native Score must be 985869");
        }
    }

    // 3. 1080p Normalized Analysis (Simulation of D3d11Normalizer DXGI pipeline)
    let resized_1080p = image::imageops::resize(
        &dynamic_img,
        1920,
        1080,
        image::imageops::FilterType::Triangle,
    );
    let mut bgra_1080p = vec![0u8; 1920 * 1080 * 4];
    for (i, p) in resized_1080p.pixels().enumerate() {
        bgra_1080p[i * 4] = p[2]; // B
        bgra_1080p[i * 4 + 1] = p[1]; // G
        bgra_1080p[i * 4 + 2] = p[0]; // R
        bgra_1080p[i * 4 + 3] = p[3]; // A
    }
    let frame_1080p = CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: bgra_1080p,
    };
    let mut rois_1080p = RoiManager::new(1920, 1080);
    rois_1080p.set_scene(SceneType::Freestyle);

    if let Some(score_roi) = rois_1080p.get_roi("score") {
        if let Some(img) = score_roi.crop(&frame_1080p) {
            let det = templates::detect_score(&img);
            assert_eq!(det, Some(985869), "1080p Normalized Score must be 985869");
        }
    }

    // 4. Virtual Atlas (512x512) Pipeline Analysis (Exact match for enable_gpu_atlas: true)
    let atlas_frame = overmax_engine::detector::atlas_layout::build_virtual_atlas(&frame_1080p);
    let mut rois_atlas = RoiManager::new(512, 512);
    rois_atlas.set_scene(SceneType::Freestyle);

    if let Some(score_roi) = rois_atlas.get_roi("score") {
        if let Some(img) = score_roi.crop(&atlas_frame) {
            let det = templates::detect_score(&img);
            assert_eq!(
                det,
                Some(985869),
                "Virtual Atlas pipeline must correctly recognize 985869"
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn test_diagnose_user_openmatch() {
    use overmax_core::SceneType;
    use overmax_engine::capture::frame::CapturedFrame;
    use overmax_engine::detector::roi::RoiManager;
    use overmax_engine::detector::templates;

    let candidates = ["scratch/user_openmatch", "../../scratch/user_openmatch"];
    let Some(base_dir) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        println!(
            "[HDR Replay Test] Notice: 'scratch/user_openmatch' directory not found, skipping."
        );
        return;
    };

    for i in 1..=5 {
        let path = format!("{}/om_{}.png", base_dir, i);
        let Ok(dynamic_img) = image::open(&path) else {
            continue;
        };
        let rgba = dynamic_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        println!("\n=======================================================");
        println!(">>> DIAGNOSING OM_{}.png ({}x{}) <<<", i, w, h);
        println!("=======================================================");

        // 1080p Normalization (simulate D3d11Normalizer)
        let resized_1080p = image::imageops::resize(
            &dynamic_img,
            1920,
            1080,
            image::imageops::FilterType::Triangle,
        );
        let mut bgra_1080p = vec![0u8; 1920 * 1080 * 4];
        for (idx, p) in resized_1080p.pixels().enumerate() {
            bgra_1080p[idx * 4] = p[2];
            bgra_1080p[idx * 4 + 1] = p[1];
            bgra_1080p[idx * 4 + 2] = p[0];
            bgra_1080p[idx * 4 + 3] = p[3];
        }
        let frame_1080p = CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: bgra_1080p,
        };

        // Virtual Atlas (512x512)
        let atlas_frame = overmax_engine::detector::atlas_layout::build_virtual_atlas(&frame_1080p);
        let mut rois_atlas = RoiManager::new(512, 512);
        rois_atlas.set_scene(SceneType::OpenMatch);

        // 1. Score
        let det_score = rois_atlas.and_then_roi(&atlas_frame, "score", templates::detect_score);
        println!("  [SCORE (Atlas)] => {:?}", det_score);

        // 2. Rate
        let det_rate =
            rois_atlas.and_then_roi(&atlas_frame, "rate", |img| templates::detect_rate(img));
        println!("  [RATE (Atlas)] => {:?}", det_rate);

        // 3. Max Combo Badge
        let is_mc =
            overmax_engine::detector::play_state::detect_max_combo(&atlas_frame, &rois_atlas);
        println!("  [BADGE (Atlas)] => is_mc={}", is_mc);

        match i {
            1 => {
                assert_eq!(det_score, Some(1000000));
                assert_eq!(det_rate, Some(100.0));
                assert!(is_mc);
            }
            2 => {
                assert_eq!(det_score, Some(999421));
                assert_eq!(det_rate, Some(99.94));
                assert!(is_mc);
            }
            3 => {
                assert_eq!(det_score, Some(1000000));
                assert_eq!(det_rate, Some(100.0));
                assert!(is_mc);
            }
            4 => {
                assert_eq!(det_score, Some(999558));
                assert_eq!(det_rate, Some(99.95));
                assert!(is_mc);
            }
            5 => {
                assert_eq!(det_score, Some(993462));
                assert_eq!(det_rate, Some(99.34));
                assert!(is_mc);
            }
            _ => {}
        }
    }
}
