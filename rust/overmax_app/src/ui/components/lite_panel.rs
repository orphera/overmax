use crate::ui::components::{FadeClippedLabel, ModeBadge, OverlayHeaderDetail, StatusLamp};
use crate::ui::overlay_theme::Theme;
use crate::ui::overlay_ui::{diff_color, OverlayActions, OverlayProps, Px, LITE_BASE_HEIGHT};
use crate::ui::ui_command::UiCommand;
use eframe::egui::{
    self, Align, Button, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, Vec2,
};
use std::sync::atomic::Ordering;

pub struct LitePanel;

impl LitePanel {
    pub(crate) fn show(ui: &mut egui::Ui, props: &OverlayProps) -> OverlayActions {
        let px = Px::new(props.scale);
        let mut actions = OverlayActions::default();

        // 라이트모드 2열 켜짐/꺼짐 시 콘텐츠 크기 변동에 의해 창 높이가 출렁이는 현상을 방지하기 위해 높이를 강제 고정
        let target_height = LITE_BASE_HEIGHT * props.scale;
        ui.set_min_height(target_height);
        ui.set_max_height(target_height);

        let response = Frame::new()
            .fill(Theme::PANEL_BG)
            .corner_radius(CornerRadius::same((8.0 * props.scale) as u8))
            .inner_margin(Margin::symmetric(
                (10.0 * props.scale) as i8,
                (6.0 * props.scale) as i8,
            ))
            .stroke(egui::Stroke::new(1.0_f32, Theme::PANEL_STROKE))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0 * props.scale;

                // 1열: 상태 표시등 + [버튼모드] [난이도] [비공식 난이도] + 곡명 + 업로드 버튼 + 설정 버튼
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0 * props.scale;

                    ui.add(StatusLamp::new(props.state.is_stable).scale(px.scale));

                    if let Some(ctx) = &props.state.context {
                        // 1. Mode 뱃지
                        ui.add(ModeBadge::new(Some(&ctx.mode)).scale(px.scale));

                        // 2. Diff 뱃지
                        let color = diff_color(&ctx.diff);
                        let (d_rect, _) = ui.allocate_exact_size(
                            Vec2::new(px.mode_badge_w(), px.mode_badge_h()),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            d_rect,
                            CornerRadius::same((3.0 * props.scale) as u8),
                            color,
                        );
                        ui.painter().text(
                            d_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &ctx.diff,
                            FontId::proportional(11.0 * props.scale),
                            Theme::TEXT_PRIMARY,
                        );

                        // 3. 비공식 난이도 (또는 공식 레벨)
                        if let Some(pattern) = props.pattern_tabs.iter().find(|p| p.diff == ctx.diff) {
                            if let Some(floor) = &pattern.floor_name {
                                ui.label(
                                    RichText::new(format!("★{}", floor))
                                        .color(Theme::TEXT_ACCENT)
                                        .font(FontId::proportional(12.0 * props.scale))
                                        .strong(),
                                );
                            } else if let Some(level) = pattern.level {
                                ui.label(
                                    RichText::new(format!("Lv.{}", level))
                                        .color(Theme::TEXT_SECONDARY)
                                        .font(FontId::proportional(11.0 * props.scale)),
                                );
                            }
                        }
                    }

                    // 우측 배치: 설정/업로드 버튼만 우측 정렬로 묶음
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().button_padding = Vec2::ZERO;

                        // 설정 버튼: 투명도 없이 불투명 fill을 적용해 버튼 영역 전체가 클릭 이벤트를 정확하게 받게 함
                        let text = RichText::new("⚙")
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(14.0 * props.scale));
                        let btn = Button::new(text)
                            .fill(Theme::SECTION_BG)
                            .corner_radius(CornerRadius::same((5.0 * props.scale) as u8))
                            .wrap();
                        let response = ui
                            .add_sized(
                                Vec2::splat(18.0 * props.scale),
                                btn.sense(Sense::click()),
                            )
                            .on_hover_text("설정");
                        if response.clicked() {
                            props.settings_open.store(true, Ordering::Relaxed);
                            actions.command = Some(UiCommand::OpenSettings);
                        }

                        // 업로드 버튼: 마찬가지로 불투명 fill 적용
                        if props.varchive_upload_needed {
                            ui.add_space(4.0 * props.scale);
                            let upload_text = RichText::new("⬆")
                                .color(if props.varchive_account_configured {
                                    Theme::TEXT_PRIMARY
                                } else {
                                    Theme::TEXT_MUTED
                                })
                                .font(FontId::proportional(11.0 * props.scale));

                            let upload_btn = Button::new(upload_text)
                                .fill(if props.varchive_account_configured {
                                    Theme::PRIMARY
                                } else {
                                    Theme::SECTION_BG
                                })
                                .corner_radius(CornerRadius::same((4.0 * props.scale) as u8))
                                .wrap();

                            let btn_size = Vec2::splat(18.0 * props.scale);
                            let response_upload =
                                ui.add_sized(btn_size, upload_btn.sense(Sense::click()));
                            let response_upload = if props.varchive_account_configured {
                                response_upload.on_hover_text("V-Archive 업로드 필요 (클릭하여 즉시 업로드)")
                            } else {
                                response_upload.on_hover_text("V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)")
                            };

                            if response_upload.clicked() && props.varchive_account_configured {
                                actions.command = Some(UiCommand::UploadCurrentPattern);
                            }
                        }

                        let title_w = ui.available_width();
                        ui.with_layout(
                            Layout::left_to_right(Align::Center).with_main_align(egui::Align::Min),
                            |ui| {
                                ui.set_max_width(title_w);
                                ui.add(
                                    FadeClippedLabel::new(props.song_label)
                                        .font(FontId::proportional(13.0 * props.scale))
                                        .color(Theme::TEXT_PRIMARY)
                                        .max_width(title_w.max(0.0))
                                        .bg_color(Theme::PANEL_BG)
                                        .scale(props.scale),
                                );
                            },
                        );
                    });
                });

                // 2열: Rate + 콤보상태 + sheet_meta 배지 (일반 모드와 동일한 배지 렌더링)
                ui.add(
                    OverlayHeaderDetail::new(props.state, props.pattern_tabs)
                        .is_result(props.state.scene.is_result())
                        .session_initial_record(props.session_initial_record)
                        .scale(props.scale)
                        .toast(props.toast),
                );
            });

        let is_snap_manual = props.is_snap_manual;

        if is_snap_manual {
            // Exclude the right settings/upload buttons area (approx 45px * scale) from the drag target
            let mut drag_rect = response.response.rect;
            drag_rect.max.x -= 45.0 * props.scale;

            let drag_response =
                ui.interact(drag_rect, ui.id().with("lite_overlay_drag"), Sense::drag());
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

        actions.response_rect = Some(response.response.rect);
        actions
    }
}
