#[cfg(target_os = "windows")]
fn main() {
    let compat = overmax_data::DataCompatibility::current();
    let state = overmax_core::GameSessionState::default();
    let mut image_db = overmax_data::ImageIndexDb::new(compat.image_index_db, 0.7);
    let image_db_status = match image_db.load() {
        Ok(count) => format!("{count} entries"),
        Err(err) => format!("unavailable ({err})"),
    };

    println!("Overmax Rust native scaffold");
    println!("settings: {}", compat.settings_user_json);
    println!("image_index: {}", image_db_status);
    println!("initial_state: {state}");
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("overmax-rs is Windows-only because Overmax depends on Win32 window tracking, capture, hotkey, and OCR APIs.");
    std::process::exit(1);
}
