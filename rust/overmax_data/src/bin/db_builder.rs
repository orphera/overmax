use clap::Parser;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "V-Archive Jacket Image Feature DB Builder")]
struct Args {
    /// Newly downloaded jacket images directory
    #[arg(short, long)]
    image_dir: PathBuf,

    /// Target SQLite image_index.db file path
    #[arg(short, long, default_value = "image_index.db")]
    db_path: PathBuf,
}

struct ProcessTask {
    song_id: String,
    path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 1. Open Database & Ensure Schema
    let mut conn = Connection::open(&args.db_path)?;
    ensure_schema(&mut conn)?;

    // 2. Scan Temporary Directory for Images
    let mut tasks = Vec::new();
    if args.image_dir.exists() {
        for entry in fs::read_dir(&args.image_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "png" {
                        if let Some(song_id) = path.file_stem().and_then(|s| s.to_str()) {
                            tasks.push(ProcessTask {
                                song_id: song_id.to_string(),
                                path,
                            });
                        }
                    }
                }
            }
        }
    }

    if tasks.is_empty() {
        println!("[Builder] No images found to process.");
        return Ok(());
    }

    println!(
        "[Builder] Start processing {} images in parallel...",
        tasks.len()
    );

    // 3. Process Features in Parallel (phash, dhash, ahash, HOG)
    let results: Vec<(String, Result<(u64, u64, u64, Vec<f32>), String>)> = tasks
        .into_par_iter()
        .map(|task| {
            let res = process_image(&task.path);
            (task.song_id, res)
        })
        .collect();

    // 4. Batch Upsert into Database (Single Transaction)
    let tx = conn.transaction()?;
    let mut success_count = 0;
    let total_tasks = results.len();

    for (song_id, feat_res) in results {
        match feat_res {
            Ok((phash, dhash, ahash, hog)) => {
                let phash_str = format!("{:016x}", phash);
                let dhash_str = format!("{:016x}", dhash);
                let ahash_str = format!("{:016x}", ahash);
                let hog_bytes = f32_vec_to_bytes(&hog);
                tx.execute(
                    "INSERT INTO images (image_id, phash, dhash, ahash, hog, orb)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL)
                     ON CONFLICT(image_id) DO UPDATE SET
                         phash = excluded.phash,
                         dhash = excluded.dhash,
                         ahash = excluded.ahash,
                         hog   = excluded.hog,
                         orb   = NULL",
                    params![song_id, phash_str, dhash_str, ahash_str, hog_bytes],
                )?;
                success_count += 1;
            }
            Err(e) => {
                eprintln!("[Builder] Failed to process {}: {}", song_id, e);
            }
        }
    }
    tx.commit()?;

    println!(
        "[Builder] Completed. Successfully indexed {}/{} images.",
        success_count, total_tasks
    );
    Ok(())
}

fn process_image(path: &Path) -> Result<(u64, u64, u64, Vec<f32>), String> {
    // 1. Read Raw File Bytes
    let bytes = fs::read(path).map_err(|e| e.to_string())?;

    // 2. Decode using the image crate
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let width = img.width() as usize;
    let height = img.height() as usize;

    let rgba = img.to_rgba8();
    let mut bgra = rgba.into_raw();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2); // Swap Red and Blue to get BGRA
    }

    // 3. Compute Features via overmax_cv (guarantees identical logic to overlay runtime)
    let (phash, dhash, ahash, hog) =
        overmax_cv::compute_image_features(&bgra, width, height, 4)
            .map_err(|e| format!("{:?}", e))?;

    Ok((phash, dhash, ahash, hog))
}

fn ensure_schema(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS images (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            image_id TEXT NOT NULL,
            phash    TEXT NOT NULL,
            dhash    TEXT NOT NULL,
            ahash    TEXT NOT NULL,
            hog      BLOB NOT NULL,
            orb      BLOB
        )",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_images_image_id ON images (image_id)",
        [],
    )?;
    Ok(())
}

fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|&val| val.to_le_bytes()).collect()
}
