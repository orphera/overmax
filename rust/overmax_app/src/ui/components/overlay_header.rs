use crate::ui::components::{
    FadeClippedLabel, ModeBadge, OverlayHeaderDetail, StatusLamp, ToastMessage, UploadFillSlots,
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
    sync_open: Option<&'a Arc<AtomicBool>>,
    upload_history_button: bool,
    px: &'a Px,
    varchive_upload_needed: bool,
    varchive_upload_progress: Option<f32>,
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
            sync_open: None,
            upload_history_button: false,
            px,
            varchive_upload_needed: false,
            varchive_upload_progress: None,
            varchive_account_configured: false,
            is_snap_manual: false,
            session_initial_record: None,
            toast: None,
        }
    }

    pub(crate) fn sync_open(mut self, sync_open: &'a Arc<AtomicBool>) -> Self {
        self.sync_open = Some(sync_open);
        self
    }

    /// 업로드 기록 창 여는 버튼 표시 여부 (메인 오버레이 헤더에서만 켠다).
    pub(crate) fn upload_history_button(mut self) -> Self {
        self.upload_history_button = true;
        self
    }

    pub(crate) fn varchive_upload_needed(mut self, needed: bool) -> Self {
        self.varchive_upload_needed = needed;
        self
    }

    pub(crate) fn varchive_upload_progress(mut self, progress: Option<f32>) -> Self {
        self.varchive_upload_progress = progress;
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
                        ModeBadge::new(self.state.context.as_ref().map(|ctx| ctx.mode))
                            .scale(self.px.scale),
                    );

                    let buttons = HeaderButtons {
                        settings_open: self.settings_open,
                        sync_open: self.sync_open,
                        upload_history: self.upload_history_button,
                        upload_needed: self.varchive_upload_needed,
                        upload_progress: self.varchive_upload_progress,
                        account_configured: self.varchive_account_configured,
                        scale: self.px.scale,
                    };
                    let spacing = ui.spacing().item_spacing.x;
                    let max_w = ui.available_width()
                        - buttons.width()
                        - spacing * 2.0
                        - 4.0 * self.px.scale;

                    ui.add(
                        FadeClippedLabel::new(self.song_label)
                            .font(FontId::proportional(14.0 * self.px.scale))
                            .color(Theme::TEXT_PRIMARY)
                            .max_width(max_w.max(0.0))
                            .bg_color(Theme::HEADER_BG)
                            .scale(self.px.scale),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        buttons_left_x = buttons.show(ui, actions);
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

/// 헤더 오른쪽 버튼 묶음 (⚙ 설정, 🔄 동기화, 📋 업로드 기록, ⬆ 업로드).
/// 일반 헤더와 라이트 헤더가 같은 크기·간격·모양으로 그리도록 공유한다.
pub(crate) struct HeaderButtons<'a> {
    pub settings_open: &'a Arc<AtomicBool>,
    pub sync_open: Option<&'a Arc<AtomicBool>>,
    pub upload_history: bool,
    pub upload_needed: bool,
    pub upload_progress: Option<f32>,
    pub account_configured: bool,
    pub scale: f32,
}

impl HeaderButtons<'_> {
    fn btn_size(&self) -> f32 {
        Px::new(self.scale).header_btn_size()
    }

    fn btn_gap(&self) -> f32 {
        2.5 * self.scale
    }

    fn count(&self) -> usize {
        1 + usize::from(self.sync_open.is_some())
            + usize::from(self.upload_history)
            + usize::from(self.upload_needed)
    }

    /// 버튼 묶음 전체 폭
    pub(crate) fn width(&self) -> f32 {
        let n = self.count() as f32;
        n * self.btn_size() + (n - 1.0) * self.btn_gap()
    }

    /// 오른쪽부터 버튼을 그리고, 그려진 버튼 묶음의 왼쪽 끝 x를 돌려준다.
    /// `right_to_left` 레이아웃 안에서 호출해야 한다.
    pub(crate) fn show(&self, ui: &mut egui::Ui, actions: &mut OverlayActions) -> Option<f32> {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = self.btn_gap();
            ui.spacing_mut().button_padding = Vec2::ZERO;
            let btn_size = Vec2::splat(self.btn_size());
            let btn_radius = CornerRadius::same((3.5 * self.scale) as u8);
            let text = RichText::new("⚙")
                .color(Theme::TEXT_PRIMARY)
                .font(FontId::proportional(11.0 * self.scale));
            let btn = Button::new(text)
                .fill(Theme::SECTION_BG)
                .corner_radius(btn_radius)
                .wrap();
            let response = ui
                .add_sized(btn_size, btn.sense(Sense::click()))
                .on_hover_text(crate::t!("settings-title"));
            let mut buttons_left_x = Some(response.rect.min.x);
            if response.clicked() {
                self.settings_open.store(true, Ordering::Relaxed);
                actions.command = Some(UiCommand::OpenSettings);
            }

            if let Some(sync_open) = self.sync_open {
                let sync_text = RichText::new("🔄")
                    .color(Theme::TEXT_PRIMARY)
                    .font(FontId::proportional(10.0 * self.scale));
                let sync_btn = Button::new(sync_text)
                    .fill(Theme::SECTION_BG)
                    .corner_radius(btn_radius)
                    .wrap();
                let response = ui
                    .add_sized(btn_size, sync_btn.sense(Sense::click()))
                    .on_hover_text(crate::t!("sync-title"));
                buttons_left_x = Some(
                    buttons_left_x
                        .map(|x| x.min(response.rect.min.x))
                        .unwrap_or(response.rect.min.x),
                );
                if response.clicked() {
                    sync_open.store(true, Ordering::Relaxed);
                    actions.command = Some(UiCommand::OpenSync);
                }
            }

            if self.upload_history {
                let history_text = RichText::new("📋")
                    .color(Theme::TEXT_PRIMARY)
                    .font(FontId::proportional(10.0 * self.scale));
                let history_btn = Button::new(history_text)
                    .fill(Theme::SECTION_BG)
                    .corner_radius(btn_radius)
                    .wrap();
                let response = ui
                    .add_sized(btn_size, history_btn.sense(Sense::click()))
                    .on_hover_text(crate::t!("upload-history-title"));
                buttons_left_x = Some(
                    buttons_left_x
                        .map(|x| x.min(response.rect.min.x))
                        .unwrap_or(response.rect.min.x),
                );
                if response.clicked() {
                    actions.command = Some(UiCommand::OpenUploadHistory);
                }
            }

            if self.upload_needed {
                let upload_text = RichText::new("⬆")
                    .color(if self.account_configured {
                        Theme::TEXT_PRIMARY
                    } else {
                        Theme::TEXT_MUTED
                    })
                    .font(FontId::proportional(10.5 * self.scale));

                let progress = self.upload_progress;
                let fill_slots = progress.map(|_| UploadFillSlots::reserve(ui));
                let upload_btn = Button::new(upload_text)
                    .fill(if fill_slots.is_some() {
                        egui::Color32::TRANSPARENT
                    } else if self.account_configured {
                        Theme::PRIMARY
                    } else {
                        Theme::SECTION_BG
                    })
                    .corner_radius(btn_radius)
                    .wrap();

                let response = ui.add_sized(btn_size, upload_btn.sense(Sense::click()));
                if let (Some(slots), Some(p)) = (fill_slots, progress) {
                    slots.paint(ui, response.rect, btn_radius, p);
                }
                let response = if self.account_configured {
                    response.on_hover_text(crate::t!("overlay-varchive-upload-needed"))
                } else {
                    response.on_hover_text(crate::t!("overlay-varchive-link-needed"))
                };
                buttons_left_x = Some(
                    buttons_left_x
                        .map(|x| x.min(response.rect.min.x))
                        .unwrap_or(response.rect.min.x),
                );

                if response.clicked() && self.account_configured {
                    actions.command = Some(UiCommand::UploadCurrentPattern);
                }
            }
            buttons_left_x
        })
        .inner
    }
}

pub(crate) fn drag_rect_excluding_buttons(header: Rect, buttons_left_x: Option<f32>) -> Rect {
    let Some(left_x) = buttons_left_x else {
        return header;
    };
    let mut rect = header;
    rect.max.x = (left_x - 4.0).max(rect.min.x);
    rect
}
