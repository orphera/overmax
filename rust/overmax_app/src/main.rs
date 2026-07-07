#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = overmax_app::ui::native_app::run_native_app() {
        eprintln!("overmax-rs failed: {err}");
        std::process::exit(1);
    }
}
