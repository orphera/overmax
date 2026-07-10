use std::fs;
use std::path::{Path, PathBuf};
use overmax_engine::detector::roi::RoiManager;
use overmax_engine::capture::frame_utils::crop_roi;
use overmax_core::SceneType;
use overmax_app::bin_utils::load_frame;

// Hash score distance calculator
fn calculate_hash_score(phash: u64, dhash: u64, ahash: u64, t_phash: u64, t_dhash: u64, t_ahash: u64) -> f32 {
    let p_dist = (phash ^ t_phash).count_ones() as f32;
    let d_dist = (dhash ^ t_dhash).count_ones() as f32;
    let a_dist = (ahash ^ t_ahash).count_ones() as f32;
    0.5 * p_dist + 0.3 * d_dist + 0.2 * a_dist
}

// Result screen Perfect Play (100.0%) badge template hashes
const TEMPLATE_RESULT_PERFECT_PHASH: u64 = 0xdea7c998117c851e;
const TEMPLATE_RESULT_PERFECT_DHASH: u64 = 0xd455544439b5b5a5;
const TEMPLATE_RESULT_PERFECT_AHASH: u64 = 0x3fbdf4e014ddd450;

// Result screen Max Combo badge template hashes
const TEMPLATE_RESULT_MC_PHASH: u64 = 0xda5a52d2123b2fe8;
const TEMPLATE_RESULT_MC_DHASH: u64 = 0x2929137dd4ef210f;
const TEMPLATE_RESULT_MC_AHASH: u64 = 0xd4fce007fffffc00;

fn analyze_folder(dir_path: &Path, default_scene: SceneType, threshold: f32) -> (u32, u32, u32) {
    let mut total = 0;
    let mut perfect_count = 0;
    let mut fc_count = 0;

    println!("\n==================================================");
    println!("Analyzing Folder: {}", dir_path.display());
    println!("==================================================");

    if !dir_path.exists() {
        println!("Directory does not exist.");
        return (0, 0, 0);
    }

    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort_by_key(|p| p.file_name().unwrap().to_os_string());

    for path in paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(frame) = load_frame(&path) else {
            println!("  - {}: Failed to load image", filename);
            continue;
        };

        let mut rois = RoiManager::new(frame.width, frame.height);
        rois.set_scene(default_scene);

        let Some(roi) = rois.get_roi("max_combo_badge") else {
            println!("  - {}: ROI 'max_combo_badge' not found", filename);
            continue;
        };

        let Some(badge_img) = crop_roi(&frame, roi) else {
            println!("  - {}: Crop failed", filename);
            continue;
        };

        let Ok((phash, dhash, ahash)) = overmax_cv::compute_image_hashes(
            &badge_img.bgra,
            badge_img.width as usize,
            badge_img.height as usize,
            4
        ) else {
            println!("  - {}: Hash calculation failed", filename);
            continue;
        };

        let mut r_sum = 0.0;
        let mut g_sum = 0.0;
        let mut b_sum = 0.0;
        let total_pixels = (badge_img.width * badge_img.height) as f32;
        for chunk in badge_img.bgra.chunks_exact(4) {
            let b = chunk[0] as f32;
            let g = chunk[1] as f32;
            let r = chunk[2] as f32;
            b_sum += b;
            g_sum += g;
            r_sum += r;
        }
        let avg_r = r_sum / total_pixels;
        let avg_g = g_sum / total_pixels;
        let avg_b = b_sum / total_pixels;
        let brightness = 0.299 * avg_r + 0.587 * avg_g + 0.114 * avg_b;

        let score_perfect = calculate_hash_score(phash, dhash, ahash, TEMPLATE_RESULT_PERFECT_PHASH, TEMPLATE_RESULT_PERFECT_DHASH, TEMPLATE_RESULT_PERFECT_AHASH);
        let score_mc = calculate_hash_score(phash, dhash, ahash, TEMPLATE_RESULT_MC_PHASH, TEMPLATE_RESULT_MC_DHASH, TEMPLATE_RESULT_MC_AHASH);

        let is_perfect = score_perfect <= threshold;
        let is_mc = score_mc <= threshold;

        total += 1;
        let status = if is_perfect {
            perfect_count += 1;
            "PERFECT PLAY"
        } else if is_mc {
            fc_count += 1;
            "MAX COMBO (FC)"
        } else {
            "NONE"
        };

        println!(
            "  - {}: status={}, perfect_score={:.1}, mc_score={:.1} | AvgRGB=({:.1}, {:.1}, {:.1}) Bright={:.1}",
            filename, status, score_perfect, score_mc, avg_r, avg_g, avg_b, brightness
        );
    }

    println!("--------------------------------------------------");
    println!("Summary for {}:", dir_path.display());
    println!("  Total processed: {}", total);
    println!("  Perfect Play:    {}", perfect_count);
    println!("  Full Combo (FC): {}", fc_count);
    println!("==================================================");

    (total, perfect_count, fc_count)
}

fn main() {
    let openmatch3_dir = Path::new("scratch/openmatch3_results");
    analyze_folder(openmatch3_dir, SceneType::ResultOpen3, 20.0);

    let freestyle_dir = Path::new("scratch/freestyle_results");
    analyze_folder(freestyle_dir, SceneType::ResultFreestyle, 20.0);
}
