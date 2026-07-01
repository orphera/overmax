use std::fs;
use std::path::{Path, PathBuf};
use image::GenericImageView;

use overmax_app::screen_capture::CapturedFrame;
use overmax_app::roi::RoiManager;
use overmax_app::ocr_engine::OcrDetector;
use overmax_app::frame_utils::crop_roi;
use overmax_core::SceneType;

fn load_frame(path: &Path) -> Option<CapturedFrame> {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("      [Error] Failed to open image '{}': {}", path.display(), e);
            return None;
        }
    };
    let (w, h) = img.dimensions();
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

fn detect_scene_from_logo(frame: &CapturedFrame, ocr: &OcrDetector, rois: &RoiManager) -> SceneType {
    let logo_roi = match rois.get_roi("logo") {
        Some(roi) => roi,
        None => return SceneType::Unknown,
    };
    let logo_img = match crop_roi(frame, logo_roi) {
        Some(img) => img,
        None => return SceneType::Unknown,
    };
    let (scene, raw_text, _) = ocr.detect_logo(&logo_img);
    println!("      [Logo OCR] raw: '{}', scene: {:?}", raw_text.trim(), scene);
    scene
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
    let mut paths: Vec<PathBuf> = Vec::new();
    
    // 1. converted 스크린샷 폴더 수집
    let screenshots_dir = Path::new("scratch/screenshots/converted");
    if screenshots_dir.exists() {
        if let Ok(entries) = fs::read_dir(screenshots_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("png") {
                    let fname = path.file_name().unwrap().to_string_lossy().to_string();
                    if !fname.contains("_mcbadge_") {
                        paths.push(path);
                    }
                }
            }
        }
    }
    
    // 2. scratch/hd_* 파일 수집
    let scratch_dir = Path::new("scratch");
    if scratch_dir.exists() {
        if let Ok(entries) = fs::read_dir(scratch_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let fname = path.file_name().unwrap().to_string_lossy().to_lowercase();
                if fname.starts_with("hd_") && (fname.ends_with(".png") || fname.ends_with(".jpg")) {
                    paths.push(path);
                }
            }
        }
    }
    
    paths.sort_by_key(|p| p.file_name().unwrap().to_os_string());
    println!("Found {} total files to process and crop.", paths.len());
    
    println!("--- Initializing OCR Detector ---");
    let ocr = OcrDetector::new();
    
    for path in paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        println!("\n==================================================");
        println!("Processing: {}", filename);
        
        let Some(frame) = load_frame(&path) else {
            println!("  - Failed to load image. Skipping.");
            continue;
        };
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
        run_roi_test(&frame, &ocr, &rois, scene);
    }
}

fn run_roi_test(frame: &CapturedFrame, ocr: &OcrDetector, rois: &RoiManager, scene: SceneType) {
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
        let is_result = matches!(
            scene,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );
        let is_song_select = matches!(scene, SceneType::Freestyle | SceneType::OpenMatch);
        
        if is_result || is_song_select {
            if let Some(s_val) = score_val {
                let calc_rate = s_val as f32 / 10000.0;
                println!("    Calculated Rate from Score: {:.4}%", calc_rate);
                
                let is_valid_range = if is_song_select {
                    (overmax_app::play_state::MIN_VALID_RATE..=100.0).contains(&calc_rate)
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
