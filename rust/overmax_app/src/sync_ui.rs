//! V-Archive sync window: list candidates and trigger scan / upload.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use overmax_data::SyncCandidate;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn render_sync(
    ctx: &egui::Context,
    class: ViewportClass,
    steam_id: &mut String,
    status: &str,
    candidates: &[SyncCandidate],
    on_scan: impl Fn(),
    on_upload: impl Fn(usize) + Copy,
    on_delete: impl Fn(usize) + Copy,
) {
    let mut body = |ui: &mut egui::Ui| {
        apply_window_style(ui.ctx());
        ui.heading(
            RichText::new("V-Archive 동기화")
                .color(Theme::TEXT)
                .strong(),
        );
        ui.label(
            RichText::new("Steam 계정 기준으로 업로드 후보를 확인합니다.").color(Theme::MUTED),
        );
        ui.add_space(12.0);

        Frame::new()
            .fill(Theme::CARD)
            .stroke(Stroke::new(1.0, Theme::STROKE))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Steam ID").color(Theme::TEXT));
                    ui.text_edit_singleline(steam_id);
                    if ui.button("스캔").clicked() {
                        on_scan();
                    }
                });
                if !status.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(status).small().color(Theme::MUTED));
                }
            });

        ui.add_space(12.0);
        ui.label(
            RichText::new(format!("후보 {}개", candidates.len()))
                .color(Theme::TEXT)
                .strong(),
        );
        ui.add_space(6.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, c) in candidates.iter().enumerate() {
                    candidate_row(ui, i, c, on_upload, on_delete);
                    ui.add_space(6.0);
                }
            });
    };

    if class == ViewportClass::Embedded {
        egui::Window::new("V-Archive 동기화").show(ctx, |ui| body(ui));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::BG).inner_margin(Margin::same(18)))
            .show(ctx, |ui| body(ui));
    }
}

fn candidate_row<F: Fn(usize), D: Fn(usize)>(ui: &mut egui::Ui, index: usize, c: &SyncCandidate, on_upload: F, on_delete: D) {
    Frame::new()
        .fill(Theme::ROW)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&c.song_name).color(Theme::TEXT).strong());
                    ui.label(
                        RichText::new(format!(
                            "{} {} · {:.1}% · {}",
                            c.button_mode,
                            c.difficulty,
                            c.overmax_rate,
                            c.reason_label()
                        ))
                        .small()
                        .color(Theme::MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("등록").clicked() {
                        on_upload(index);
                    }
                    if ui.button("삭제").clicked() {
                        on_delete(index);
                    }
                });
            });
            if !c.upload_status.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{} {}", c.upload_status, c.upload_message))
                        .small()
                        .color(Theme::ACCENT),
                );
            }
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

struct Theme;

impl Theme {
    const BG: Color32 = Color32::from_rgb(17, 20, 27);
    const CARD: Color32 = Color32::from_rgb(27, 32, 42);
    const ROW: Color32 = Color32::from_rgb(32, 38, 50);
    const STROKE: Color32 = Color32::from_rgb(55, 64, 80);
    const TEXT: Color32 = Color32::from_rgb(235, 239, 247);
    const MUTED: Color32 = Color32::from_rgb(145, 154, 170);
    const ACCENT: Color32 = Color32::from_rgb(126, 200, 227);
}

fn apply_window_style(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 45, 58);
        s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 59, 76);
        s.visuals.selection.bg_fill = Color32::from_rgb(70, 105, 150);
    });
}
