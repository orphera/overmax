//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::ui::overlay_theme::{apply_secondary_window_style, Theme};
use overmax_engine::detector::ocr_engine::OcrTelemetry;

pub fn push_log(lines: &Arc<Mutex<VecDeque<String>>>, max_lines: usize, line: String) {
    let Ok(mut g) = lines.lock() else {
        return;
    };
    if g.len() >= max_lines {
        g.pop_front();
    }
    g.push_back(line);
}

pub fn render_debug(
    ctx: &egui::Context,
    class: ViewportClass,
    title: &str,
    lines: &Arc<Mutex<VecDeque<String>>>,
    paused: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
    rate_ocr: &Arc<Mutex<Option<OcrTelemetry>>>,
    rate_ocr_texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
) {
    apply_secondary_window_style(ctx);

    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| {
            render_ocr_telemetry(ui, rate_ocr, rate_ocr_texture);
            render_controls(ui, lines, paused, filters);
            ui.add_space(8.0);
            log_scroll(ui, lines, filters);
        });
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::PANEL_BG).inner_margin(Margin::same(24)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Debug")
                            .color(Theme::TEXT_ACCENT)
                            .size(Theme::FONT_HEADING)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Logs")
                            .color(Theme::TEXT_PRIMARY)
                            .size(Theme::FONT_HEADING)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let total_lines = if let Ok(g) = lines.lock() { g.len() } else { 0 };
                        ui.label(RichText::new(format!("{} lines", total_lines)).color(Theme::TEXT_MUTED).size(Theme::FONT_TINY));
                    });
                });
                ui.add_space(16.0);

                render_ocr_telemetry(ui, rate_ocr, rate_ocr_texture);
                render_controls(ui, lines, paused, filters);
                ui.add_space(16.0);

                log_scroll(ui, lines, filters);
            });
    }
}

fn update_ocr_texture(
    ctx: &egui::Context,
    info: &OcrTelemetry,
    texture_guard: &mut Option<egui::TextureHandle>,
) {
    let should_update = match texture_guard.as_ref() {
        None => true,
        Some(handle) => {
            handle.size()[0] != info.image_width || handle.size()[1] != info.image_height
        }
    };
    let texture_name = format!("ocr_rate_{}_{}_{}", info.rate_text, info.threshold, info.use_invert);
    let should_update = should_update || match texture_guard.as_ref() {
        None => true,
        Some(handle) => handle.name() != texture_name,
    };
    
    if should_update {
        let pixels = if info.image_pixels.len() == info.image_width * info.image_height * 4 {
            info.image_pixels
                .chunks_exact(4)
                .map(|chunk| egui::Color32::from_rgba_unmultiplied(chunk[2], chunk[1], chunk[0], chunk[3]))
                .collect()
        } else {
            info.image_pixels.iter()
                .map(|&p| egui::Color32::from_gray(p))
                .collect()
        };
        let color_image = egui::ColorImage {
            size: [info.image_width, info.image_height],
            pixels,
            source_size: egui::vec2(info.image_width as f32, info.image_height as f32),
        };
        *texture_guard = Some(ctx.load_texture(
            texture_name,
            color_image,
            egui::TextureOptions::default(),
        ));
    }
}

fn render_ocr_telemetry(
    ui: &mut egui::Ui,
    rate_ocr: &Arc<Mutex<Option<OcrTelemetry>>>,
    rate_ocr_texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
) {
    let ocr_info = if let Ok(g) = rate_ocr.lock() { g.clone() } else { None };
    let Some(info) = ocr_info else { return; };
    if info.image_width == 0 || info.image_height == 0 || info.image_pixels.is_empty() {
        return;
    }
    
    let mut texture_guard = rate_ocr_texture.lock().unwrap();
    update_ocr_texture(ui.ctx(), &info, &mut texture_guard);
    
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Rate OCR Status:").strong().color(Theme::TEXT_ACCENT));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Text: \"{}\"", info.rate_text)).color(Theme::TEXT_PRIMARY));
            ui.separator();
            ui.label(RichText::new(format!("Threshold: {}", info.threshold)).color(Theme::TEXT_PRIMARY));
            ui.separator();
            ui.label(RichText::new(format!("BgMean: {:.1}", info.bg_mean)).color(Theme::TEXT_PRIMARY));
            ui.separator();
            ui.label(RichText::new(format!("Inverted: {}", info.use_invert)).color(Theme::TEXT_PRIMARY));
        });
        
        ui.add_space(6.0);
        
        if let Some(texture) = &*texture_guard {
            let max_width = 300.0;
            let ratio = texture.size()[1] as f32 / texture.size()[0] as f32;
            let display_width = (texture.size()[0] as f32).min(max_width);
            let display_size = egui::vec2(display_width, display_width * ratio);
            
            ui.horizontal(|ui| {
                ui.label(RichText::new("OCR Image:").size(Theme::FONT_TINY).color(Theme::TEXT_MUTED));
                ui.add_space(4.0);
                ui.image((texture.id(), display_size));
            });
        }
    });
    ui.add_space(8.0);
}

fn render_controls(
    ui: &mut egui::Ui,
    lines: &Arc<Mutex<VecDeque<String>>>,
    paused: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    ui.horizontal(|ui| {
        // Pause Button
        let is_paused = paused.load(Ordering::Relaxed);
        let pause_text = if is_paused { "▶ 재개" } else { "⏸ 일시정지" };
        let pause_btn = egui::Button::new(RichText::new(pause_text).size(Theme::FONT_SMALL).strong())
            .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
            .fill(if is_paused { Theme::BTN_PAUSED } else { Theme::SECONDARY })
            .corner_radius(egui::CornerRadius::same(Theme::R_SM));
        if ui.add(pause_btn).clicked() {
            paused.store(!is_paused, Ordering::Relaxed);
        }

        // Clear Button
        let clear_btn = egui::Button::new(RichText::new("🗑 지우기").size(Theme::FONT_SMALL))
            .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
            .fill(Theme::CARD)
            .stroke(Stroke::new(1.0, Theme::STROKE))
            .corner_radius(egui::CornerRadius::same(Theme::R_SM));
        if ui.add(clear_btn).clicked() {
            if let Ok(mut g) = lines.lock() {
                g.clear();
            }
        }
    });

    ui.add_space(8.0);

    // Filters Row
    ui.horizontal(|ui| {
        ui.label(RichText::new("필터:").color(Theme::TEXT_SECONDARY).size(Theme::FONT_SMALL).strong());
        ui.add_space(4.0);
        
        if let Ok(mut filters_lock) = filters.lock() {
            let tags = [
                "[ScreenCapture]",
                "[Overlay]",
                "[VArchive]",
                "[WindowTracker]",
                "[Main]",
            ];
            for tag in &tags {
                let tag_name = tag.trim_matches(|c| c == '[' || c == ']');
                if let Some(val) = filters_lock.get_mut(*tag) {
                    let color = get_line_color(tag);
                    let mut checked = *val;
                    let cb = egui::Checkbox::new(&mut checked, RichText::new(tag_name).color(color).size(Theme::FONT_SMALL));
                    if ui.add(cb).changed() {
                        *val = checked;
                    }
                }
            }
        }
    });
}

fn log_scroll(
    ui: &mut egui::Ui,
    lines: &Arc<Mutex<VecDeque<String>>>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    let snapshot: Vec<String> = lines
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();

    let filters_lock = filters.lock().unwrap();

    let filtered_lines: Vec<String> = snapshot
        .into_iter()
        .filter(|line| {
            let tags = [
                "[ScreenCapture]",
                "[Overlay]",
                "[VArchive]",
                "[WindowTracker]",
                "[Main]",
                "[UI]",
            ];
            for tag in &tags {
                if line.contains(tag) {
                    let lookup_tag = if *tag == "[UI]" { "[Main]" } else { *tag };
                    return *filters_lock.get(lookup_tag).unwrap_or(&true);
                }
            }
            true
        })
        .collect();

    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, filtered_lines.len(), |ui, range| {
                    for idx in range {
                        let line = &filtered_lines[idx];
                        let color = get_line_color(line);
                        ui.label(RichText::new(line).color(color).monospace().size(Theme::FONT_TINY));
                    }
                });
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn get_line_color(line: &str) -> Color32 {
    if line.contains("[ScreenCapture]") {
        Theme::LOG_CAPTURE
    } else if line.contains("[Overlay]") {
        Theme::LOG_OVERLAY
    } else if line.contains("[VArchive]") {
        Theme::LOG_VARCHIVE
    } else if line.contains("[WindowTracker]") {
        Theme::LOG_WINDOW
    } else if line.contains("[Main]") || line.contains("[UI]") {
        Theme::LOG_MAIN
    } else {
        Theme::LOG_DEFAULT
    }
}
