use std::fs;
use std::path::Path;
use image::GenericImageView;
use overmax_app::capture::screen_capture::CapturedFrame;
use overmax_app::detector::roi::RoiManager;
use overmax_app::detector::ocr_engine::OcrDetector;
use overmax_app::capture::frame_utils::crop_roi;
use overmax_core::SceneType;

// 자동 생성된 템플릿 배열 상수 바인딩
use overmax_app::detector::templates::digit::DIGIT_TEMPLATES;

fn load_frame(path: &Path) -> Option<CapturedFrame> {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(_) => return None,
    };
    let (w, h) = img.dimensions();
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

// 고휘도 임계값 필터링
fn threshold_luminance(bgra: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut binary = vec![0u8; width * height];
    let mut max_y = 0u8;
    let mut y_vals = vec![0u8; width * height];
    
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let b = bgra[idx];
            let g = bgra[idx + 1];
            let r = bgra[idx + 2];
            let y_val = ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8;
            y_vals[y * width + x] = y_val;
            if y_val > max_y {
                max_y = y_val;
            }
        }
    }
    
    let threshold = if max_y > 80 {
        ((max_y as f32 * 0.80) as u8).max(max_y.saturating_sub(38))
    } else {
        180
    };
    
    for idx in 0..(width * height) {
        binary[idx] = if y_vals[idx] >= threshold { 255 } else { 0 };
    }
    binary
}

fn crop_binary_character(
    binary: &[u8],
    full_width: usize,
    full_height: usize,
    x1: usize,
    x2: usize,
) -> Vec<u8> {
    let width = x2 - x1;
    let height = full_height;
    let mut char_bin = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            char_bin[y * width + x] = binary[y * full_width + (x1 + x)];
        }
    }
    char_bin
}

fn main() {
    let screenshots_dir = Path::new("scratch/screenshots");
    let mut paths = Vec::new();
    if screenshots_dir.exists() {
        if let Ok(entries) = fs::read_dir(screenshots_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let fname = path.file_name().unwrap().to_string_lossy().to_string();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if (ext == "png" || ext == "jpg" || ext == "jpeg") 
                    && !fname.contains("_mcbadge_") 
                    && !fname.starts_with("cropped_")
                    && !fname.starts_with("debug_") 
                {
                    paths.push(path);
                }
            }
        }
    }
    
    paths.sort();
    println!("Evaluating template matching on {} screenshots...", paths.len());
    
    // cv_templates 매핑 구성
    let cv_templates: Vec<overmax_cv::CvTemplate> = DIGIT_TEMPLATES.iter().map(|t| {
        overmax_cv::CvTemplate {
            char_val: t.char_val,
            width: t.width,
            height: t.height,
            mask: t.mask,
        }
    }).collect();
    
    let ocr = OcrDetector::new();
    let mut rois = RoiManager::new(1920, 1080);
    
    let mut total_evaluated = 0;
    let mut total_correct = 0;
    
    for path in paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(frame) = load_frame(&path) else { continue; };
        
        // 씬 판별
        let logo_roi = match rois.get_roi("logo") {
            Some(roi) => roi,
            None => continue,
        };
        let logo_img = match crop_roi(&frame, logo_roi) {
            Some(img) => img,
            None => continue,
        };
        let (mut scene, _, _) = ocr.detect_logo(&logo_img);
        
        // 씬 Unknown 이면 파일명으로 유추
        if scene == SceneType::Unknown {
            let fname = filename.to_lowercase();
            if fname.contains("freestyle") {
                scene = SceneType::Freestyle;
            } else if fname.contains("open") || fname.contains("match") {
                scene = SceneType::OpenMatch;
            } else if fname.contains("hd_test_2p") {
                scene = SceneType::ResultOpen2;
            } else if fname.contains("hd_test_1") || fname.contains("hd_test_3") {
                scene = SceneType::ResultOpen3;
            } else if fname.contains("hd_test") {
                scene = SceneType::ResultFreestyle;
            }
        }
        
        if scene == SceneType::Unknown {
            continue;
        }
        
        rois.set_scene(scene);
        
        let Some(rate_roi) = rois.get_roi("rate") else { continue; };
        let Some(rate_img) = crop_roi(&frame, rate_roi) else { continue; };
        
        // 1. Windows OCR로 예상 텍스트 추출 (Ground Truth)
        let (rate_val, raw_txt, _) = ocr.detect_rate(&rate_img);
        let Some(val) = rate_val else {
            continue;
        };
        
        // OCR 텍스트 정규화
        let expected_str: String = raw_txt
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '%')
            .collect();
            
        if expected_str.is_empty() {
            continue;
        }
        
        // 2. 픽셀 전처리 및 수직 분할
        let binary = threshold_luminance(&rate_img.bgra, rate_img.width as usize, rate_img.height as usize);
        let segments = overmax_cv::segment_characters(&binary, rate_img.width as usize, rate_img.height as usize).unwrap();
        
        // 3. 템플릿 매칭 수행
        let mut matched_str = String::new();
        for &(x1, x2) in &segments {
            let char_bin = crop_binary_character(&binary, rate_img.width as usize, rate_img.height as usize, x1, x2);
            let char_w = x2 - x1;
            let char_h = rate_img.height as usize;
            
            if let Ok(Some((ch, _score))) = overmax_cv::match_character(&char_bin, char_w, char_h, &cv_templates) {
                matched_str.push(ch);
            } else {
                matched_str.push('?');
            }
        }
        
        total_evaluated += 1;
        
        let mut is_match = matched_str == expected_str;
        // Ground Truth 보정 가드: Windows OCR이 마지막 '%'를 '9', '0', '8' 등으로 오독하는 고질적 현상 구제
        if !is_match && (expected_str.ends_with('9') || expected_str.ends_with('0') || expected_str.ends_with('8')) && matched_str.ends_with('%') {
            let mut corrected_expected = expected_str.clone();
            corrected_expected.pop();
            corrected_expected.push('%');
            if matched_str == corrected_expected {
                is_match = true;
            }
        }

        if is_match {
            total_correct += 1;
            println!("  [OK] {} -> Match SUCCESS. Matched: '{}', Expected: '{}'", filename, matched_str, expected_str);
        } else {
            println!("  [FAIL] {} -> Match FAILED. Matched: '{}', Expected: '{}' (OCR value: {:.2}%)", 
                     filename, matched_str, expected_str, val);
        }
    }
    
    let accuracy = if total_evaluated > 0 {
        (total_correct as f32 / total_evaluated as f32) * 100.0
    } else {
        0.0
    };
    
    println!("\n==================================================");
    println!("Evaluation Summary:");
    println!("Total Evaluated Rates: {}", total_evaluated);
    println!("Successfully Matched:  {}", total_correct);
    println!("Template Matching Accuracy: {:.2}%", accuracy);
    println!("==================================================");
}
