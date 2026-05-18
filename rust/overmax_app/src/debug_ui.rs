//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
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

pub fn drain_channel(
    lines: &Arc<Mutex<VecDeque<String>>>,
    rx: &std::sync::mpsc::Receiver<String>,
    max_lines: usize,
) {
    while let Ok(msg) = rx.try_recv() {
        push_log(lines, max_lines, msg);
    }
}

pub fn render_debug(
    ctx: &egui::Context,
    class: ViewportClass,
    title: &str,
    lines: &Arc<Mutex<VecDeque<String>>>,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| log_scroll(ui, lines));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::BG).inner_margin(Margin::same(18)))
            .show(ctx, |ui| {
                ui.heading(RichText::new(title).color(Theme::TEXT).strong());
                ui.label(RichText::new("최근 런타임 로그").color(Theme::MUTED));
                ui.add_space(12.0);
                log_scroll(ui, lines);
            });
    }
}

fn log_scroll(ui: &mut egui::Ui, lines: &Arc<Mutex<VecDeque<String>>>) {
    let snapshot: Vec<String> = lines
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                ui.monospace(RichText::new(snapshot.join("\n")).color(Theme::TEXT));
            });
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
    }
}

struct Theme;

impl Theme {
    const BG: Color32 = Color32::from_rgb(17, 20, 27);
    const CARD: Color32 = Color32::from_rgb(20, 24, 32);
    const STROKE: Color32 = Color32::from_rgb(55, 64, 80);
    const TEXT: Color32 = Color32::from_rgb(220, 226, 238);
    const MUTED: Color32 = Color32::from_rgb(145, 154, 170);
}
