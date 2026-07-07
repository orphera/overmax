use std::fs;
use std::path::{Path, PathBuf};
use image::GenericImageView;

use overmax_engine::capture::screen_capture::CapturedFrame;
use overmax_engine::detector::roi::RoiManager;
use overmax_engine::detector::ocr_engine::OcrDetector;
use overmax_engine::capture::frame_utils::crop_roi;
use overmax_engine::detector::detection_pipeline::detect_scene_from_logo;
use overmax_core::SceneType;
use overmax_data::ImageIndexDb;

fn load_frame(path: &Path) -> Option<CapturedFrame> {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("      [Error] Failed to open image '{}': {}", path.display(), e);
            return None;
        }
    };
    let (w, h) = img.dimensions();
    println!("      [Image Resolution] Original size: {}x{}", w, h);
    // 1920x1080 리사이즈 (hd_* 파일들의 해상도가 FHD가 아닐 경우 대비)
    let img_resized = if w != 1920 || h != 1080 {
        img.resize_exact(1920, 1080, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    
    let mut rgba = img_resized.to_rgba8().into_raw();
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    Some(CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: rgba,
    })
}



fn save_badge_crop(frame: &CapturedFrame, rois: &RoiManager, scene: SceneType, original_filename: &str) {
    let badge_roi = match rois.get_roi_for_scene("max_combo_badge", scene) {
        Some(roi) => roi,
        None => return,
    };
    let Some(badge_img) = crop_roi(frame, badge_roi) else {
        return;
    };
    let scene_str = match scene {
        SceneType::Freestyle => "freestyle",
        SceneType::OpenMatch => "openmatch",
        SceneType::ResultFreestyle => "result_freestyle",
        SceneType::ResultOpen3 => "result_open3",
        SceneType::ResultOpen2 => "result_open2",
        _ => "unknown",
    };
    
    // 대상 폴더 존재 확인
    let dst_dir = Path::new("scratch/screenshots");
    if !dst_dir.exists() {
        fs::create_dir_all(dst_dir).unwrap();
    }
    
    let dst_name = format!("{}_mcbadge_{}", scene_str, original_filename);
    // 확장자가 png가 아니면 png로 강제 변경
    let dst_path = dst_dir.join(dst_name).with_extension("png");
    
    let mut bgra = badge_img.bgra.clone();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    let rgba = bgra;
    
    let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
        badge_img.width as u32,
        badge_img.height as u32,
        rgba
    ).expect("failed to create image buffer");
    let dynamic_img = image::DynamicImage::ImageRgba8(buf);
    dynamic_img.save(&dst_path).expect("failed to save badge crop image");
    println!("      Saved badge crop to: {}", dst_path.display());
    if let Ok((phash, dhash, ahash)) = overmax_cv::compute_image_hashes(
        &badge_img.bgra,
        badge_img.width as usize,
        badge_img.height as usize,
        4
    ) {
        println!("      [Badge Hash] phash={:016x}, dhash={:016x}, ahash={:016x}", phash, dhash, ahash);
    }
}

fn main() {
    let diff_dir = Path::new("scratch/screenshots/diff_rois");
    fs::create_dir_all(diff_dir).ok();

    let mut paths: Vec<PathBuf> = Vec::new();
    
    // 0. openmatch3_results 폴더 수집
    let open3_results_dir = Path::new("scratch/openmatch3_results");
    if open3_results_dir.exists() {
        if let Ok(entries) = fs::read_dir(open3_results_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    paths.push(path);
                }
            }
        }
    }
    
    // 1. screenshots 폴더 수집
    let screenshots_dir = Path::new("scratch/screenshots");
    if screenshots_dir.exists() {
        if let Ok(entries) = fs::read_dir(screenshots_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    let fname = path.file_name().unwrap().to_string_lossy().to_lowercase();
                    if !fname.contains("_mcbadge_") 
                        && !fname.contains("cropped_") 
                        && !fname.contains("debug_")
                        && !fname.contains("result_")
                    {
                        paths.push(path);
                    }
                }
            }
        }
    }
    
    // 2. scratch 파일 수집
    let scratch_dir = Path::new("scratch");
    if scratch_dir.exists() {
        if let Ok(entries) = fs::read_dir(scratch_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    let fname = path.file_name().unwrap().to_string_lossy().to_lowercase();
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    if (ext == "png" || ext == "jpg" || ext == "jpeg") 
                        && !fname.contains("_mcbadge_") 
                        && !fname.contains("cropped_")
                        && !fname.contains("debug_") 
                        && !fname.contains("result_") 
                    {
                        paths.push(path);
                    }
                }
            }
        }
    }
    
    paths.sort_by_key(|p| p.file_name().unwrap().to_os_string());
    println!("Found {} total files to process and crop.", paths.len());
    
    println!("--- Initializing OCR Detector ---");
    let ocr = OcrDetector::new();
    
    let db_path = "cache/image_index.db";
    let mut image_db = ImageIndexDb::new(db_path, 0.6);
    let db_loaded = image_db.load().is_ok();
    println!("--- Image DB Loaded: {} ---", db_loaded);
    let matcher = image_db.matcher();
    
    for path in paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        println!("\n==================================================");
        println!("Processing: {}", filename);
        
        let Some(frame) = load_frame(&path) else {
            println!("  - Failed to load image. Skipping.");
            continue;
        };
        if frame.width < 1920 {
            println!("  - Resolution width {} < 1920 (likely a crop/debug image). Skipping.", frame.width);
            continue;
        }
        let mut rois = RoiManager::new(frame.width, frame.height);
        
        // 1. Logo 분석을 통해 씬 판별
        let mut scene = detect_scene_from_logo(&frame, &ocr, &rois);
        println!("  - Detected Scene: {:?}", scene);
        
        if scene == SceneType::Unknown {
            let fname = filename.to_lowercase();
            if fname.contains("freestyle") {
                scene = SceneType::Freestyle;
                println!("  - (Fallback) Using Freestyle from filename");
            } else if fname.contains("open") || fname.contains("match") {
                scene = SceneType::OpenMatch;
                println!("  - (Fallback) Using OpenMatch from filename");
            } else if fname.contains("hd_test_2p") {
                scene = SceneType::ResultOpen2;
                println!("  - (Fallback) Using ResultOpen2 from filename");
            } else if fname.contains("hd_test_1") || fname.contains("hd_test_3") {
                scene = SceneType::ResultOpen3;
                println!("  - (Fallback) Using ResultOpen3 from filename");
            } else if fname.contains("hd_test") {
                scene = SceneType::ResultFreestyle;
                println!("  - (Fallback) Using ResultFreestyle from filename");
            } else {
                println!("  - Scene Unknown. Generating badge crops for all candidate scenes:");
                let candidates = [
                    SceneType::Freestyle,
                    SceneType::OpenMatch,
                    SceneType::ResultFreestyle,
                    SceneType::ResultOpen2,
                    SceneType::ResultOpen3,
                ];
                for &cand in &candidates {
                    save_badge_crop(&frame, &rois, cand, &filename);
                }
                continue;
            }
        }
        
        rois.set_scene(scene);
        
        // 뱃지 영역 저장
        save_badge_crop(&frame, &rois, scene, &filename);
        
        // Rate/Score 출력 테스트도 함께 수행
        run_roi_test(&frame, &ocr, &rois, scene, &matcher, &filename);
    }
}

fn run_roi_test(
    frame: &CapturedFrame,
    ocr: &OcrDetector,
    rois: &RoiManager,
    scene: SceneType,
    matcher: &overmax_data::JacketMatcher,
    filename: &str,
) {
    // 1. Jacket Matching (song_id) 테스트
    if let Some(jacket_roi) = rois.get_roi("jacket") {
        if let Some(jacket_img) = crop_roi(frame, jacket_roi) {
            // 디버그 이미지 저장
            let mut bgra = jacket_img.bgra.clone();
            for chunk in bgra.chunks_exact_mut(4) { chunk.swap(0, 2); }
            if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(jacket_img.width as u32, jacket_img.height as u32, bgra) {
                let dynamic_img = image::DynamicImage::ImageRgba8(buf);
                dynamic_img.save(format!("scratch/screenshots/debug_jacket_{}", filename)).ok();
            }

            if let Some(match_res) = matcher.match_jacket(
                &jacket_img.bgra,
                jacket_img.width as usize,
                jacket_img.height as usize,
                4,
            ) {
                println!(
                    "    Jacket Matching Result: song_id={:?}, similarity={:.4}",
                    match_res.image_id.parse::<u32>().ok(),
                    match_res.similarity
                );
            } else {
                println!("    Jacket Matching Result: No Match");
            }
        } else {
            println!("    Jacket Matching Result: Crop Failed");
        }
    } else {
        println!("    Jacket Matching Result: ROI 'jacket' Missing");
    }

    // 2. Max Combo 테스트
    let is_result = matches!(
        scene,
        SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
    );
    let is_mc = if is_result {
        overmax_engine::detector::play_state::detect_max_combo_result(frame, rois)
    } else {
        overmax_engine::detector::play_state::detect_max_combo(frame, rois)
    };
    println!("    Max Combo Badge Detected: {}", is_mc);

    // 3. Mode & Difficulty 테스트

    let mut detected_mode = None;
    let mut detected_diff = None;

    if is_result {
        match scene {
            SceneType::ResultFreestyle => {
                if let Some(mode_roi) = rois.get_roi("mode_digit") {
                    if let Some(mode_img) = crop_roi(frame, mode_roi) {
                        // 디버그 이미지 저장
                        let mut bgra = mode_img.bgra.clone();
                        for chunk in bgra.chunks_exact_mut(4) { chunk.swap(0, 2); }
                        if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(mode_img.width as u32, mode_img.height as u32, bgra) {
                            let dynamic_img = image::DynamicImage::ImageRgba8(buf);
                            dynamic_img.save(format!("scratch/screenshots/debug_mode_{}", filename)).ok();
                        }

                        detected_mode = ocr.detect_freestyle_mode(&mode_img);
                        println!("    Mode Match: Resolved: {:?}", detected_mode);
                    }
                }
                if let Some(diff_roi) = rois.get_roi("diff_panel") {
                    if let Some(diff_img) = crop_roi(frame, diff_roi) {
                        // 디버그 이미지 저장
                        let mut bgra = diff_img.bgra.clone();
                        for chunk in bgra.chunks_exact_mut(4) { chunk.swap(0, 2); }
                        if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(diff_img.width as u32, diff_img.height as u32, bgra) {
                            let dynamic_img = image::DynamicImage::ImageRgba8(buf);
                            dynamic_img.save(format!("scratch/screenshots/diff_rois/result_freestyle_diff_{}", filename)).ok();
                        }

                        detected_diff = ocr.detect_result_difficulty(&diff_img);
                        println!("    Difficulty BGR/Pattern Match: Resolved: {:?}", detected_diff);
                    }
                }
            }
            SceneType::ResultOpen3 | SceneType::ResultOpen2 => {
                detected_mode = overmax_engine::detector::play_state::detect_button_mode_from_roi(frame, rois, "openmatch_mode");
                if let Some(diff_roi) = rois.get_roi("openmatch_diff") {
                    if let Some(diff_img) = crop_roi(frame, diff_roi) {
                        detected_diff = ocr.detect_openmatch_result_difficulty(&diff_img);
                    }
                }
                if (detected_mode.is_none() || detected_diff.is_none()) && scene == SceneType::ResultOpen2 {
                    if let Some(logo_roi) = rois.get_roi("logo") {
                        if let Some(logo_img) = crop_roi(frame, logo_roi) {
                            if let Some(txt) = ocr.recognize_text_all_passes(&logo_img) {
                                if detected_mode.is_none() {
                                    detected_mode = ocr.parse_mode_from_text(&txt);
                                }
                                if detected_diff.is_none() {
                                    let norm = txt.to_lowercase();
                                    if norm.contains("sc") { detected_diff = Some("SC".to_string()); }
                                    else if norm.contains("mx") || norm.contains("maximum") || norm.contains("max") { detected_diff = Some("MX".to_string()); }
                                    else if norm.contains("hd") || norm.contains("hard") { detected_diff = Some("HD".to_string()); }
                                    else if norm.contains("nm") || norm.contains("normal") { detected_diff = Some("NM".to_string()); }
                                }
                            }
                        }
                    }
                }
                println!("    Badge Match: Resolved Mode: {:?}, Resolved Diff: {:?}", detected_mode, detected_diff);
            }
            _ => {}
        }
    } else {
        // Song select (Freestyle / OpenMatch)
        detected_mode = overmax_engine::detector::play_state::detect_button_mode(frame, rois);
        let (d, conf) = overmax_engine::detector::play_state::detect_difficulty(frame, rois);
        detected_diff = d;
        println!("    Detected Mode from color: {:?}, Diff from brightness: {:?} (confident: {})", detected_mode, detected_diff, conf);

        if let Some(diff_roi) = rois.get_roi("diff_panel") {
            if let Some(diff_img) = crop_roi(frame, diff_roi) {
                let mut bgra = diff_img.bgra.clone();
                for chunk in bgra.chunks_exact_mut(4) { chunk.swap(0, 2); }
                if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(diff_img.width as u32, diff_img.height as u32, bgra) {
                    let dynamic_img = image::DynamicImage::ImageRgba8(buf);
                    dynamic_img.save(format!("scratch/screenshots/diff_rois/select_diff_{}", filename)).ok();
                }
            }
        }
    }

    // Rate ROI 테스트
    let mut rate_val: Option<f32> = None;
    if let Some(rate_roi) = rois.get_roi("rate") {
        if let Some(rate_img) = crop_roi(frame, rate_roi) {
            let res = ocr.detect_rate(&rate_img);
            rate_val = res.0;
            println!("    Rate ROI OCR Result: {:?}", res.0);
        }
    }
    
    // Score ROI 테스트
    let mut score_val: Option<u32> = None;
    if let Some(score_roi) = rois.get_roi("score") {
        if let Some(score_img) = crop_roi(frame, score_roi) {
            let res = ocr.detect_score(&score_img);
            score_val = res;
            println!("    Score ROI OCR Result: {:?}", res);
        }
    }
    
    // 크로스 검증 로직 모사 및 보강 테스트
    if score_val.is_some() || rate_val.is_some() {
        let is_song_select = matches!(scene, SceneType::Freestyle | SceneType::OpenMatch);
        
        if is_result || is_song_select {
            if let Some(s_val) = score_val {
                let calc_rate = s_val as f32 / 10000.0;
                println!("    Calculated Rate from Score: {:.4}%", calc_rate);
                
                let is_valid_range = if is_song_select {
                    (overmax_engine::detector::play_state::MIN_VALID_RATE..=100.0).contains(&calc_rate)
                } else {
                    (0.0..=100.0).contains(&calc_rate)
                };

                if is_valid_range {
                    match rate_val {
                        Some(r) => {
                            if let Some(final_rate) = resolve_most_plausible_rate(r, calc_rate, is_song_select) {
                                println!("    [Validation] Resolved Rate: {}%", final_rate);
                            } else {
                                println!("    [Validation] Resolution failed, keeping original rate: {}%", r);
                            }
                        }
                        None => {
                            let corrected = (calc_rate * 100.0).floor() / 100.0;
                            println!("    [Validation] Rate OCR failed. Filling with score rate: {}%", corrected);
                        }
                    }
                } else {
                    println!("    [Validation] Calculated rate {:.4}% is out of valid range, ignoring.", calc_rate);
                }
            } else {
                println!("    [Validation] Score OCR failed, keeping original rate: {:?}", rate_val);
            }
        }
    }
}

fn resolve_most_plausible_rate(rate_ocr: f32, score_rate: f32, is_song_select: bool) -> Option<f32> {
    if (rate_ocr - score_rate).abs() < 0.1 {
        return Some((score_rate * 100.0).floor() / 100.0);
    }

    let score_plaus = get_rate_plausibility(score_rate);
    let ocr_plaus = get_rate_plausibility(rate_ocr);

    if score_plaus != ocr_plaus {
        if score_plaus > ocr_plaus {
            println!("    [Plausibility] Trusting Score Rate ({:.2}%) over Rate OCR ({:.2}%)", score_rate, rate_ocr);
            return Some((score_rate * 100.0).floor() / 100.0);
        } else {
            println!("    [Plausibility] Trusting Rate OCR ({:.2}%) over Score Rate ({:.2}%)", rate_ocr, score_rate);
            return Some(rate_ocr);
        }
    }

    if is_song_select {
        println!("    [Plausibility] Tie in song select. Keeping Rate OCR: {:.2}%", rate_ocr);
        Some(rate_ocr)
    } else {
        Some((score_rate * 100.0).floor() / 100.0)
    }
}

fn get_rate_plausibility(rate: f32) -> i32 {
    if (90.0..=100.0).contains(&rate) {
        3
    } else if (70.0..=90.0).contains(&rate) {
        2
    } else if (50.0..=70.0).contains(&rate) {
        1
    } else {
        0
    }
}
