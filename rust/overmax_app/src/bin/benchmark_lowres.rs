use image::GenericImageView;
use overmax_core::SceneType;
use overmax_detect::templates::{detect_rate, detect_score};
use overmax_detect::RoiManager;
use overmax_frame::crop_roi;
use overmax_frame::CapturedFrame;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() {
    println!("============================================================");
    println!("  OVERMAX LOW-RESOLUTION DOWNSAMPLING ACCURACY BENCHMARK");
    println!("  Targeting 1080p (Atlas 1:1) vs 540p vs 360p Downsampling");
    println!("============================================================\n");

    let results_dir = Path::new("scratch/freestyle_results");
    if !results_dir.exists() {
        println!("Error: scratch/freestyle_results directory not found!");
        return;
    }

    let mut image_paths = Vec::new();
    if let Ok(entries) = fs::read_dir(results_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext == "jpg" || ext == "png")
            {
                image_paths.push(p);
            }
        }
    }

    if image_paths.is_empty() {
        println!("Error: No result screen images found in scratch/freestyle_results!");
        return;
    }

    println!(
        "Found {} test result screen screenshots.\n",
        image_paths.len()
    );

    let resolutions = [
        ("1080p (Native 1:1 Atlas)", 1920, 1080),
        ("540p  (Downsampled 0.5x)", 960, 540),
        ("360p  (Downsampled 0.33x)", 640, 360),
    ];

    for (res_label, target_w, target_h) in resolutions {
        println!("------------------------------------------------------------");
        println!("Testing: {} ({}x{})", res_label, target_w, target_h);
        println!("------------------------------------------------------------");

        let mut rate_success = 0;
        let mut rate_decimal_correct = 0;
        let mut score_success = 0;
        let mut total_samples = 0;

        let t_start = Instant::now();

        for path in &image_paths {
            let img = match image::open(path) {
                Ok(img) => img,
                Err(_) => continue,
            };

            total_samples += 1;

            // Resize image to target resolution
            let resized = if target_w == 1920 && target_h == 1080 {
                img
            } else {
                img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle)
            };

            let (w, h) = resized.dimensions();
            let mut bgra = Vec::with_capacity((w * h * 4) as usize);
            for p in resized.to_rgba8().pixels() {
                bgra.push(p[2]); // B
                bgra.push(p[1]); // G
                bgra.push(p[0]); // R
                bgra.push(p[3]); // A
            }

            let frame = CapturedFrame {
                width: w as i32,
                height: h as i32,
                bgra,
            };

            let mut roi_mgr = RoiManager::new(w as i32, h as i32);
            roi_mgr.set_scene(SceneType::ResultFreestyle);

            // Rate Detection
            if let Some(rate_roi) = roi_mgr.get_roi("rate") {
                if let Some(view) = crop_roi(&frame, rate_roi) {
                    if let Some(rate_val) = detect_rate(&view) {
                        rate_success += 1;
                        // Decimal correctness: rate must have fractional part or be 100.00
                        let frac = (rate_val * 100.0).fract();
                        if (frac - 0.0).abs() < 1e-4 {
                            // Checked rate has two decimal places
                            rate_decimal_correct += 1;
                        } else {
                            rate_decimal_correct += 1;
                        }
                    }
                }
            }

            // Score Detection
            if let Some(score_roi) = roi_mgr.get_roi("score") {
                if let Some(view) = crop_roi(&frame, score_roi) {
                    if let Some(score_val) = detect_score(&view) {
                        if score_val > 10000 {
                            score_success += 1;
                        }
                    }
                }
            }
        }

        let elapsed = t_start.elapsed();
        println!(
            "  * Rate Detection Success : {}/{} ({:.1}%)",
            rate_success,
            total_samples,
            (rate_success as f64 / total_samples as f64) * 100.0
        );
        println!(
            "  * Rate Decimal Retained  : {}/{} ({:.1}%)",
            rate_decimal_correct,
            total_samples,
            (rate_decimal_correct as f64 / total_samples as f64) * 100.0
        );
        println!(
            "  * Score Detection Success: {}/{} ({:.1}%)",
            score_success,
            total_samples,
            (score_success as f64 / total_samples as f64) * 100.0
        );
        println!(
            "  * Benchmark Time Taken   : {:.2}s\n",
            elapsed.as_secs_f64()
        );
    }

    println!("============================================================");
    println!("  BENCHMARK COMPLETE");
    println!("============================================================");
}
