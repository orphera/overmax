//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    roi: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| {
            render_controls(ui, lines, paused, roi, filters);
            ui.add_space(8.0);
            log_scroll(ui, lines, filters);
        });
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::BG).inner_margin(Margin::same(18)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new(title).color(Theme::TEXT).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let total_lines = if let Ok(g) = lines.lock() { g.len() } else { 0 };
                        ui.label(RichText::new(format!("총 {total_lines}줄")).color(Theme::MUTED).font(FontId::proportional(11.0)));
                    });
                });
                ui.add_space(8.0);

                render_controls(ui, lines, paused, roi, filters);
                ui.add_space(12.0);

                log_scroll(ui, lines, filters);
            });
    }
}

fn render_controls(
    ui: &mut egui::Ui,
    lines: &Arc<Mutex<VecDeque<String>>>,
    paused: &Arc<AtomicBool>,
    roi: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    ui.horizontal(|ui| {
        // Pause Button
        let is_paused = paused.load(Ordering::Relaxed);
        let pause_text = if is_paused { "▶ 재개" } else { "⏸ 일시정지" };
        let pause_btn = egui::Button::new(pause_text)
            .fill(if is_paused { Color32::from_rgb(90, 58, 26) } else { Theme::CARD })
            .stroke(Stroke::new(1.0, Theme::STROKE));
        if ui.add(pause_btn).clicked() {
            paused.store(!is_paused, Ordering::Relaxed);
        }

        // Clear Button
        let clear_btn = egui::Button::new("🗑 지우기")
            .fill(Theme::CARD)
            .stroke(Stroke::new(1.0, Theme::STROKE));
        if ui.add(clear_btn).clicked() {
            if let Ok(mut g) = lines.lock() {
                g.clear();
            }
        }

        // ROI Overlay Button
        let is_roi = roi.load(Ordering::Relaxed);
        let roi_text = if is_roi { "ROI 표시 ON" } else { "ROI 표시 OFF" };
        let roi_btn = egui::Button::new(roi_text)
            .fill(if is_roi { Color32::from_rgb(31, 90, 58) } else { Theme::CARD })
            .stroke(Stroke::new(1.0, Theme::STROKE));
        if ui.add(roi_btn).clicked() {
            roi.store(!is_roi, Ordering::Relaxed);
        }
    });

    ui.add_space(6.0);

    // Filters Row
    ui.horizontal(|ui| {
        ui.label(RichText::new("필터:").color(Theme::MUTED).font(FontId::proportional(11.0)));
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
                    let cb = egui::Checkbox::new(&mut checked, RichText::new(tag_name).color(color).font(FontId::proportional(11.0)));
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
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            ScrollArea::vertical()
                .stick_to_bottom(true)
                .show_rows(ui, row_height, filtered_lines.len(), |ui, range| {
                    for idx in range {
                        let line = &filtered_lines[idx];
                        let color = get_line_color(line);
                        ui.monospace(RichText::new(line).color(color));
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
        Color32::from_rgb(126, 200, 227) // 하늘
    } else if line.contains("[Overlay]") {
        Color32::from_rgb(181, 234, 215) // 민트
    } else if line.contains("[VArchive]") {
        Color32::from_rgb(255, 214, 165) // 살구
    } else if line.contains("[WindowTracker]") {
        Color32::from_rgb(201, 177, 255) // 보라
    } else if line.contains("[Main]") || line.contains("[UI]") {
        Color32::from_rgb(255, 255, 181) // 노랑
    } else {
        Color32::from_rgb(204, 204, 204) // 기본 회색
    }
}

struct Theme;

impl Theme {
    const BG: Color32 = Color32::from_rgb(26, 26, 46);
    const CARD: Color32 = Color32::from_rgb(13, 13, 26);
    const STROKE: Color32 = Color32::from_rgb(51, 51, 51);
    const TEXT: Color32 = Color32::from_rgb(204, 204, 204);
    const MUTED: Color32 = Color32::from_rgb(102, 102, 102);
}
