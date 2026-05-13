#[cfg(target_os = "windows")]
fn main() {
    let compat = overmax_data::DataCompatibility::current();
    let state = overmax_core::GameSessionState::default();

    println!("Overmax Rust native scaffold");
    println!("settings: {}", compat.settings_user_json);
    println!("initial_state: {state}");
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("overmax-rs is Windows-only because Overmax depends on Win32 window tracking, capture, hotkey, and OCR APIs.");
    std::process::exit(1);
}
