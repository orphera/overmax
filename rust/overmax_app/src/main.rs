mod cache_update;
mod debug_ui;
mod detection_pipeline;
mod detection_worker;
mod frame_utils;
mod global_hotkey;
mod hysteresis;
mod native_app;
mod native_app_commands;
mod native_app_log;
mod native_app_recommend;
mod native_app_viewports;
mod native_helpers;
mod ocr_engine;
mod overlay_recommend_ui;
mod overlay_ui;
mod play_state;
mod roi;
mod screen_capture;
mod settings_ui;
mod single_instance;
mod steam_session;
mod sync_ui;
#[cfg(target_os = "windows")]
mod tray_icon;
mod ui_command;
mod updater;
mod varchive_upload;
mod window_tracker;

#[cfg(target_os = "windows")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(wa) = updater::worker::parse_worker_args(&args) {
        std::process::exit(updater::worker::run_update_worker(wa));
    }
    if let Err(err) = native_app::run_native_app() {
        eprintln!("overmax-rs failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("overmax-rs is Windows-only because Overmax depends on Win32 window tracking, capture, hotkey, and OCR APIs.");
    std::process::exit(1);
}
