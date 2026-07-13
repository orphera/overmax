//! V-Archive sync window: list candidates and trigger scan / upload.

use crate::ui::overlay_theme::{apply_secondary_window_style, Theme};
use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use overmax_data::SyncCandidate;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SyncProps<'a, F1, F2, F3>
where
    F1: Fn(),
    F2: Fn(usize) + Copy,
    F3: Fn(usize) + Copy,
{
    pub steam_id: &'a mut String,
    pub status: &'a str,
    pub candidates: &'a [SyncCandidate],
    pub steam_users: &'a std::collections::HashMap<String, crate::system::steam_session::SteamUser>,
    pub on_scan: F1,
    pub on_upload: F2,
    pub on_delete: F3,
}

pub fn render_sync<F1, F2, F3>(
    ctx: &egui::Context,
    class: ViewportClass,
    props: SyncProps<F1, F2, F3>,
) where
    F1: Fn(),
    F2: Fn(usize) + Copy,
    F3: Fn(usize) + Copy,
{
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
                    let mut label_text = "Steam ID".to_string();
                    if let Some(user) = props.steam_users.get(props.steam_id) {
                        if !user.persona_name.is_empty() {
                            label_text = format!("{} ({})", user.persona_name, user.account_name);
                        }
                    }

                    ui.add_sized(
                        egui::vec2(160.0, Theme::CONTROL_HEIGHT),
                        egui::Label::new(
                            RichText::new(label_text)
                                .color(Theme::TEXT_PRIMARY)
                                .size(Theme::FONT_BODY),
                        )
                        .truncate(),
                    );
                    ui.add_space(8.0);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let scan_btn = egui::Button::new(
                            RichText::new("스캔").size(Theme::FONT_BODY).strong(),
                        )
                        .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
                        .fill(Theme::PRIMARY)
                        .corner_radius(egui::CornerRadius::same(Theme::R_SM));

                        if ui.add(scan_btn).clicked() {
                            (props.on_scan)();
                        }

                        ui.add_space(8.0);

                        ui.add(
                            egui::TextEdit::singleline(props.steam_id)
                                .font(egui::FontId::proportional(Theme::FONT_BODY))
                                .vertical_align(egui::Align::Center)
                                .margin(egui::Margin::symmetric(8, 0))
                                .desired_width(ui.available_width())
                                .min_size(egui::vec2(0.0, Theme::CONTROL_HEIGHT)),
                        );
                    });
                });
                if !props.status.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(props.status)
                            .size(Theme::FONT_SMALL)
                            .color(Theme::TEXT_MUTED),
                    );
                }
            });

        let sort_mode_id = ui.make_persistent_id("sync_sort_mode");
        let mut sort_mode =
            ui.data_mut(|d| d.get_temp::<SyncSortMode>(sort_mode_id).unwrap_or_default());

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
                RichText::new(format!("{}", props.candidates.len()))
                    .color(Theme::TEXT_ACCENT)
                    .size(Theme::FONT_BODY)
                    .strong(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let diff_btn_fill = if sort_mode == SyncSortMode::RateDiff {
                    Theme::PRIMARY
                } else {
                    Theme::SECONDARY
                };
                let diff_btn = egui::Button::new(RichText::new("변경순").size(Theme::FONT_SMALL))
                    .fill(diff_btn_fill)
                    .corner_radius(CornerRadius::same(Theme::R_SM));
                if ui.add(diff_btn).clicked() {
                    sort_mode = SyncSortMode::RateDiff;
                    ui.data_mut(|d| d.insert_temp(sort_mode_id, sort_mode));
                }

                ui.add_space(4.0);

                let title_btn_fill = if sort_mode == SyncSortMode::Title {
                    Theme::PRIMARY
                } else {
                    Theme::SECONDARY
                };
                let title_btn = egui::Button::new(RichText::new("제목순").size(Theme::FONT_SMALL))
                    .fill(title_btn_fill)
                    .corner_radius(CornerRadius::same(Theme::R_SM));
                if ui.add(title_btn).clicked() {
                    sort_mode = SyncSortMode::Title;
                    ui.data_mut(|d| d.insert_temp(sort_mode_id, sort_mode));
                }
            });
        });
        ui.add_space(12.0);

        let mut sorted_candidates: Vec<(usize, &SyncCandidate)> =
            props.candidates.iter().enumerate().collect();

        match sort_mode {
            SyncSortMode::Title => {
                sorted_candidates.sort_by(|a, b| {
                    let mode_cmp = a.1.button_mode.cmp(&b.1.button_mode);
                    if mode_cmp != std::cmp::Ordering::Equal {
                        return mode_cmp;
                    }
                    a.1.song_name.cmp(&b.1.song_name)
                });
            }
            SyncSortMode::RateDiff => {
                sorted_candidates.sort_by(|a, b| {
                    let diff_a = match a.1.varchive_rate {
                        None => a.1.overmax_rate,
                        Some(vr) => a.1.overmax_rate - vr,
                    };
                    let diff_b = match b.1.varchive_rate {
                        None => b.1.overmax_rate,
                        Some(vr) => b.1.overmax_rate - vr,
                    };
                    diff_b
                        .partial_cmp(&diff_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().spacing.item_spacing.y = 12.0;
                for (orig_idx, c) in sorted_candidates {
                    candidate_row(ui, orig_idx, c, props.on_upload, props.on_delete);
                }
            });
    };

    if class == ViewportClass::Embedded {
        egui::Window::new("V-Archive 동기화").show(ctx, |ui| body(ui));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Theme::PANEL_BG)
                    .inner_margin(Margin::same(24)),
            )
            .show(ctx, |ui| body(ui));
    }
}

fn candidate_row<F: Fn(usize), D: Fn(usize)>(
    ui: &mut egui::Ui,
    index: usize,
    c: &SyncCandidate,
    on_upload: F,
    on_delete: D,
) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&c.song_name)
                            .color(Theme::TEXT_PRIMARY)
                            .size(Theme::FONT_BODY)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        // Button Mode Badge
                        let mode_color =
                            crate::ui::components::ModeBadge::mode_color(&c.button_mode);
                        badge(ui, &c.button_mode, mode_color, Theme::TEXT_PRIMARY);
                        ui.add_space(4.0);
                        // Difficulty Badge
                        let diff_color = crate::ui::overlay_ui::diff_color(&c.difficulty);
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
                    let upload_btn =
                        egui::Button::new(RichText::new("등록").size(Theme::FONT_SMALL).strong())
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
            ui.label(
                RichText::new(text)
                    .color(text_color)
                    .size(Theme::FONT_TINY)
                    .strong(),
            );
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum SyncSortMode {
    #[default]
    Title,
    RateDiff,
}
