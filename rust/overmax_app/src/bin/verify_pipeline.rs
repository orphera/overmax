use overmax_app::bin_utils::load_frame;
use overmax_data::ImageIndexDb;
use overmax_engine::detector::detection_pipeline::DetectionPipeline;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[cfg(windows)]
fn redirect_stdout(path: &str) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_OUTPUT_HANDLE};

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    let raw_handle = file.into_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    unsafe {
        SetStdHandle(STD_OUTPUT_HANDLE, raw_handle);
    }
    Ok(())
}

#[cfg(not(windows))]
fn redirect_stdout(_path: &str) -> std::io::Result<()> {
    Ok(())
}

fn main() {
    fs::create_dir_all("scratch").ok();
    if let Err(e) = redirect_stdout("scratch/verify_console.log") {
        eprintln!("Warning: Failed to redirect stdout to file: {:?}", e);
    }

    eprintln!("=== OVERMAX PIPELINE VERIFICATION SUITE ===");

    // 1. songs.json 로드하여 곡명 매핑 사전 생성
    let mut song_titles = HashMap::new();
    if let Ok(content) = fs::read_to_string("cache/songs.json") {
        if let Ok(Value::Array(songs)) = serde_json::from_str(&content) {
            for song in songs {
                if let (Some(id_val), Some(name_val)) = (song.get("title"), song.get("name")) {
                    let song_id = match id_val {
                        Value::Number(n) => n.as_i64().unwrap_or(0) as i32,
                        _ => 0,
                    };
                    let name = name_val.as_str().unwrap_or("UNKNOWN").to_string();
                    song_titles.insert(song_id, name);
                }
            }
        }
    }
    eprintln!("Loaded {} song titles from songs.json", song_titles.len());

    // 2. 이미지 DB 로드
    let db_path = "cache/image_index.db";
    let mut image_db = ImageIndexDb::new(db_path, 0.7)
        .with_disable_hog(false)
        .with_margin_threshold(3.0);

    if image_db.load().is_err() {
        eprintln!("Error: Failed to load image index DB.");
        return;
    }
    eprintln!("Image DB loaded successfully.");

    // 3. DetectionPipeline 초기화
    let mut pipeline = DetectionPipeline::new(image_db);

    // 4. 대상 폴더 매핑
    let target_dirs = [
        ("freestyle_songselect", "scratch/freestyle_songselect"),
        ("openmatch_songselect", "scratch/openmatch_songselect"),
        ("freestyle_results", "scratch/freestyle_results"),
        ("openmatch_results", "scratch/openmatch3_results"),
        ("other_screenshots", "scratch/other_screenshots"),
    ];

    let mut txt_log = String::new();
    let mut md_summary = String::new();

    txt_log.push_str("=== OVERMAX PIPELINE DETAILED VERIFICATION LOG ===\n\n");

    md_summary.push_str("# 📊 DJMAX RESPECT V Pipeline Verification Summary\n\n");
    md_summary.push_str(
        "이 정리본은 `verify_pipeline` 도구를 기동하여 스캔한 실시간 분석 요약 보고서입니다.\n\n",
    );

    let mut global_file_idx = 0;

    for &(label, dir_path) in &target_dirs {
        let path = Path::new(dir_path);
        if !path.exists() {
            eprintln!("\nDirectory {} does not exist. Skipping.", dir_path);
            txt_log.push_str(&format!(
                "\nDirectory {} does not exist. Skipping.\n",
                dir_path
            ));
            continue;
        }

        eprintln!("\n==================================================");
        eprintln!("Scanning Category: {} ({})", label, dir_path);
        eprintln!("==================================================");

        txt_log.push_str("\n==================================================\n");
        txt_log.push_str(&format!("Category: {} ({})\n", label, dir_path));
        txt_log.push_str("==================================================\n");

        md_summary.push_str(&format!("## 📂 Category: {}\n\n", label));
        md_summary.push_str("| 파일명 | 판독 씬 (Scene) | 곡 ID | 대조 곡명 (`songs.json`) | 모드 | 난이도 | 판독 정확도 / 특이사항 |\n");
        md_summary.push_str("| :--- | :---: | :---: | :--- | :---: | :---: | :--- |\n");

        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    let fname = p.file_name().unwrap().to_string_lossy().to_lowercase();
                    if !fname.contains("_mcbadge_")
                        && !fname.contains("cropped_")
                        && !fname.contains("debug_")
                    {
                        files.push(p);
                    }
                }
            }
        }
        files.sort();

        for f in files {
            let fname = f.file_name().unwrap().to_string_lossy().to_string();
            let Some(frame) = load_frame(&f) else {
                eprintln!("  File: {:<35} -> Error: Failed to load image", fname);
                txt_log.push_str(&format!(
                    "  File: {:<35} -> Error: Failed to load image\n",
                    fname
                ));
                continue;
            };

            global_file_idx += 1;
            let now = global_file_idx as f64 * 10.0;

            pipeline.reset();
            pipeline.detect(&frame, now - 4.0);
            pipeline.detect(&frame, now - 2.0);
            let out = pipeline.detect(&frame, now);

            let song_id_str = match out.current_song_id {
                Some(id) => id.to_string(),
                None => "None".to_string(),
            };

            let title = out
                .current_song_id
                .and_then(|id| song_titles.get(&id))
                .cloned()
                .unwrap_or("None".to_string());

            let mut mode_str = "None".to_string();
            let mut diff_str = "None".to_string();
            let mut result_str = "None".to_string();

            if let Some(ref ctx) = out.state.context {
                mode_str = ctx.mode.clone();
                diff_str = ctx.diff.clone();
                let mc_suffix = if ctx.is_max_combo { " (MAX COMBO)" } else { "" };
                result_str = if ctx.rate > 0.0 {
                    format!("{:.2}%{}", ctx.rate, mc_suffix)
                } else {
                    format!("Stable{}", mc_suffix)
                };
            }

            let scene_type = out.state.scene;

            eprintln!(
                "  File: {:<35} -> Scene={:?}, SongID={:<5} ({})",
                fname, scene_type, song_id_str, title
            );
            txt_log.push_str(&format!(
                "  File: {:<35} -> Scene={:?}, SongID={:<5} ({}) | Mode={}, Diff={}, Info={}\n",
                fname, scene_type, song_id_str, title, mode_str, diff_str, result_str
            ));

            md_summary.push_str(&format!(
                "| {} | {:?} | {} | {} | {} | {} | {} |\n",
                fname, scene_type, song_id_str, title, mode_str, diff_str, result_str
            ));
        }
        md_summary.push('\n');
    }

    // 결과 파일 저장
    fs::create_dir_all("scratch").ok();

    let mut txt_file =
        File::create("scratch/verify_result.txt").expect("Failed to create verify_result.txt");
    txt_file
        .write_all(txt_log.as_bytes())
        .expect("Failed to write to verify_result.txt");
    eprintln!("\n[Success] Detailed log saved to scratch/verify_result.txt");

    let mut md_file =
        File::create("scratch/verify_summary.md").expect("Failed to create verify_summary.md");
    md_file
        .write_all(md_summary.as_bytes())
        .expect("Failed to write to verify_summary.md");
    eprintln!("[Success] Summary Markdown saved to scratch/verify_summary.md");
    eprintln!("[Success] Full console output redirected to scratch/verify_console.log");
}
