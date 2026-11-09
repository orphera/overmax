//! Linux-specific UI platform implementation.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, ViewportBuilder};
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::ui::linux_layer_overlay::LinuxLayerOverlayHandle;
use crate::ui::ui_command::UiCommand;

pub fn init_platform_on_startup() -> Result<(), String> {
    for name in ["DISPLAY", "WAYLAND_DISPLAY"] {
        if std::env::var_os(name).is_none_or(|value| value.is_empty()) {
            return Err(format!(
                "{name} is not set. Overmax requires XWayland and Wayland."
            ));
        }
    }
    Ok(())
}

pub fn show_startup_error(message: &str) {
    eprintln!("[Startup Error] {message}");
    let _ = rfd::MessageDialog::new()
        .set_title("Overmax cannot continue")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

pub fn install_cjk_fonts(ctx: &egui::Context) -> bool {
    let Ok(output) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", ":lang=ko"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(output) = String::from_utf8(output.stdout) else {
        return false;
    };
    let Some((file, index)) = parse_fontconfig_match(&output) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(file) else {
        return false;
    };

    let mut fonts = FontDefinitions::default();
    let mut font = FontData::from_owned(bytes);
    font.index = index;
    fonts.font_data.insert("cjk".into(), Arc::new(font));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);
    true
}

pub(crate) fn parse_fontconfig_match(output: &str) -> Option<(&str, u32)> {
    let mut lines = output.lines();
    let file = lines.next()?.trim();
    let index = lines.next()?.trim().parse().ok()?;
    (!file.is_empty()).then_some((file, index))
}

pub fn init_overlay_window_immediate() -> Option<isize> {
    None
}

pub fn native_options(settings: &overmax_data::Settings) -> eframe::NativeOptions {
    let _ = settings;
    let mut options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Overmax")
            .with_inner_size([2.0, 1.0])
            .with_min_inner_size([2.0, 1.0])
            .with_max_inner_size([2.0, 1.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_taskbar(false)
            .with_mouse_passthrough(true),
        ..Default::default()
    };
    options.event_loop_builder = Some(Box::new(|builder| {
        use winit::platform::x11::EventLoopBuilderExtX11;

        builder.with_x11();
    }));
    options
}

pub struct PlatformState {
    pub linux_overlay: LinuxLayerOverlayHandle,
    pub game_geometry: overmax_engine::capture::window_tracker::SharedGameGeometry,
}

impl PlatformState {
    pub fn new(
        ctx_holder: &Arc<Mutex<Option<egui::Context>>>,
        _settings: &Arc<Mutex<Value>>,
        command_tx: &Sender<UiCommand>,
        _initial_hwnd: Option<isize>,
        game_window_title: &str,
        runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
    ) -> Result<Self, String> {
        let ctx_holder_clone = ctx_holder.clone();
        let repaint = Arc::new(move || {
            if let Ok(holder) = ctx_holder_clone.lock() {
                if let Some(ctx) = &*holder {
                    ctx.request_repaint();
                }
            }
        });

        let linux_overlay = crate::ui::linux_layer_overlay::spawn(
            game_window_title.to_string(),
            command_tx.clone(),
            repaint.clone(),
            runtime_telemetry,
        )?;

        let (game_geometry, geometry_source) =
            overmax_engine::capture::window_tracker::spawn_game_geometry_source(
                game_window_title,
                repaint,
            );
        if let Err(error) = geometry_source {
            eprintln!("[LinuxOverlay] geometry source failed: {error}");
        }

        Ok(Self {
            linux_overlay,
            game_geometry,
        })
    }

    pub fn presentation_observation(
        &self,
    ) -> overmax_engine::capture::window_tracker::SharedPresentationObservation {
        self.linux_overlay.presentation_observation()
    }
}

pub fn get_local_mouse_pos(_ctx: &egui::Context, _hwnd_opt: Option<isize>) -> Option<egui::Pos2> {
    None
}

pub fn draw_custom_cursor(_painter: &egui::Painter, _p: egui::Pos2) {}

/// Detects the OS user interface language via POSIX environment variables.
/// Checks LANGUAGE -> LC_ALL -> LC_MESSAGES -> LANG in standard priority order.
/// Returns "ko", "ja", or "en" (fallback for other languages).
pub fn detect_os_language() -> &'static str {
    parse_posix_locale(&[
        std::env::var("LANGUAGE").ok(),
        std::env::var("LC_ALL").ok(),
        std::env::var("LC_MESSAGES").ok(),
        std::env::var("LANG").ok(),
    ])
}

fn parse_posix_locale(envs: &[Option<String>]) -> &'static str {
    for env in envs.iter().flatten() {
        let trimmed = env.trim();
        if trimmed.is_empty() {
            continue;
        }
        for part in trimmed.split(':') {
            let p = part.trim().to_ascii_lowercase();
            if p.starts_with("ko") {
                return "ko";
            } else if p.starts_with("ja") {
                return "ja";
            } else if p.starts_with("en") {
                return "en";
            }
        }
    }
    "en"
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_fontconfig_match_with_face_index() {
        assert_eq!(
            super::parse_fontconfig_match("fonts/NotoSansCJK.ttc\n7\n"),
            Some(("fonts/NotoSansCJK.ttc", 7))
        );
    }

    #[test]
    fn test_posix_locale_detection() {
        use super::parse_posix_locale;

        assert_eq!(parse_posix_locale(&[Some("ko_KR.UTF-8".into())]), "ko");
        assert_eq!(parse_posix_locale(&[Some("ja_JP.UTF-8".into())]), "ja");
        assert_eq!(parse_posix_locale(&[Some("en_US.UTF-8".into())]), "en");
        assert_eq!(parse_posix_locale(&[None, Some("ko_KR".into())]), "ko");
        // Colon-separated LANGUAGE priority list
        assert_eq!(
            parse_posix_locale(&[Some("de_DE:ja_JP:en_US".into())]),
            "ja"
        );
        // Unsupported language falls back to English
        assert_eq!(parse_posix_locale(&[Some("fr_FR.UTF-8".into())]), "en");
        // Empty falls back to English
        assert_eq!(parse_posix_locale(&[None, Some("".into())]), "en");
    }
}
