use crate::ui::components::{
    FadeClippedLabel, ModeBadge, OverlayHeaderDetail, StatusLamp, ToastMessage,
};
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_theme::Theme;
use crate::ui::overlay_ui::{OverlayActions, Px};
use crate::ui::ui_command::UiCommand;
use eframe::egui::{
    self, Align, Button, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Vec2,
};
use overmax_core::{GameSessionState, RecordValue};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct OverlayHeader<'a> {
    state: &'a GameSessionState,
    song_label: &'a str,
    pattern_tabs: &'a [PatternTabInfo],
    settings_open: &'a Arc<AtomicBool>,
    px: &'a Px,
    varchive_upload_needed: bool,
    varchive_account_configured: bool,
    is_snap_manual: bool,
    session_initial_record: Option<RecordValue>,
    toast: Option<&'a ToastMessage>,
}

impl<'a> OverlayHeader<'a> {
    pub(crate) fn new(
        state: &'a GameSessionState,
        song_label: &'a str,
        pattern_tabs: &'a [PatternTabInfo],
        settings_open: &'a Arc<AtomicBool>,
        px: &'a Px,
    ) -> Self {
        Self {
            state,
            song_label,
            pattern_tabs,
            settings_open,
            px,
            varchive_upload_needed: false,
            varchive_account_configured: false,
            is_snap_manual: false,
            session_initial_record: None,
            toast: None,
        }
    }

    pub(crate) fn varchive_upload_needed(mut self, needed: bool) -> Self {
        self.varchive_upload_needed = needed;
        self
    }

    pub(crate) fn varchive_account_configured(mut self, configured: bool) -> Self {
        self.varchive_account_configured = configured;
        self
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn is_snap_manual(mut self, is_snap_manual: bool) -> Self {
        self.is_snap_manual = is_snap_manual;
        self
    }

    pub(crate) fn session_initial_record(mut self, record: Option<RecordValue>) -> Self {
        self.session_initial_record = record;
        self
    }

    pub(crate) fn toast(mut self, toast: Option<&'a ToastMessage>) -> Self {
        self.toast = toast;
        self
    }

    pub(crate) fn show(self, ui: &mut egui::Ui, actions: &mut OverlayActions) {
        let mut buttons_left_x = None;
        let header = Frame::new()
            .fill(Theme::HEADER_BG)
            .corner_radius(CornerRadius::same(self.px.header_radius() as u8))
            .inner_margin(Margin::symmetric(
                self.px.header_margin_x() as i8,
                self.px.header_margin_y() as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = self.px.header_row_gap();
                    ui.add(StatusLamp::new(self.state.is_stable).scale(self.px.scale));
                    ui.add(
                        ModeBadge::new(self.state.context.as_ref().map(|ctx| ctx.mode.as_str()))
                            .scale(self.px.scale),
                    );

                    let right_w = if self.varchive_upload_needed {
                        (24.0 + 18.0 + 4.0) * self.px.scale
                    } else {
                        24.0 * self.px.scale
                    };
                    let spacing = ui.spacing().item_spacing.x;
                    let max_w = ui.available_width() - right_w - spacing * 2.0 - 4.0 * self.px.scale;

                    ui.add(
                        FadeClippedLabel::new(self.song_label)
                            .font(FontId::proportional(14.0 * self.px.scale))
                            .color(Theme::TEXT_PRIMARY)
                            .max_width(max_w.max(0.0))
                            .bg_color(Theme::HEADER_BG)
                            .scale(self.px.scale),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().button_padding = Vec2::ZERO;
                        let text = RichText::new("⚙")
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(15.0 * self.px.scale));
                        let btn = Button::new(text)
                            .fill(Theme::SECTION_BG)
                            .corner_radius(CornerRadius::same((6.0 * self.px.scale) as u8))
                            .wrap();
                        let response = ui
                            .add_sized(
                                Vec2::splat(self.px.settings_btn()),
                                btn.sense(Sense::click()),
                            )
                            .on_hover_text("설정");
                        buttons_left_x = Some(response.rect.min.x);
                        if response.clicked() {
                            self.settings_open.store(true, Ordering::Relaxed);
                            actions.command = Some(UiCommand::OpenSettings);
                        }

                        if self.varchive_upload_needed {
                            ui.add_space(4.0 * self.px.scale);
                            let upload_text = RichText::new("⬆").color(
                                if self.varchive_account_configured {
                                    Theme::TEXT_PRIMARY
                                } else {
                                    Theme::TEXT_MUTED
                                },
                            ).font(FontId::proportional(11.0 * self.px.scale));

                            let upload_btn = Button::new(upload_text)
                                .fill(if self.varchive_account_configured {
                                    Theme::PRIMARY
                                } else {
                                    Theme::SECTION_BG
                                })
                                .corner_radius(CornerRadius::same((4.0 * self.px.scale) as u8))
                                .wrap();

                            let btn_size = Vec2::splat(18.0 * self.px.scale);
                            let response = ui.add_sized(btn_size, upload_btn.sense(Sense::click()));
                            let response = if self.varchive_account_configured {
                                response.on_hover_text("V-Archive 업로드 필요 (클릭하여 즉시 업로드)")
                            } else {
                                response
                                    .on_hover_text("V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)")
                            };
                            buttons_left_x = Some(
                                buttons_left_x
                                    .map(|x| x.min(response.rect.min.x))
                                    .unwrap_or(response.rect.min.x),
                            );

                            if response.clicked() && self.varchive_account_configured {
                                actions.command = Some(UiCommand::UploadCurrentPattern);
                            }
                        }
                    });
                });
                ui.add_space(self.px.header_meta_gap());
                let scale = self.px.scale;
                let second_row_height = 15.0 * scale;
                ui.add(
                    OverlayHeaderDetail::new(self.state, self.pattern_tabs)
                        .is_result(self.state.scene.is_result())
                        .session_initial_record(self.session_initial_record)
                        .scale(scale)
                        .height(second_row_height)
                        .toast(self.toast),
                );
            });

        if self.is_snap_manual {
            let drag_rect = drag_rect_excluding_buttons(header.response.rect, buttons_left_x);
            let drag_response = ui.interact(
                drag_rect,
                ui.id().with("overlay_header_drag"),
                Sense::drag(),
            );
            if drag_response.drag_started() {
                actions.start_drag = true;
            }
            if drag_response.dragged() {
                actions.drag_delta = Some(drag_response.drag_delta());
            }
            if drag_response.drag_stopped() {
                actions.restore_game_focus = true;
            }
        }
    }
}

fn drag_rect_excluding_buttons(header: Rect, buttons_left_x: Option<f32>) -> Rect {
    let Some(left_x) = buttons_left_x else {
        return header;
    };
    let mut rect = header;
    rect.max.x = (left_x - 4.0).max(rect.min.x);
    rect
}
