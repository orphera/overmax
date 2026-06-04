use std::path::Path;
use serde_json::Value;

use overmax_app::cache_update;

fn main() {
    // Current directory is C:\Users\jeongwoong\dev\overmax\rust when run from cargo
    // We should use the workspace root directory (which is "..")
    let root = Path::new("..");
    let defaults: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../settings.json"
    )))
    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
    
    let mut merged = overmax_data::settings::load_merged_settings(root, defaults);
    overmax_data::settings::normalize_settings(&mut merged);
    
    // Force stale by deleting the old cache file if it exists
    let path = root.join("cache/pattern_meta.json");
    if path.exists() {
        println!("Deleting old cache/pattern_meta.json to force refresh...");
        let _ = std::fs::remove_file(&path);
    }
    
    println!("Refreshing startup caches...");
    cache_update::refresh_startup_caches(root, &merged, &mut |msg| {
        println!("{}", msg);
    });
    println!("Refresh done!");
}
