//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::overlay_theme::{apply_secondary_window_style, Theme};

pub fn push_log(lines: &Arc<Mutex<VecDeque<String>>>, max_lines: usize, line: String) {
    let Ok(mut g) = lines.lock() else {
        return;
    };
    while g.len() >= max_lines {
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
) {
    apply_secondary_window_style(ctx);

    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| {
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

                render_controls(ui, lines, paused, filters);
                ui.add_space(16.0);

                log_scroll(ui, lines, filters);
            });
    }
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
