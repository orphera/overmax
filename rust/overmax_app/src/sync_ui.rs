//! V-Archive sync window: list candidates and trigger scan / upload.

use crate::overlay_theme::{apply_secondary_window_style, Theme};
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
        apply_secondary_window_style(ui.ctx());
        
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("V-Archive")
                    .color(Theme::TEXT_ACCENT)
                    .size(Theme::FONT_HEADING)
                    .strong(),
            );
            ui.label(
                RichText::new("동기화")
                    .color(Theme::TEXT_PRIMARY)
                    .size(Theme::FONT_HEADING)
                    .strong(),
            );
        });
        
        ui.add_space(4.0);
        ui.label(
            RichText::new("Steam 계정 기준으로 업로드 후보를 확인합니다.")
                .color(Theme::TEXT_SECONDARY)
                .size(Theme::FONT_BODY),
        );
        ui.add_space(20.0);

        Frame::new()
            .fill(Theme::CARD)
            .stroke(Stroke::new(1.0, Theme::STROKE))
            .corner_radius(CornerRadius::same(Theme::R_MD))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(80.0, Theme::CONTROL_HEIGHT),
                        egui::Label::new(RichText::new("Steam ID").color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY)),
                    );
                    ui.add_space(8.0);
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let scan_btn = egui::Button::new(RichText::new("스캔").size(Theme::FONT_BODY).strong())
                            .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
                            .fill(Theme::PRIMARY)
                            .corner_radius(egui::CornerRadius::same(Theme::R_SM));

                        if ui.add(scan_btn).clicked() {
                            on_scan();
                        }
                        
                        ui.add_space(8.0);
                        
                        ui.add(egui::TextEdit::singleline(steam_id)
                            .font(egui::FontId::proportional(Theme::FONT_BODY))
                            .vertical_align(egui::Align::Center)
                            .margin(egui::Margin::symmetric(8, 0))
                            .desired_width(ui.available_width())
                            .min_size(egui::vec2(0.0, Theme::CONTROL_HEIGHT)));
                    });
                });
                if !status.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(status).size(Theme::FONT_SMALL).color(Theme::TEXT_MUTED));
                }
            });

        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("업로드 후보")
                    .color(Theme::TEXT_PRIMARY)
                    .size(Theme::FONT_BODY)
                    .strong(),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{}", candidates.len()))
                    .color(Theme::TEXT_ACCENT)
                    .size(Theme::FONT_BODY)
                    .strong(),
            );
        });
        ui.add_space(12.0);
        
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().spacing.item_spacing.y = 12.0;
                for (i, c) in candidates.iter().enumerate() {
                    candidate_row(ui, i, c, on_upload, on_delete);
                }
            });
    };

    if class == ViewportClass::Embedded {
        egui::Window::new("V-Archive 동기화").show(ctx, |ui| body(ui));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::PANEL_BG).inner_margin(Margin::same(24)))
            .show(ctx, |ui| body(ui));
    }
}

fn candidate_row<F: Fn(usize), D: Fn(usize)>(ui: &mut egui::Ui, index: usize, c: &SyncCandidate, on_upload: F, on_delete: D) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&c.song_name).color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY).strong());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        // Button Mode Badge
                        badge(ui, &c.button_mode, Theme::SECONDARY, Theme::TEXT_PRIMARY);
                        ui.add_space(4.0);
                        // Difficulty Badge
                        let diff_color = match c.difficulty.as_str() {
                            "SC" => Theme::DANGER,
                            "MAX" => Theme::PRIMARY,
                            _ => Theme::TAB_DIM_BG,
                        };
                        badge(ui, &c.difficulty, diff_color, Theme::TEXT_BRIGHT);
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("{:.2}%", c.overmax_rate))
                                .size(Theme::FONT_SMALL)
                                .color(Theme::TEXT_ACCENT)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(c.reason_label())
                                .size(Theme::FONT_SMALL)
                                .color(Theme::TEXT_MUTED),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let upload_btn = egui::Button::new(RichText::new("등록").size(Theme::FONT_SMALL).strong())
                        .min_size(egui::vec2(60.0, Theme::CONTROL_HEIGHT))
                        .fill(Theme::PRIMARY)
                        .stroke(Stroke::new(1.0, Theme::STROKE))
                        .corner_radius(CornerRadius::same(Theme::R_SM));
                    if ui.add(upload_btn).clicked() {
                        on_upload(index);
                    }
                    
                    ui.add_space(4.0);
                    
                    let del_btn = egui::Button::new(RichText::new("삭제").size(Theme::FONT_SMALL))
                        .min_size(egui::vec2(60.0, Theme::CONTROL_HEIGHT))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0, Theme::STROKE))
                        .corner_radius(CornerRadius::same(Theme::R_SM));
                    if ui.add(del_btn).clicked() {
                        on_delete(index);
                    }
                });
            });
            if !c.upload_status.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{} {}", c.upload_status, c.upload_message))
                        .size(Theme::FONT_SMALL)
                        .color(Theme::TEXT_ACCENT),
                );
            }
        });
}

fn badge(ui: &mut egui::Ui, text: &str, bg: Color32, text_color: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(text_color).size(Theme::FONT_TINY).strong());
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}
