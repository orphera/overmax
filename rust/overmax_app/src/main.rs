mod detection_pipeline;
mod frame_utils;
mod hysteresis;
mod play_state;
mod roi;
mod screen_capture;
mod window_tracker;

#[cfg(target_os = "windows")]
fn main() {
    let compat = overmax_data::DataCompatibility::current();
    let state = overmax_core::GameSessionState::default();
    let mut image_db = overmax_data::ImageIndexDb::new(compat.image_index_db, 0.6);
    let image_db_status = match image_db.load() {
        Ok(count) => format!("{count} entries"),
        Err(err) => format!("unavailable ({err})"),
    };

    println!("Overmax Rust native scaffold");
    println!("settings: {}", compat.settings_user_json);
    println!("image_index: {}", image_db_status);
    print_runtime_probe(image_db);
    println!("initial_state: {state}");
}

#[cfg(target_os = "windows")]
fn print_runtime_probe(image_db: overmax_data::ImageIndexDb) {
    let tracker = window_tracker::WindowTracker::new("DJMAX RESPECT V");
    match tracker.game_rect() {
        Some(rect) => {
            let foreground = tracker.is_foreground();
            let capture = screen_capture::ScreenCapturer::new()
                .and_then(|capturer| capturer.capture_bgra(rect));
            let capture_status = capture
                .as_ref()
                .map(|frame| format!("capture={}x{} bgra", frame.width, frame.height))
                .unwrap_or_else(|err| format!("capture unavailable ({err})"));
            println!(
                "window: {}x{} @ ({},{}), foreground={foreground}, {capture_status}",
                rect.width, rect.height, rect.left, rect.top
            );
            if let Ok(frame) = capture {
                let mut pipeline = detection_pipeline::DetectionPipeline::new(image_db);
                let state = pipeline.process_frame(&frame, false, 0.0).state;
                println!("detection_probe: {state}");
            }
        }
        None => println!("window: not found"),
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("overmax-rs is Windows-only because Overmax depends on Win32 window tracking, capture, hotkey, and OCR APIs.");
    std::process::exit(1);
}
