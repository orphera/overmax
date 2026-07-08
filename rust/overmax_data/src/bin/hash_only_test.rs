use rusqlite::Connection;
use std::fs;
use std::path::Path;
use image::{GenericImageView, DynamicImage};

const REF_WIDTH: f32 = 1920.0;
const REF_HEIGHT: f32 = 1080.0;
const REF_ASPECT: f32 = REF_WIDTH / REF_HEIGHT;

#[derive(Clone, Debug)]
struct ImageEntry {
    image_id: String,
    phash: u64,
    dhash: u64,
    ahash: u64,
}

#[derive(Clone, Copy, Debug)]
struct RoiRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

// NOTE: overmax-data와 overmax-engine 간의 순환 의존성(Circular Dependency) 제약으로 인해
// overmax_engine::detector::roi::RoiManager를 직접 임포트하지 못하므로,
// 해당 변환 로직(calculate_transform / transform_point)을 로컬에 모사하여 종횡비별 스케일 및 레터박스 오프셋 적용.
// 만약 RoiManager의 좌표 계산 방식이 변경될 경우 여기의 get_scaled_roi도 동기화되어야 함.
fn get_scaled_roi(w: u32, h: u32, base_roi: RoiRect) -> (u32, u32, u32, u32) {
    let w = w as f32;
    let h = h as f32;
    let current_aspect = w / h;
    
    let scale;
    let offset_x;
    let offset_y;
    
    if current_aspect > REF_ASPECT {
        scale = h / REF_HEIGHT;
        offset_x = (w - REF_WIDTH * scale) / 2.0;
        offset_y = 0.0;
    } else if current_aspect < REF_ASPECT {
        scale = w / REF_WIDTH;
        offset_x = 0.0;
        offset_y = (h - REF_HEIGHT * scale) / 2.0;
    } else {
        scale = w / REF_WIDTH;
        offset_x = 0.0;
        offset_y = 0.0;
    }
    
    let x1 = offset_x + (base_roi.x as f32 * scale);
    let y1 = offset_y + (base_roi.y as f32 * scale);
    let rw = base_roi.width as f32 * scale;
    let rh = base_roi.height as f32 * scale;
    
    (x1 as u32, y1 as u32, rw as u32, rh as u32)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "cache/image_index.db";
    let conn = Connection::open(db_path)?;
    
    // DB의 모든 엔트리 로드
    let mut stmt = conn.prepare(
        "SELECT image_id, phash, dhash, ahash
         FROM images
         WHERE id IN (SELECT MAX(id) FROM images GROUP BY image_id)
         ORDER BY id ASC",
    )?;
    
    let mut entries = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let image_id_ref = row.get_ref(0)?;
        let image_id = match image_id_ref {
            rusqlite::types::ValueRef::Integer(v) => v.to_string(),
            rusqlite::types::ValueRef::Text(v) => String::from_utf8_lossy(v).to_string(),
            _ => return Err("Invalid image_id type".into()),
        };
        let phash_str: String = row.get(1)?;
        let dhash_str: String = row.get(2)?;
        let ahash_str: String = row.get(3)?;
        
        entries.push(ImageEntry {
            image_id,
            phash: u64::from_str_radix(&phash_str, 16).unwrap_or(0),
            dhash: u64::from_str_radix(&dhash_str, 16).unwrap_or(0),
            ahash: u64::from_str_radix(&ahash_str, 16).unwrap_or(0),
        });
    }
    println!("Loaded {} entries from DB", entries.len());
    
    // 테스트용 스크린샷 이미지와 정답 리스트 (HOG로 100% 매치 검증된 곡 ID)
    let tests = [
        ("hd_test_1.png", "473", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultOpen3
        ("hd_test_2.png", "380", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultOpen3
        ("hd_test_3.png", "713", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultOpen3
        ("hd_test_4.png", "138", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultFreestyle (1920x1200)
        ("hd_test_5.png", "516", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultFreestyle
        ("hd_test_2p_1.png", "413", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultOpen2
        ("hd_test_2p_2.png", "76", RoiRect { x: 705, y: 14, width: 60, height: 60 }), // ResultOpen2
    ];
    
    let scratch_dir = Path::new("scratch");
    let mut total = 0;
    let mut correct_hash_only = 0;
    
    for &(img_name, expected_id, ref base_roi) in &tests {
        let path = scratch_dir.join(img_name);
        if !path.exists() {
            println!("{}: Not found", img_name);
            continue;
        }
        total += 1;
        
        // 이미지 로딩
        let img = match image::ImageReader::open(&path) {
            Ok(reader) => match reader.with_guessed_format() {
                Ok(reader) => match reader.decode() {
                    Ok(i) => i,
                    Err(e) => {
                        println!("Image [{}]: Decode error -> {:?}", img_name, e);
                        continue;
                    }
                },
                Err(e) => {
                    println!("Image [{}]: Guess format error -> {:?}", img_name, e);
                    continue;
                }
            },
            Err(e) => {
                println!("Image [{}]: Open error -> {:?}", img_name, e);
                continue;
            }
        };
        let (w, h) = img.dimensions();
        
        // ROI 크기 보정 계산
        let (rx, ry, rw, rh) = get_scaled_roi(w, h, *base_roi);
        
        // 이미지 크롭
        let mut cropped = img.crop_imm(rx, ry, rw, rh);
        
        // 크롭된 이미지가 64x64 사이즈가 아니라면 overmax_cv 처리를 위해 resize
        if cropped.width() != 64 || cropped.height() != 64 {
            cropped = cropped.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        }
        
        let cropped_w = cropped.width() as usize;
        let cropped_h = cropped.height() as usize;
        
        // cropped 이미지를 BGRA u8 벡터로 변환
        let rgba = cropped.to_rgba8();
        let mut bgra = rgba.into_raw();
        for chunk in bgra.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        
        // 피처 연산
        let (q_phash, q_dhash, q_ahash, _) = 
            overmax_cv::compute_image_features(&bgra, cropped_w, cropped_h, 4)
                .map_err(|e| format!("{:?}", e))?;
        
        // 해시 단독 매칭
        let mut best_match: Option<(&ImageEntry, f32)> = None;
        for db_entry in &entries {
            let p_dist = (db_entry.phash ^ q_phash).count_ones() as f32;
            let d_dist = (db_entry.dhash ^ q_dhash).count_ones() as f32;
            let a_dist = (db_entry.ahash ^ q_ahash).count_ones() as f32;
            let score = 0.5 * p_dist + 0.3 * d_dist + 0.2 * a_dist;
            
            if best_match.is_none() || score < best_match.unwrap().1 {
                best_match = Some((db_entry, score));
            }
        }
        
        if let Some((matched_entry, score)) = best_match {
            let is_correct = matched_entry.image_id == expected_id;
            if is_correct {
                correct_hash_only += 1;
            }
            println!(
                "Image [{}]: Expected -> {}, Matched -> {} (Score: {:.2}) - {}",
                img_name,
                expected_id,
                matched_entry.image_id,
                score,
                if is_correct { "OK" } else { "FAIL" }
            );
        }
    }
    
    println!("\n=== Result ===");
    println!("Total Test Screenshots: {}", total);
    println!("Hash-Only Accuracy: {} / {} ({:.2}%)", correct_hash_only, total, (correct_hash_only as f32 / total as f32) * 100.0);
    
    Ok(())
}
