use rayon::prelude::*;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const HOG_LEN: usize = 1764;

struct TestImage {
    filename: String,
    folder: String,
    expected_id: String,
    roi: RoiRect,
}

#[derive(Clone, Debug)]
struct DbEntry {
    image_id: String,
    phash: u64,
    dhash: u64,
    ahash: u64,
    grid_hist: [u8; 32],
    hog: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct RoiRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

// ROI 좌표 보정 로직 (16:9 기준 레터박스 대응)
fn get_scaled_roi(w: u32, h: u32, base_roi: RoiRect) -> (u32, u32, u32, u32) {
    let w = w as f32;
    let h = h as f32;
    let ref_width = 1920.0;
    let ref_height = 1080.0;
    let ref_aspect = ref_width / ref_height;
    let current_aspect = w / h;

    let scale;
    let offset_x;
    let offset_y;

    if current_aspect > ref_aspect {
        scale = h / ref_height;
        offset_x = (w - ref_width * scale) / 2.0;
        offset_y = 0.0;
    } else if current_aspect < ref_aspect {
        scale = w / ref_width;
        offset_x = 0.0;
        offset_y = (h - ref_height * scale) / 2.0;
    } else {
        scale = w / ref_width;
        offset_x = 0.0;
        offset_y = 0.0;
    }

    let x1 = offset_x + (base_roi.x as f32 * scale);
    let y1 = offset_y + (base_roi.y as f32 * scale);
    let rw = base_roi.width as f32 * scale;
    let rh = base_roi.height as f32 * scale;

    (x1 as u32, y1 as u32, rw as u32, rh as u32)
}

// 2x2 분할 영역별 8-bin 그레이스케일 히스토그램 (총 32바이트)
fn compute_grid_histogram(gray: &[u8], width: usize, height: usize) -> [u8; 32] {
    let mut grid_hist = [0u8; 32];
    if width < 2 || height < 2 {
        return grid_hist;
    }

    let mid_x = width / 2;
    let mid_y = height / 2;

    for gy in 0..2 {
        for gx in 0..2 {
            let start_x = gx * mid_x;
            let end_x = if gx == 1 { width } else { mid_x };
            let start_y = gy * mid_y;
            let end_y = if gy == 1 { height } else { mid_y };

            let mut bins = [0u32; 8];
            let mut count = 0u32;

            for y in start_y..end_y {
                let row_offset = y * width;
                for x in start_x..end_x {
                    let val = gray[row_offset + x];
                    let bin = (val / 32) as usize; // 8-bin
                    bins[bin.min(7)] += 1;
                    count += 1;
                }
            }

            let grid_idx = (gy * 2 + gx) * 8;
            if count > 0 {
                for i in 0..8 {
                    // 한 영역당 합이 64가 되도록 L1 정규화 (4개 영역 총합 = 256 근사)
                    grid_hist[grid_idx + i] = ((bins[i] * 64) / count) as u8;
                }
            }
        }
    }
    grid_hist
}

// DynamicImage -> BGRA u8 변환 헬퍼
fn to_bgra(img: &image::DynamicImage) -> Vec<u8> {
    let rgba = img.to_rgba8();
    let mut bgra = rgba.into_raw();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    bgra
}

fn to_gray(img: &image::DynamicImage) -> Vec<u8> {
    img.to_luma8().into_raw()
}

fn stretch_contrast(gray: &mut [u8]) {
    if gray.is_empty() {
        return;
    }
    let mut min = 255u8;
    let mut max = 0u8;
    for &val in gray.iter() {
        if val < min {
            min = val;
        }
        if val > max {
            max = val;
        }
    }
    let range = max.saturating_sub(min);
    if range > 15 {
        let range_f = range as f32;
        for val in gray.iter_mut() {
            let stretched = ((*val as f32 - min as f32) / range_f * 255.0).round();
            *val = stretched.clamp(0.0, 255.0) as u8;
        }
    }
}

// verify_summary_old.md를 파싱하여 테스트 셋 로드
fn load_test_images_from_markdown(md_path: &str) -> Vec<TestImage> {
    let content = fs::read_to_string(md_path).unwrap_or_default();
    let mut test_images = Vec::new();
    let mut current_folder = String::new();

    let freestyle_song_roi = RoiRect {
        x: 710,
        y: 533,
        width: 60,
        height: 60,
    };
    let openmatch_song_roi = RoiRect {
        x: 664,
        y: 533,
        width: 60,
        height: 60,
    };
    let results_roi = RoiRect {
        x: 705,
        y: 14,
        width: 60,
        height: 60,
    };

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("## 📂 Category:") {
            let cat = line.replace("## 📂 Category:", "").trim().to_string();
            current_folder = match cat.as_str() {
                "openmatch_results" => "scratch/openmatch3_results".to_string(),
                other => format!("scratch/{}", other),
            };
        } else if line.starts_with("|") {
            let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if parts.len() >= 4 {
                let filename = parts[1];
                let scene = parts[2];
                let expected_id = parts[3];

                if filename.ends_with(".jpg") && expected_id.parse::<u32>().is_ok() {
                    let roi = match scene {
                        "Freestyle" => freestyle_song_roi,
                        "OpenMatch" | "LadderMatch" => openmatch_song_roi,
                        "ResultFreestyle" | "ResultOpen3" | "ResultOpen2" => results_roi,
                        _ => results_roi, // 기본
                    };

                    test_images.push(TestImage {
                        filename: filename.to_string(),
                        folder: current_folder.clone(),
                        expected_id: expected_id.to_string(),
                        roi,
                    });
                }
            }
        }
    }
    test_images
}

fn parse_hog_blob(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() != HOG_LEN * std::mem::size_of::<f32>() {
        return None;
    }
    let mut values = Vec::with_capacity(HOG_LEN);
    for chunk in blob.chunks_exact(4) {
        values.push(f32::from_le_bytes(chunk.try_into().ok()?));
    }
    Some(values)
}

fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.len() != v2.len() || v1.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm1 = 0.0;
    let mut norm2 = 0.0;
    for (&x1, &x2) in v1.iter().zip(v2.iter()) {
        dot += x1 * x2;
        norm1 += x1 * x1;
        norm2 += x2 * x2;
    }
    if norm1 > 0.0 && norm2 > 0.0 {
        dot / (norm1.sqrt() * norm2.sqrt())
    } else {
        0.0
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OVERMAX SPIKE: FAST HISTOGRAM MATCHING BENCHMARK ===");

    // 1. DB에서 해시, HOG 및 히스토그램 정보 로드 (프로덕션 image_index 모듈 사용)
    let db_path = "cache/image_index.db";
    if !Path::new(db_path).exists() {
        eprintln!("Error: cache/image_index.db not found. Run db_builder or copy it first.");
        return Ok(());
    }

    let mut index_db = overmax_data::store::image_index::ImageIndexDb::new(db_path, 0.65);
    let loaded_count = index_db.load()?;
    println!(
        "Loaded {} songs (with hashes, HOGs, and metadata histograms) from DB.",
        loaded_count
    );

    let db_entries = index_db.entries();

    if db_entries.is_empty() {
        println!("Error: No entries found in cache/image_index.db. Cannot continue spike.");
        return Ok(());
    }

    // 모든 DB 항목에 히스토그램이 들어있는지 무결성 체크
    let missing_histograms = db_entries.iter().filter(|e| e.grid_hist.is_none()).count();
    if missing_histograms > 0 {
        eprintln!(
            "Warning: {} entries in cache/image_index.db are missing metadata histograms!",
            missing_histograms
        );
    }

    // 3. verify_summary_old.md를 파싱하여 전수 테스트 데이터셋 로드
    let md_path = "scratch/verify_summary_old.md";
    let test_set = load_test_images_from_markdown(md_path);
    println!("Parsed {} test entries from {}.", test_set.len(), md_path);

    // 즐겨찾기 가림 영역 해시 마스킹 비트 정의
    let mut mask_bits: u64 = 0;
    for x in 0..8 {
        mask_bits |= 1 << x;
    } // y = 0
    for y in 0..8 {
        mask_bits |= 1 << (y * 8 + 7);
    } // x = 7
    mask_bits |= 1 << 8; // y = 1, x = 0
    let hash_mask: u64 = !mask_bits;

    // 씬별 자켓 ROI 목록 (교차 오탐지 검사용)
    let freestyle_song_roi = RoiRect {
        x: 710,
        y: 533,
        width: 60,
        height: 60,
    };
    let openmatch_song_roi = RoiRect {
        x: 664,
        y: 533,
        width: 60,
        height: 60,
    };
    let results_roi = RoiRect {
        x: 705,
        y: 14,
        width: 60,
        height: 60,
    };

    let jacket_rois = vec![
        ("FreestyleSongSelect_Jacket", freestyle_song_roi),
        ("OpenMatchSongSelect_Jacket", openmatch_song_roi),
        ("ResultScene_Jacket", results_roi),
    ];

    let mut true_positives = 0;
    let mut false_positives = 0;
    let mut total_true_tests = 0;
    let mut total_false_tests = 0;

    // 유사도 통계 수집기
    let mut true_pos_similarities: Vec<f32> = Vec::new();
    let mut false_pos_similarities: Vec<f32> = Vec::new();

    // 프로파일링 정밀 분석 변수
    let mut total_hog_extract_ns = 0u64;
    let mut total_hog_match_ns = 0u64;

    let mut total_real_old_ns = 0u64;
    let mut total_real_old_skips = 0u64;

    let mut total_new_extract_ns = 0u64;
    let mut total_new_match_ns = 0u64;

    println!("\n--- [START] BENCHMARK SCANS ---");
    println!(
        "{:<45} | {:<8} | {:<8} | {:<7} | {:<7} | {:<12}",
        "Image File Path", "Expected", "Matched", "TrueOk", "FalseOk", "Speedup"
    );
    println!("{}", "-".repeat(105));

    for test in &test_set {
        let path = PathBuf::from(&test.folder).join(&test.filename);
        if !path.exists() {
            continue;
        }

        let img = image::open(&path)?;
        let (w, h) = (img.width(), img.height());

        // A. 정답 테스트셋 (True Positive) 검증
        total_true_tests += 1;
        let (rx, ry, rw, rh) = get_scaled_roi(w, h, test.roi);
        let cropped = img.crop_imm(rx, ry, rw, rh);
        let cropped_64 = cropped.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let bgra = to_bgra(&cropped_64);

        let mut gray = overmax_cv::to_gray(&bgra, 4);
        overmax_cv::stretch_contrast(&mut gray, 64, 64);
        let q_grid_hist = overmax_cv::compute_grid_histogram(&gray, 64, 64);

        // 1. 기존 매칭 방식: HOG 추출 및 900여개 코사인 유사도 순회 (시간 정밀 분리 측정)
        let t_start_hog_ext = Instant::now();
        let q_features = overmax_cv::compute_image_features(&bgra, 64, 64, 4)?;
        let q_hog = q_features.3;
        let t_dur_hog_ext = t_start_hog_ext.elapsed().as_nanos() as u64;
        total_hog_extract_ns += t_dur_hog_ext;

        let t_start_hog_match = Instant::now();
        let _matched_hog = db_entries
            .par_iter()
            .map(|entry| {
                let sim = cosine_similarity(&entry.hog, &q_hog);
                (entry.image_id.clone(), sim)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let t_dur_hog_match = t_start_hog_match.elapsed().as_nanos() as u64;
        total_hog_match_ns += t_dur_hog_match;

        let total_hog_ns = t_dur_hog_ext + t_dur_hog_match;

        // 해시 추출은 신규 방식과 실제 구버전 skip_hog 방식 모두에서 쓰이므로 여기서 한 번만 추출하고, 소요 시간만 new_ext_ns에 누적합니다.
        let t_start_hash_ext = Instant::now();
        let (q_phash, q_dhash, q_ahash) = overmax_cv::compute_image_hashes(&bgra, 64, 64, 4)?;
        let t_dur_hash_ext = t_start_hash_ext.elapsed().as_nanos() as u64;
        total_new_extract_ns += t_dur_hash_ext;

        // [추가] 실제 구버전 skip_hog 프로덕션 경로 시뮬레이터 측정
        let t_start_real_old = Instant::now();
        let compare_bits = hash_mask.count_ones() as f32; // 48.0
        let mut hash_candidates = db_entries
            .iter()
            .map(|entry| {
                let p_dist = (entry.phash ^ q_phash).count_ones();
                let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();
                let p_sim = 1.0 - (p_dist as f32 / 64.0);
                let d_sim = 1.0 - (d_dist as f32 / compare_bits);
                let a_sim = 1.0 - (a_dist as f32 / compare_bits);
                let hash_sim = 0.5 * p_sim + 0.3 * d_sim + 0.2 * a_sim;
                (entry, hash_sim)
            })
            .collect::<Vec<_>>();
        hash_candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

        let first_hash_sim = hash_candidates[0].1;
        let skip_hog = if hash_candidates.len() > 1 {
            let second_hash_sim = hash_candidates[1].1;
            let margin = first_hash_sim - second_hash_sim;
            margin >= 3.0 * 0.1 || first_hash_sim >= 0.99
        } else {
            true
        };

        let real_old_hog_match_time = if skip_hog {
            total_real_old_skips += 1;
            0
        } else {
            // HOG 연산 실행
            let ext_time = t_dur_hog_ext;
            let start_match = Instant::now();
            let _matched_hog_top_k = hash_candidates
                .iter()
                .take(10)
                .map(|&(entry, hash_sim)| {
                    let sim = cosine_similarity(&entry.hog, &q_hog);
                    let similarity = 0.45 * hash_sim + 0.55 * sim;
                    (entry.image_id.clone(), similarity)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
            ext_time + start_match.elapsed().as_nanos() as u64
        };
        let t_dur_real_old =
            t_start_real_old.elapsed().as_nanos() as u64 + t_dur_hash_ext + real_old_hog_match_time;
        total_real_old_ns += t_dur_real_old;

        // 2. 신규 히스토그램 + Early Exit 방식: 900여개 매칭 (시간 정밀 분리 측정)
        let t_start_new_match = Instant::now();
        // Rayon을 걷어낸 싱글 스레드 순차 매칭 루프 (Early Exit + WTA 유사도 계산)
        let matched = db_entries
            .iter()
            .filter_map(|entry| {
                let e_grid_hist = entry.grid_hist.as_ref()?;

                let p_dist = (entry.phash ^ q_phash).count_ones();
                let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();

                let hamming_sum = p_dist + d_dist + a_dist;

                // Early Exit: 해밍 임계치 완화 (42비트)
                if hamming_sum > 42 {
                    return None;
                }

                let mut hist_diff = 0u32;
                for (&e_h, &q_h) in e_grid_hist.iter().zip(q_grid_hist.iter()) {
                    hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                }

                // 최종 유사도 계산
                let total_compare_bits = 64.0 + compare_bits * 2.0; // 160.0
                let hash_sim = 1.0 - (hamming_sum as f32 / total_compare_bits);
                let hist_sim = 1.0 - (hist_diff as f32 / 256.0).clamp(0.0, 1.0);
                let similarity = 0.5 * hash_sim + 0.5 * hist_sim;

                Some((entry.image_id.clone(), similarity))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));

        let t_dur_new_match = t_start_new_match.elapsed().as_nanos() as u64;
        total_new_match_ns += t_dur_new_match;

        let total_new_ns = t_dur_hash_ext + t_dur_new_match;
        let speedup = total_hog_ns as f64 / total_new_ns as f64;

        let mut matched_id = "None".to_string();
        let mut true_ok = false;
        let mut matched_sim = 0.0;
        if let Some((id, similarity)) = matched {
            matched_sim = similarity;
            true_pos_similarities.push(similarity);
            if similarity >= 0.65 {
                matched_id = id;
                if matched_id == test.expected_id {
                    true_ok = true;
                    true_positives += 1;
                }
            }
        }

        // 실패한 경우 상세 디버그 정보 추출
        let mut debug_expected_sim = 0.0;
        let mut debug_expected_hamming = 0;
        let mut debug_expected_hist_diff = 0;
        if !true_ok {
            if let Some(target_entry) = db_entries.iter().find(|e| e.image_id == test.expected_id) {
                let p_dist = (target_entry.phash ^ q_phash).count_ones();
                let d_dist = ((target_entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((target_entry.ahash ^ q_ahash) & hash_mask).count_ones();
                debug_expected_hamming = p_dist + d_dist + a_dist;

                let mut hist_diff = 0u32;
                if let Some(target_hist) = target_entry.grid_hist.as_ref() {
                    for (&e_h, &q_h) in target_hist.iter().zip(q_grid_hist.iter()) {
                        hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                    }
                }
                debug_expected_hist_diff = hist_diff;

                let compare_bits = hash_mask.count_ones() as f32;
                let total_compare_bits = 64.0 + compare_bits * 2.0;
                let hash_sim = 1.0 - (debug_expected_hamming as f32 / total_compare_bits);
                let hist_sim = 1.0 - (hist_diff as f32 / 256.0).clamp(0.0, 1.0);
                debug_expected_sim = 0.5 * hash_sim + 0.5 * hist_sim;
            }
        }

        // B. 대량 오탐지 방지 (False Positive) 검증 - 다른 모든 씬의 자켓 ROI 교차 크롭 대조
        let mut false_ok = true;
        for (roi_name, base_roi) in &jacket_rois {
            // 정답 자켓 ROI인 경우는 오탐 검사에서 건너뜀
            if base_roi.x == test.roi.x && base_roi.y == test.roi.y {
                continue;
            }

            total_false_tests += 1;
            let (fx, fy, fw, fh) = get_scaled_roi(w, h, *base_roi);
            let false_cropped = img.crop_imm(fx, fy, fw, fh);
            let false_cropped_64 =
                false_cropped.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
            let false_bgra = to_bgra(&false_cropped_64);

            let mut false_gray = overmax_cv::to_gray(&false_bgra, 4);
            overmax_cv::stretch_contrast(&mut false_gray, 64, 64);
            let false_q_grid_hist = overmax_cv::compute_grid_histogram(&false_gray, 64, 64);

            let (fq_phash, fq_dhash, fq_ahash) =
                overmax_cv::compute_image_hashes(&false_bgra, 64, 64, 4)?;

            let false_matched = db_entries
                .iter()
                .filter_map(|entry| {
                    let e_grid_hist = entry.grid_hist.as_ref()?;

                    let p_dist = (entry.phash ^ fq_phash).count_ones();
                    let d_dist = ((entry.dhash ^ fq_dhash) & hash_mask).count_ones();
                    let a_dist = ((entry.ahash ^ fq_ahash) & hash_mask).count_ones();

                    let hamming_sum = p_dist + d_dist + a_dist;
                    if hamming_sum > 42 {
                        return None;
                    }

                    let mut hist_diff = 0u32;
                    for (&e_h, &q_h) in e_grid_hist.iter().zip(false_q_grid_hist.iter()) {
                        hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                    }

                    let compare_bits = hash_mask.count_ones() as f32;
                    let total_compare_bits = 64.0 + compare_bits * 2.0;
                    let hash_sim = 1.0 - (hamming_sum as f32 / total_compare_bits);
                    let hist_sim = 1.0 - (hist_diff as f32 / 256.0).clamp(0.0, 1.0);
                    let similarity = 0.5 * hash_sim + 0.5 * hist_sim;

                    Some((entry.image_id.clone(), similarity))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));

            if let Some((matched_id, similarity)) = false_matched {
                false_pos_similarities.push(similarity);
                if similarity >= 0.65 {
                    false_ok = false;
                    false_positives += 1;
                    eprintln!(
                        "  [FP ALERT] File: {}, FalseROI: {} -> Incorrectly matched to '{}' with Sim {:.4}",
                        test.filename, roi_name, matched_id, similarity
                    );
                }
            }
        }

        let file_rel_path = format!("{}/{}", test.folder.replace("scratch/", ""), test.filename);
        if true_ok {
            println!(
                "{:<45} | {:<8} | {:<8} | {:<7} | {:<7} | {:.1}x",
                file_rel_path,
                test.expected_id,
                matched_id,
                "PASS",
                if false_ok { "PASS" } else { "FAIL" },
                speedup
            );
        } else {
            println!(
                "{:<45} | {:<8} | {:<8} | {:<7} | {:<7} | {:.1}x (ExpectedSim: {:.4}, Hamming: {}, HistDiff: {}, MatchedSim: {:.4})",
                file_rel_path,
                test.expected_id,
                matched_id,
                "FAIL",
                if false_ok { "PASS" } else { "FAIL" },
                speedup,
                debug_expected_sim,
                debug_expected_hamming,
                debug_expected_hist_diff,
                matched_sim
            );
        }
    }

    // C. 완전한 엉뚱한 이미지 추가 검증 (랜덤 이미지 False Positive 검증)
    println!("\n--- [ADDITIONAL] RANDOM NOISE FALSE POSITIVE TEST ---");

    // 100% 랜덤 노이즈 이미지 런타임 빌드 (False Positive 보장용 대안셋)
    let mut noise_img = image::ImageBuffer::new(120, 120);
    for (x, y, pixel) in noise_img.enumerate_pixels_mut() {
        let val = ((x * y) % 256) as u8;
        *pixel = image::Rgb([val, val, val]);
    }
    let noise_dynamic = image::DynamicImage::ImageRgb8(noise_img);

    // 빈 검정 단색 이미지 빌드
    let black_dynamic = image::DynamicImage::ImageRgb8(image::ImageBuffer::new(120, 120));

    let random_images = vec![
        ("black_solid", black_dynamic),
        ("random_noise", noise_dynamic),
    ];

    for (name, img) in &random_images {
        total_false_tests += 1;
        let bgra = to_bgra(&img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3));

        let mut gray = overmax_cv::to_gray(&bgra, 4);
        overmax_cv::stretch_contrast(&mut gray, 64, 64);
        let q_grid_hist = overmax_cv::compute_grid_histogram(&gray, 64, 64);

        let (rq_phash, rq_dhash, rq_ahash) = overmax_cv::compute_image_hashes(&bgra, 64, 64, 4)?;

        let matched = db_entries
            .iter()
            .filter_map(|entry| {
                let e_grid_hist = entry.grid_hist.as_ref()?;

                let p_dist = (entry.phash ^ rq_phash).count_ones();
                let d_dist = ((entry.dhash ^ rq_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ rq_ahash) & hash_mask).count_ones();
                let hamming_sum = p_dist + d_dist + a_dist;
                if hamming_sum > 42 {
                    return None;
                }
                let mut hist_diff = 0u32;
                for (&e_h, &q_h) in e_grid_hist.iter().zip(q_grid_hist.iter()) {
                    hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                }
                let compare_bits = hash_mask.count_ones() as f32;
                let total_compare_bits = 64.0 + compare_bits * 2.0;
                let hash_sim = 1.0 - (hamming_sum as f32 / total_compare_bits);
                let hist_sim = 1.0 - (hist_diff as f32 / 256.0).clamp(0.0, 1.0);
                let similarity = 0.5 * hash_sim + 0.5 * hist_sim;

                Some((entry.image_id.clone(), similarity))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));

        let mut rand_ok = true;
        if let Some((_, similarity)) = matched {
            false_pos_similarities.push(similarity);
            if similarity >= 0.65 {
                rand_ok = false;
                false_positives += 1;
            }
        }
        println!(
            "Type: {:<35} -> Match Result: {} (Expect None) | {}",
            name,
            if rand_ok {
                "None (OK)"
            } else {
                "Matched (FAIL)"
            },
            if rand_ok { "PASS" } else { "FAIL" }
        );
    }

    // 통계 연산 및 출력 (단위: microsecond)
    let avg_hog_ext_us = (total_hog_extract_ns / total_true_tests) as f64 / 1000.0;
    let avg_hog_match_us = (total_hog_match_ns / total_true_tests) as f64 / 1000.0;

    let avg_new_ext_us = (total_new_extract_ns / total_true_tests) as f64 / 1000.0;
    let avg_new_match_us = (total_new_match_ns / total_true_tests) as f64 / 1000.0;

    let match_speedup = avg_hog_match_us / avg_new_match_us;
    let total_speedup = (avg_hog_ext_us + avg_hog_match_us) / (avg_new_ext_us + avg_new_match_us);

    println!("\n=== BENCHMARK REPORT SUMMARY ===");
    println!("Total True Tests Checked: {}", total_true_tests);
    println!(
        "True Positives Matched  : {} / {} ({:.2}%)",
        true_positives,
        total_true_tests,
        (true_positives as f32 / total_true_tests as f32) * 100.0
    );
    println!("Total False Tests Checked: {}", total_false_tests);
    println!(
        "False Positives Detected: {} / {} ({:.2}%)",
        false_positives,
        total_false_tests,
        (false_positives as f32 / total_false_tests as f32) * 100.0
    );
    println!("{}", "-".repeat(60));
    println!("⏱️ [1단계: 쿼리 이미지 특징 추출 시간 비교 (평균)]");
    println!("   - HOG 특징 추출 (1764차원) : {:.2} us", avg_hog_ext_us);
    println!(
        "   - Hash 특징 추출 (3종 해시)  : {:.2} us (DCT 연산 등 포함)",
        avg_new_ext_us
    );
    println!(
        "   - 특징 추출 부문 Speedup    : {:.2}x",
        avg_hog_ext_us / avg_new_ext_us
    );
    println!("{}", "-".repeat(60));
    println!("⏱️ [2단계: 900여개 DB 순회 매칭 연산 시간 비교 (평균)]");
    println!(
        "   - 기존 HOG 코사인 유사도 900회 : {:.2} us",
        avg_hog_match_us
    );
    println!(
        "   - 신규 1차 Early Exit + 2차 L1  : {:.2} us (Rayon 병렬화)",
        avg_new_match_us
    );
    println!(
        "   - 순수 매칭 부문 Speedup        : {:.2}x faster",
        match_speedup
    );
    println!("{}", "-".repeat(60));
    println!("🚀 [종합 파이프라인(추출+매칭) 총 연산 시간 Speedup]");
    println!(
        "   - 기존 HOG 파이프라인 : {:.2} us",
        avg_hog_ext_us + avg_hog_match_us
    );
    println!(
        "   - 신규 매칭 파이프라인 : {:.2} us",
        avg_new_ext_us + avg_new_match_us
    );
    println!("   - 종합 파이프라인 Speedup : {:.2}x", total_speedup);
    println!("=================================");

    Ok(())
}
