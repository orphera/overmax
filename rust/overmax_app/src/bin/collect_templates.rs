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

fn crop_roi_direct(frame: &CapturedFrame, x: usize, y: usize, width: usize, height: usize) -> overmax_app::frame_utils::ImageRegion {
    let mut bgra = vec![0u8; width * height * 4];
    for dy in 0..height {
        for dx in 0..width {
            let src_x = x + dx;
            let src_y = y + dy;
            let src_idx = (src_y * frame.width as usize + src_x) * 4;
            let dst_idx = (dy * width + dx) * 4;
            bgra[dst_idx..dst_idx + 4].copy_from_slice(&frame.bgra[src_idx..src_idx + 4]);
        }
    }
    overmax_app::frame_utils::ImageRegion {
        bgra,
        width: width as i32,
        height: height as i32,
    }
}

// 고휘도 임계값 필터링 (휘도 Y >= threshold 이면 255, 아니면 0)
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
            // BT.601 휘도 가중치 변환
            let y_val = ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8;
            y_vals[y * width + x] = y_val;
            if y_val > max_y {
                max_y = y_val;
            }
        }
    }
    
    // 동적 임계값: 최대 휘도의 80% 또는 최대 휘도 - 38 중 큰 값 (글자 획 두께 보존 및 배경 배제 밸런스)
    let threshold = if max_y > 80 {
        ((max_y as f32 * 0.80) as u8).max(max_y.saturating_sub(38))
    } else {
        180
    };
    
    println!("      [Luminance Debug] max_y={}, calculated threshold={}", max_y, threshold);

    for idx in 0..(width * height) {
        binary[idx] = if y_vals[idx] >= threshold { 255 } else { 0 };
    }
    binary
}

// 수직 프로젝션을 이용한 폰트 영역 슬라이싱 (세그멘테이션)
// 각 문자의 시작 x 좌표와 끝 x 좌표 목록을 반환
fn segment_characters(binary: &[u8], width: usize, height: usize) -> Vec<(usize, usize)> {
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
    
    // 켜진 픽셀 임계값 (노이즈 방지를 위해 1열당 최소 1픽셀 초과하여 켜져 있어야 문자로 인정)
    let col_threshold = 1;

    for x in 0..width {
        let active = col_proj[x] >= col_threshold;
        if active && !in_char {
            start_x = x;
            in_char = true;
        } else if !active && in_char {
            // 문자가 끝남 (공백 구간 진입)
            let end_x = x;
            // 너무 좁은 세그먼트는 노이즈로 간주하고 배제 (최소 너비 2픽셀)
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

fn save_segment_as_png(
    binary: &[u8],
    full_width: usize,
    full_height: usize,
    x1: usize,
    x2: usize,
    out_path: &Path,
) -> Result<(), String> {
    let width = x2 - x1;
    let height = full_height;
    let mut bgra = vec![0u8; width * height * 4];
    
    for y in 0..height {
        for x in 0..width {
            let src_x = x1 + x;
            let val = binary[y * full_width + src_x];
            let idx = (y * width + x) * 4;
            bgra[idx] = val;     // B
            bgra[idx + 1] = val; // G
            bgra[idx + 2] = val; // R
            bgra[idx + 3] = 255; // A
        }
    }
    
    let mut rgba = bgra;
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2); // BGR -> RGB
    }
    
    let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width as u32, height as u32, rgba)
        .ok_or_else(|| "failed to create image buffer".to_string())?;
    let dynamic_img = image::DynamicImage::ImageRgba8(buf);
    dynamic_img.save(out_path).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    let output_dir = Path::new("scratch/screenshots/digits");
    fs::create_dir_all(output_dir).ok();
    
    // 테스트용 1080p 스크린샷 탐색
    let mut paths = Vec::new();
    let screenshots_dir = Path::new("scratch/screenshots");
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
    println!("Found {} candidate screenshots for template collection.", paths.len());
    
    let ocr = OcrDetector::new();
    let mut rois = RoiManager::new(1920, 1080);
    
    let mut total_saved = 0;
    
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
        let (mut scene, logo_raw, _) = ocr.detect_logo(&logo_img);
        
        // 씬 Unknown 이면 텍스트 키워드 및 파일명으로 유추
        if scene == SceneType::Unknown {
            let logo_norm = logo_raw.to_lowercase();
            if logo_norm.contains("button") || logo_norm.contains("tunes") {
                scene = SceneType::ResultFreestyle;
            } else {
                // 오픈매치 결과창 뱃지 탐지를 통한 ResultOpen3 구원 로직 이식 (씬 설정 없이 다이렉트 크롭 - ResultOpen3 mode_diff_badge coordinates)
                let temp_badge = crop_roi_direct(&frame, 212, 830, 316, 39);
                if let Some(txt) = ocr.recognize_text_all_passes(&temp_badge) {
                    let norm = txt.to_lowercase();
                    if norm.contains("tunes") || norm.contains("mode") || norm.contains("button") 
                        || norm.contains("4b") || norm.contains("5b") || norm.contains("6b") || norm.contains("8b") {
                        scene = SceneType::ResultOpen3;
                    }
                }
            }
        }
        
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
        
        // 1. Windows OCR로 현재 Rate 생 텍스트 추출 (템플릿 매칭 우회하여 수집 방해 원천 방지)
        let mut raw_txt = String::new();
        if let Some(txt) = ocr.recognize_text_color(&rate_img) {
            raw_txt = txt;
        }
        
        let mut rate_val = None;
        let clean: String = raw_txt.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(v) = clean.parse::<f32>() {
            if v >= 0.0 && v <= 100.0 {
                rate_val = Some(v);
            }
        }
        
        let Some(val) = rate_val else {
            println!("      [DEBUG collect] detect_rate failed. raw_txt='{}', filename='{}', scene={:?}", raw_txt, filename, scene);
            continue;
        };
        
        // 원본 OCR 텍스트 정규화 (예: "99.42%" -> '9', '9', '.', '4', '2', '%')
        let expected_chars: Vec<char> = raw_txt
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '%')
            .collect();
            
        if expected_chars.is_empty() {
            continue;
        }
        
        // 2. 고휘도 이진화 전처리 실행
        let binary = threshold_luminance(&rate_img.bgra, rate_img.width as usize, rate_img.height as usize);
        
        // 3. 수직 프로젝션 분할 실행
        let segments = segment_characters(&binary, rate_img.width as usize, rate_img.height as usize);
        
        println!("File: {} (Scene: {:?}) -> OCR Rate: {:.2}%, Segment count: {}, Expected Char count: {}",
                 filename, scene, val, segments.len(), expected_chars.len());
                 
        // 분할된 글자 개수와 원래 OCR의 글자 개수가 완전히 일치할 때만 안전하게 라벨링 저장
        if segments.len() == expected_chars.len() {
            for (idx, &(x1, x2)) in segments.iter().enumerate() {
                let ch = expected_chars[idx];
                let label = match ch {
                    '.' => "dot".to_string(),
                    '%' => "percent".to_string(),
                    _ => ch.to_string(),
                };
                
                let out_name = format!("{}_char_{}_{}_{}.png", label, filename.strip_suffix(".jpg").unwrap_or(&filename), idx, x1);
                let out_path = output_dir.join(out_name);
                
                if save_segment_as_png(&binary, rate_img.width as usize, rate_img.height as usize, x1, x2, &out_path).is_ok() {
                    total_saved += 1;
                }
            }
        }
    }
    
    println!("Successfully collected {} standard character masks in scratch/screenshots/digits/", total_saved);
}
