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

pub fn native_options(settings: &overmax_data::Settings) -> eframe::NativeOptions {
    let _ = settings;
    eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Overmax")
            .with_inner_size([1.0, 1.0])
            .with_min_inner_size([1.0, 1.0])
            .with_max_inner_size([1.0, 1.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_mouse_passthrough(true),
        ..Default::default()
    }
}

pub struct PlatformState {
    pub linux_overlay: LinuxLayerOverlayHandle,
}

impl PlatformState {
    pub fn new(
        ctx_holder: &Arc<Mutex<Option<egui::Context>>>,
        _settings: &Arc<Mutex<Value>>,
        command_tx: &Sender<UiCommand>,
    ) -> Result<Self, String> {
        let ctx_holder_clone = ctx_holder.clone();
        let repaint = Arc::new(move || {
            if let Ok(holder) = ctx_holder_clone.lock() {
                if let Some(ctx) = &*holder {
                    ctx.request_repaint();
                }
            }
        });

        let linux_overlay = crate::ui::linux_layer_overlay::spawn(command_tx.clone(), repaint)?;

        Ok(Self { linux_overlay })
    }
}

pub fn get_local_mouse_pos(_ctx: &egui::Context, _hwnd_opt: Option<isize>) -> Option<egui::Pos2> {
    None
}

pub fn draw_custom_cursor(_painter: &egui::Painter, _p: egui::Pos2) {}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_fontconfig_match_with_face_index() {
        assert_eq!(
            super::parse_fontconfig_match("fonts/NotoSansCJK.ttc\n7\n"),
            Some(("fonts/NotoSansCJK.ttc", 7))
        );
    }
}
