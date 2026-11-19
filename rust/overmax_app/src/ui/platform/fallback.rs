//! Fallback UI platform implementation for unsupported OS.

use eframe::egui;
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::ui::ui_command::UiCommand;

pub fn init_platform_on_startup() -> Result<(), String> {
    Ok(())
}

pub fn show_startup_error(message: &str) {
    eprintln!("[Startup Error] {message}");
}

pub fn install_cjk_fonts(_ctx: &egui::Context) -> bool {
    false
}

pub fn init_overlay_window_immediate() -> Option<isize> {
    None
}

pub fn native_options(_settings: &overmax_data::Settings) -> eframe::NativeOptions {
    eframe::NativeOptions::default()
}

pub struct PlatformState;

impl PlatformState {
    pub fn new(
        _ctx_holder: &Arc<Mutex<Option<egui::Context>>>,
        _settings: &Arc<Mutex<Value>>,
        _command_tx: &Sender<UiCommand>,
        _initial_hwnd: Option<isize>,
        _game_window_title: &str,
        _runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
    ) -> Result<Self, String> {
        Ok(Self)
    }

    pub fn presentation_observation(&self) -> overmax_capture::SharedPresentationObservation {
        std::sync::Arc::new(std::sync::Mutex::new(None))
    }
}

pub fn get_local_mouse_pos(_ctx: &egui::Context, _hwnd_opt: Option<isize>) -> Option<egui::Pos2> {
    None
}

pub fn draw_custom_cursor(_painter: &egui::Painter, _p: egui::Pos2) {}

pub fn detect_os_language() -> &'static str {
    "en"
}
