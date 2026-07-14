use overmax_app::bin_utils::load_frame;
use overmax_core::SceneType;
use overmax_engine::capture::frame::CapturedFrame;
use overmax_engine::capture::frame_utils::crop_roi;
use overmax_engine::detector::ocr_engine::OcrDetector;
use overmax_engine::detector::roi::RoiManager;
use std::fs;
use std::path::{Path, PathBuf};

fn save_crop(
    frame: &CapturedFrame,
    roi: overmax_engine::detector::roi::RoiRect,
    dst_path: &Path,
) -> bool {
    let Some(cropped) = crop_roi(frame, roi) else {
        return false;
    };
    let mut bgra = cropped.bgra.clone();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2); // BGRA to RGBA
    }
    let rgba = bgra;
    if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
        cropped.width as u32,
        cropped.height as u32,
        rgba,
    ) {
        let dynamic_img = image::DynamicImage::ImageRgba8(buf);
        if dynamic_img.save(dst_path).is_ok() {
            return true;
        }
    }
    false
}

fn save_binary_crop(pixels: &[u8], width: u32, height: u32, dst_path: &Path) -> bool {
    let len = pixels.len();
    let expected_luma = (width * height) as usize;
    let expected_rgba = (width * height * 4) as usize;

    if len == expected_rgba {
        let mut rgba = pixels.to_vec();
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        if let Some(buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, rgba) {
            let dynamic_img = image::DynamicImage::ImageRgba8(buf);
            return dynamic_img.save(dst_path).is_ok();
        }
    } else if len == expected_luma {
        if let Some(buf) =
            image::ImageBuffer::<image::Luma<u8>, _>::from_raw(width, height, pixels.to_vec())
        {
            let dynamic_img = image::DynamicImage::ImageLuma8(buf);
            return dynamic_img.save(dst_path).is_ok();
        }
    }
    false
}

fn main() {
    let src_dir = Path::new("scratch/freestyle_results");
    let crops_dir = src_dir.join("crops");
    fs::create_dir_all(&crops_dir).ok();

    let mut paths: Vec<PathBuf> = Vec::new();
    if src_dir.exists() {
        if let Ok(entries) = fs::read_dir(src_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if ext == "png" || ext == "jpg" || ext == "jpeg" {
                        paths.push(path);
                    }
                }
            }
        }
    }
    paths.sort_by_key(|p| p.file_name().unwrap().to_os_string());

    println!(
        "Found {} screenshots in scratch/freestyle_results",
        paths.len()
    );
    if paths.is_empty() {
        println!("No screenshots found. Exiting.");
        return;
    }

    let ocr = OcrDetector::new();
    if !ocr.is_available() {
        println!("Windows OCR Engine is NOT available! Check your OS language settings.");
    }

    let mut analysis_report = String::new();
    analysis_report.push_str("==================================================\n");
    analysis_report.push_str("FREESTYLE RESULT SCREENSHOTS OCR ANALYSIS REPORT\n");
    analysis_report.push_str("==================================================\n\n");

    for path in &paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
        println!("\n--------------------------------------------------");
        println!("Processing image: {}", filename);

        let Some(frame) = load_frame(path) else {
            println!("Failed to load frame for {}", filename);
            continue;
        };

        // RoiManager 생성 (1920x1080 고정 해상도로 로드됨)
        let mut rois = RoiManager::new(frame.width, frame.height);
        rois.set_scene(SceneType::ResultFreestyle);

        let mut img_report = format!("--- Image: {} ---\n", filename);

        // 1. Rate 분석
        if let Some(rate_roi) = rois.get_roi("rate") {
            let rate_raw_path = crops_dir.join(format!("{}_rate_raw.png", file_stem));
            let saved_raw = save_crop(&frame, rate_roi, &rate_raw_path);

            if let Some(rate_img) = crop_roi(&frame, rate_roi) {
                // 기존 파이프라인의 detect_rate 실행
                let (parsed_val, matched_str, telemetry_opt) = ocr.detect_rate(&rate_img);

                img_report.push_str("  [Rate ROI]\n");
                img_report.push_str(&format!(
                    "    - ROI Coord: x1={}, y1={}, x2={}, y2={}\n",
                    rate_roi.x1, rate_roi.y1, rate_roi.x2, rate_roi.y2
                ));
                img_report.push_str(&format!(
                    "    - Raw Crop Saved: {} ({})\n",
                    rate_raw_path.display(),
                    if saved_raw { "OK" } else { "Failed" }
                ));
                img_report.push_str(&format!(
                    "    - detect_rate() result: parsed={:?}, matched_str='{}'\n",
                    parsed_val, matched_str
                ));

                if let Some(tel) = telemetry_opt {
                    img_report.push_str(&format!(
                        "    - Telemetry: threshold={}, bg_mean={}, use_invert={}\n",
                        tel.threshold, tel.bg_mean, tel.use_invert
                    ));
                    let rate_bin_path = crops_dir.join(format!("{}_rate_bin.png", file_stem));
                    let saved_bin = save_binary_crop(
                        &tel.image_pixels,
                        tel.image_width as u32,
                        tel.image_height as u32,
                        &rate_bin_path,
                    );
                    img_report.push_str(&format!(
                        "    - Binary Crop Saved: {} ({})\n",
                        rate_bin_path.display(),
                        if saved_bin { "OK" } else { "Failed" }
                    ));
                }

                // 추가 개별 테스트 (Windows OCR fallback 분석)
                let color_ocr = ocr.recognize_text_color(&rate_img).unwrap_or_default();
                let binarized_ocr = ocr
                    .recognize_text_binarized(&rate_img, false)
                    .unwrap_or_default();
                let binarized_invert_ocr = ocr
                    .recognize_text_binarized(&rate_img, true)
                    .unwrap_or_default();

                img_report.push_str(&format!(
                    "    - Windows OCR Color: '{}'\n",
                    color_ocr.trim()
                ));
                img_report.push_str(&format!(
                    "    - Windows OCR Binarized (Normal): '{}'\n",
                    binarized_ocr.trim()
                ));
                img_report.push_str(&format!(
                    "    - Windows OCR Binarized (Inverted): '{}'\n",
                    binarized_invert_ocr.trim()
                ));
            } else {
                img_report.push_str("  [Rate ROI] Crop failed.\n");
            }
        } else {
            img_report.push_str("  [Rate ROI] Missing in scene configuration.\n");
        }

        // 2. Score 분석
        if let Some(score_roi) = rois.get_roi("score") {
            let score_raw_path = crops_dir.join(format!("{}_score_raw.png", file_stem));
            let saved_raw = save_crop(&frame, score_roi, &score_raw_path);

            if let Some(score_img) = crop_roi(&frame, score_roi) {
                // 기존 파이프라인의 detect_score 실행
                let parsed_val = ocr.detect_score(&score_img);

                img_report.push_str("  [Score ROI]\n");
                img_report.push_str(&format!(
                    "    - ROI Coord: x1={}, y1={}, x2={}, y2={}\n",
                    score_roi.x1, score_roi.y1, score_roi.x2, score_roi.y2
                ));
                img_report.push_str(&format!(
                    "    - Raw Crop Saved: {} ({})\n",
                    score_raw_path.display(),
                    if saved_raw { "OK" } else { "Failed" }
                ));
                img_report.push_str(&format!(
                    "    - detect_score() result: parsed={:?}\n",
                    parsed_val
                ));

                // score 이진화 테스트를 모사하여 binary 파일 저장
                // ocr_engine.rs 의 match_digits_template 이진화 로직 적용
                let (binary, threshold, max_y) = overmax_cv::binarize_by_luminance(
                    &score_img.bgra,
                    score_img.width as usize,
                    score_img.height as usize,
                    overmax_cv::LumaMethod::Average,
                    |max, _| {
                        if max > 80 {
                            ((max as f32 * 0.80) as u8).max(max.saturating_sub(45))
                        } else {
                            180
                        }
                    },
                    255,
                );

                let score_bin_path = crops_dir.join(format!("{}_score_bin.png", file_stem));
                let saved_bin = save_binary_crop(
                    &binary,
                    score_img.width as u32,
                    score_img.height as u32,
                    &score_bin_path,
                );
                img_report.push_str(&format!(
                    "    - Binarization simulation: threshold={}, max_y={}\n",
                    threshold, max_y
                ));
                img_report.push_str(&format!(
                    "    - Binary Crop Saved: {} ({})\n",
                    score_bin_path.display(),
                    if saved_bin { "OK" } else { "Failed" }
                ));

                // 추가 개별 테스트
                let color_ocr = ocr.recognize_text_color(&score_img).unwrap_or_default();
                let binarized_ocr = ocr
                    .recognize_text_binarized(&score_img, false)
                    .unwrap_or_default();
                let binarized_invert_ocr = ocr
                    .recognize_text_binarized(&score_img, true)
                    .unwrap_or_default();

                img_report.push_str(&format!(
                    "    - Windows OCR Color: '{}'\n",
                    color_ocr.trim()
                ));
                img_report.push_str(&format!(
                    "    - Windows OCR Binarized (Normal): '{}'\n",
                    binarized_ocr.trim()
                ));
                img_report.push_str(&format!(
                    "    - Windows OCR Binarized (Inverted): '{}'\n",
                    binarized_invert_ocr.trim()
                ));
            } else {
                img_report.push_str("  [Score ROI] Crop failed.\n");
            }
        }
        // 3. Mode 분석
        if let Some(mode_roi) = rois.get_roi("mode_digit") {
            let mode_raw_path = crops_dir.join(format!("{}_mode_raw.png", file_stem));
            let saved_raw = save_crop(&frame, mode_roi, &mode_raw_path);

            if let Some(mode_img) = crop_roi(&frame, mode_roi) {
                let detected_mode = ocr.detect_freestyle_mode(&mode_img);
                img_report.push_str("  [Mode Digit ROI]\n");
                img_report.push_str(&format!(
                    "    - Raw Crop Saved: {} ({})\n",
                    mode_raw_path.display(),
                    if saved_raw { "OK" } else { "Failed" }
                ));
                img_report.push_str(&format!(
                    "    - detect_freestyle_mode() result: {:?}\n",
                    detected_mode
                ));

                let w = mode_img.width as usize;
                let h = mode_img.height as usize;
                if w * h > 0 {
                    let (binary, _, _) = match overmax_cv::binarize_by_global_contrast(
                        &mode_img.bgra,
                        w,
                        h,
                        overmax_cv::LumaMethod::Average,
                        1,
                    ) {
                        Ok(b) => b,
                        Err(_) => (vec![], 0, 0),
                    };
                    let visual_binary: Vec<u8> = binary
                        .iter()
                        .map(|&x| if x == 1 { 255 } else { 0 })
                        .collect();
                    let mode_bin_path = crops_dir.join(format!("{}_mode_bin.png", file_stem));
                    let saved_bin =
                        save_binary_crop(&visual_binary, w as u32, h as u32, &mode_bin_path);
                    img_report.push_str(&format!(
                        "    - Binary Crop Saved: {} ({})\n",
                        mode_bin_path.display(),
                        if saved_bin { "OK" } else { "Failed" }
                    ));
                }
            }
        }

        // 4. Difficulty 분석
        if let Some(diff_roi) = rois.get_roi("diff_panel") {
            let diff_raw_path = crops_dir.join(format!("{}_diff_raw.png", file_stem));
            let saved_raw = save_crop(&frame, diff_roi, &diff_raw_path);

            if let Some(diff_img) = crop_roi(&frame, diff_roi) {
                let detected_diff = ocr.detect_result_difficulty(&diff_img);
                img_report.push_str("  [Diff Panel ROI]\n");
                img_report.push_str(&format!(
                    "    - Raw Crop Saved: {} ({})\n",
                    diff_raw_path.display(),
                    if saved_raw { "OK" } else { "Failed" }
                ));
                img_report.push_str(&format!(
                    "    - detect_result_difficulty() result: {:?}\n",
                    detected_diff
                ));

                let w = diff_img.width as usize;
                let h = diff_img.height as usize;
                if w * h > 0 {
                    let (binary, _, _) = match overmax_cv::binarize_by_global_contrast(
                        &diff_img.bgra,
                        w,
                        h,
                        overmax_cv::LumaMethod::Average,
                        1,
                    ) {
                        Ok(b) => b,
                        Err(_) => (vec![], 0, 0),
                    };
                    let visual_binary: Vec<u8> = binary
                        .iter()
                        .map(|&x| if x == 1 { 255 } else { 0 })
                        .collect();
                    let diff_bin_path = crops_dir.join(format!("{}_diff_bin.png", file_stem));
                    let saved_bin =
                        save_binary_crop(&visual_binary, w as u32, h as u32, &diff_bin_path);
                    img_report.push_str(&format!(
                        "    - Binary Crop Saved: {} ({})\n",
                        diff_bin_path.display(),
                        if saved_bin { "OK" } else { "Failed" }
                    ));
                }
            }
        }

        img_report.push('\n');
        println!("{}", img_report);
        analysis_report.push_str(&img_report);
    }

    let report_path = Path::new("scratch/freestyle_results_ocr_analysis.txt");
    if fs::write(report_path, &analysis_report).is_ok() {
        println!("Report saved to: {}", report_path.display());
    } else {
        println!("Failed to save report to {}", report_path.display());
    }
}
