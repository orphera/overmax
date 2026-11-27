//! 플레이어용 업로드 기록 창. 디버그 로그와 별개로 이번 실행 중 업로드 결과를 최신순으로 보여준다.
//! 메인 오버레이와 같은 헤더 룩(Theme, 같은 버튼 크기·라운드)을 쓰며, 제목표시줄 없이(프레임리스)
//! 헤더에서 드래그하고 헤더의 ✕로 닫는다.

use crate::ui::overlay_theme::Theme;
use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, TextWrapMode,
};
use overmax_data::{UploadHistoryRecord, UPLOAD_HISTORY_LIMIT};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const WINDOW_SIZE: [f32; 2] = [360.0, 480.0];

/// 현재 시각 (Unix 초). 시계가 뒤로 가도 0을 반환해 패닉하지 않는다.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 업로드 기록 목록의 메모리 사본. DB(`record.db`)가 원본이고, 여기서는 창 표시용으로만 든다.
/// 최신 항목이 앞에 온다.
#[derive(Clone, Default)]
pub struct SharedUploadHistory {
    entries: Arc<Mutex<VecDeque<UploadHistoryRecord>>>,
}

impl SharedUploadHistory {
    pub fn push(&self, record: UploadHistoryRecord) {
        let mut guard = overmax_core::lock_or_recover(&self.entries);
        guard.push_front(record);
        guard.truncate(UPLOAD_HISTORY_LIMIT);
    }

    /// DB에서 다시 읽어 전체를 교체한다 (앱 기동 시, 사용자가 바뀔 때).
    pub fn replace(&self, records: Vec<UploadHistoryRecord>) {
        let mut guard = overmax_core::lock_or_recover(&self.entries);
        *guard = VecDeque::from(records);
    }

    pub fn snapshot(&self) -> Vec<UploadHistoryRecord> {
        Vec::from(overmax_core::lock_clone_or_default(&self.entries))
    }
}

/// 기록 시각을 로컬 시간의 `MM-DD HH:MM`으로 표시한다 (기록은 최근 것만 남으므로 연도는 생략).
/// 로컬 오프셋을 못 얻으면 UTC로 표시한다.
fn format_timestamp(unix_secs: i64, offset: time::UtcOffset) -> String {
    let utc = time::OffsetDateTime::from_unix_timestamp(unix_secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local = utc.to_offset(offset);
    format!(
        "{:02}-{:02} {:02}:{:02}",
        u8::from(local.month()),
        local.day(),
        local.hour(),
        local.minute()
    )
}

/// Esc 또는 OS 닫기 요청 시 창을 닫는다.
pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

/// 창 본문. 창 전체를 패널 색으로 채운 뒤 헤더(제목 드래그 · ✕ 닫기)와 기록 카드 목록을 그린다.
/// 반환값은 이번 프레임에 헤더를 드래그 중인지 여부이다 (창 이동은 호출 쪽에서 처리한다).
pub fn render_upload_history(
    ui: &mut egui::Ui,
    entries: &[UploadHistoryRecord],
    open: &Arc<AtomicBool>,
    scale: f32,
) -> bool {
    // 내용이 짧아도 창 바닥이 검게 남지 않도록 창 전체 영역을 먼저 채운다.
    let window_rect = ui.max_rect();
    ui.painter().rect_filled(
        window_rect,
        CornerRadius::same((14.0 * scale) as u8),
        Theme::PANEL_BG,
    );

    // 헤더는 창 제목표시줄처럼 바깥 여백 없이 창 상단에 붙인다.
    ui.spacing_mut().item_spacing.y = 0.0;
    let dragging = render_header(ui, open, scale);
    Frame::new()
        .inner_margin(Margin::same((8.0 * scale) as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0 * scale;

            if entries.is_empty() {
                ui.label(
                    RichText::new(crate::t!("upload-history-empty"))
                        .color(Theme::TEXT_MUTED)
                        .font(FontId::proportional(12.0 * scale)),
                );
                return;
            }

            // 로컬 오프셋은 프레임당 한 번만 구한다 (행마다 OS를 조회하지 않도록).
            let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
            // 스크롤바가 오른쪽 폭을 따로 차지하면 카드 좌우 여백이 어긋나므로 내용 위에 띄운다.
            ui.spacing_mut().scroll = egui::style::ScrollStyle::floating();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in entries {
                        draw_row(ui, entry, offset, scale);
                        ui.add_space(6.0 * scale);
                    }
                });
        });
    dragging
}

/// 헤더: 메인 오버레이 헤더와 같은 색·버튼 크기를 쓰고, 창 상단에 붙는 제목표시줄 모양으로 그린다. 제목 영역 전체가 드래그 핸들이고,
/// 오른쪽 ✕ 버튼만 닫기 동작을 가진다 (드래그 영역은 버튼을 덮지 않도록 버튼 왼쪽까지만 잡는다).
fn render_header(ui: &mut egui::Ui, open: &Arc<AtomicBool>, scale: f32) -> bool {
    let button_size = 18.0 * scale;
    // 위쪽 모서리는 창 라운드와 같게, 아래쪽은 본문과 맞닿도록 각지게 둔다.
    let top = (14.0 * scale) as u8;
    Frame::new()
        .fill(Theme::HEADER_BG)
        .corner_radius(CornerRadius {
            nw: top,
            ne: top,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(
            (14.0 * scale) as i8,
            (10.0 * scale) as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let drag_width = (ui.available_width() - button_size).max(0.0);
                let (drag_rect, drag) =
                    ui.allocate_exact_size(egui::vec2(drag_width, button_size), Sense::drag());
                ui.painter().text(
                    drag_rect.left_center(),
                    Align2::LEFT_CENTER,
                    crate::t!("upload-history-title").to_string(),
                    FontId::proportional(14.0 * scale),
                    Theme::TEXT_PRIMARY,
                );
                let dragging = drag.dragged();

                let close = egui::Button::new(
                    RichText::new("✕")
                        .color(Theme::TEXT_PRIMARY)
                        .font(FontId::proportional(10.0 * scale)),
                )
                .fill(Theme::SECTION_BG)
                .corner_radius(CornerRadius::same((3.5 * scale) as u8))
                .wrap();
                if ui
                    .add_sized([button_size, button_size], close.sense(Sense::click()))
                    .on_hover_text(crate::t!("upload-history-close"))
                    .clicked()
                {
                    open.store(false, Ordering::Relaxed);
                }
                dragging
            })
            .inner
        })
        .inner
}

/// TOP N 진입 배지: 어두운 알약 위에 강조색(TEXT_ACCENT) 글자.
fn badge(ui: &mut egui::Ui, text: &str, scale: f32) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .corner_radius(CornerRadius::same((4.0 * scale) as u8))
        .inner_margin(Margin::symmetric((5.0 * scale) as i8, (1.0 * scale) as i8))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .color(Theme::TEXT_ACCENT)
                        .font(FontId::proportional(9.5 * scale))
                        .strong(),
                )
                .wrap_mode(TextWrapMode::Extend),
            );
        });
}

fn draw_row(ui: &mut egui::Ui, entry: &UploadHistoryRecord, offset: time::UtcOffset, scale: f32) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same((8.0 * scale) as u8))
        .inner_margin(Margin::symmetric((10.0 * scale) as i8, (6.0 * scale) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 성공은 기본 상태라 따로 표시하지 않고, 실패만 경고색으로 드러낸다. 상세 메시지는 행 툴팁.
            ui.response().on_hover_text(entry.message.as_str());
            // 오른쪽 시각 칸의 폭을 먼저 고정하고, 왼쪽 정보는 남은 폭 안에서만 줄바꿈한다.
            let right_w = (80.0 * scale).min(ui.available_width() * 0.4);
            let left_w = (ui.available_width() - right_w - ui.spacing().item_spacing.x).max(0.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        // 내용 폭으로 줄어들면 오른쪽 칸 시작 위치가 행마다 달라지므로 폭을 고정한다.
                        ui.set_width(left_w);
                        // 곡명이 길면 잘라서 오른쪽 칸을 밀어내지 않는다.
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.song_name)
                                    .color(Theme::TEXT_PRIMARY)
                                    .font(FontId::proportional(13.0 * scale))
                                    .strong(),
                            )
                            .truncate(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&entry.pattern)
                                    .color(Theme::TEXT_SECONDARY)
                                    .font(FontId::proportional(11.0 * scale)),
                            );
                            ui.spacing_mut().item_spacing.x = 5.0 * scale;
                            ui.label(
                                RichText::new(format!("{:.2}%", entry.rate))
                                    .color(Theme::OK)
                                    .font(FontId::proportional(11.0 * scale))
                                    .strong(),
                            );
                            // 메인 오버레이 헤더와 같은 표기: Rate 바로 뒤에 P(퍼펙트) / M(맥스 콤보).
                            // 이번에 새로 단 경우만 강조색, 원래 달려 있던 경우는 헤더의 이전 기록처럼 흐린 색.
                            if let Some(sym) = entry.max_combo_symbol() {
                                let color = if entry.is_new_max_combo() {
                                    Theme::TEXT_ACCENT
                                } else {
                                    Theme::TEXT_SECONDARY
                                };
                                ui.label(
                                    RichText::new(sym)
                                        .color(color)
                                        .font(FontId::proportional(11.0 * scale))
                                        .strong(),
                                );
                            }
                            // 상승폭도 헤더와 같은 색(RED)으로, 헤더처럼 실제로 올랐을 때만 표시한다.
                            if let Some(delta) = entry.visible_rate_delta() {
                                ui.label(
                                    RichText::new(format!("{delta:+.2}%"))
                                        .color(Theme::RED)
                                        .font(FontId::proportional(11.0 * scale)),
                                );
                            }
                            if let Some(rank) = entry.top50_rank {
                                badge(ui, &format!("TOP {rank}"), scale);
                            }
                        });
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(right_w, 0.0),
                    Layout::top_down(Align::Max),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format_timestamp(entry.uploaded_at, offset))
                                    .color(Theme::TEXT_MUTED)
                                    .font(FontId::proportional(10.5 * scale)),
                            )
                            .wrap_mode(TextWrapMode::Extend),
                        );
                        if !entry.success {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(crate::t!("upload-history-result-failed"))
                                        .color(Theme::WARN)
                                        .font(FontId::proportional(11.0 * scale))
                                        .strong(),
                                )
                                .wrap_mode(TextWrapMode::Extend),
                            );
                        }
                    },
                );
            });
        });
}
