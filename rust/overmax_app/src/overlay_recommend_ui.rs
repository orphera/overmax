use crate::overlay_theme::Theme;
use crate::overlay_ui::diff_color;
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Label, Layout, Margin, Rect, RichText, Vec2,
};
use overmax_core::GameSessionState;
use overmax_data::{RecommendEntry, RecommendResult};

const TAB_WIDTH: f32 = 52.0;
const TAB_HEIGHT: f32 = 46.0;
const TAB_GAP: f32 = 4.0;
const TAB_PAD_Y: f32 = 6.0;
const RECOMMEND_WIDTH: f32 = 286.0;
const RECOMMEND_PAD_Y: f32 = 6.0;
const RECOMMEND_ROW_HEIGHT: f32 = 30.0;
const RECOMMEND_ROW_MARGIN_X: f32 = 8.0;
const BADGE_HEIGHT: f32 = 18.0;
const SONG_LABEL_WIDTH: f32 = 140.0;
const RECOMMEND_ROW_GAP: f32 = 3.2;
pub(crate) const RECOMMEND_BODY_HEIGHT: f32 =
    RECOMMEND_PAD_Y * 2.0 + RECOMMEND_ROW_HEIGHT * 6.0 + RECOMMEND_ROW_GAP * 5.0;

#[derive(Clone, Debug, PartialEq)]
pub struct PatternTabInfo {
    pub diff: String,
    pub level: Option<u32>,
    pub floor_name: Option<String>,
    pub gold: String,
    pub note: String,
    pub assist_key: String,
}

pub fn draw_diff_tabs(
    ui: &mut egui::Ui,
    active: Option<&str>,
    patterns: &[PatternTabInfo],
    scale: f32,
) {
    ui.set_width(TAB_WIDTH * scale);
    ui.vertical(|ui| {
        ui.add_space(TAB_PAD_Y * scale);
        ui.spacing_mut().item_spacing.y = TAB_GAP * scale;
        for diff in ["NM", "HD", "MX", "SC"] {
            draw_diff_tab(ui, diff, active, patterns, scale);
        }
        ui.add_space(TAB_PAD_Y * scale);
    });
}

pub fn draw_recommendations(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    recommendations: &RecommendResult,
    scale: f32,
) {
    ui.vertical(|ui| {
        ui.set_width(RECOMMEND_WIDTH * scale);
        ui.add_space(RECOMMEND_PAD_Y * scale);
        draw_recommend_content(ui, state, recommendations, scale);
        ui.add_space(RECOMMEND_PAD_Y * scale);
    });
}

pub fn avg_rate_text(result: &RecommendResult) -> String {
    format!("{:.2}%", f64::max(result.avg_rate, 0.0))
}

pub fn pattern_count_text(result: &RecommendResult) -> String {
    format!("{}/{}개 패턴", result.has_record_count, result.total_count)
}

fn draw_diff_tab(
    ui: &mut egui::Ui,
    diff: &str,
    active: Option<&str>,
    patterns: &[PatternTabInfo],
    scale: f32,
) {
    let pattern = patterns.iter().find(|item| item.diff == diff);
    let exists = pattern.is_some();
    Frame::new()
        .fill(tab_fill(active == Some(diff), exists))
        .corner_radius(CornerRadius::same((6.0 * scale) as u8))
        .inner_margin(Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(TAB_WIDTH * scale, TAB_HEIGHT * scale));
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(6.0 * scale);
                ui.add(diff_label(diff, scale));
                ui.add(pattern_floor_label(pattern, active == Some(diff), exists, scale));
            });
        });
}

fn draw_recommend_content(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    recommendations: &RecommendResult,
    scale: f32,
) {
    if state.context.is_none() {
        draw_empty_recommend(ui, "패턴을 감지하는 중...", scale);
    } else if recommendations.entries.is_empty() {
        draw_empty_recommend(ui, "추천 결과 없음", scale);
    } else {
        ui.spacing_mut().item_spacing.y = RECOMMEND_ROW_GAP * scale;
        for entry in recommendations.entries.iter().take(6) {
            draw_recommend_row(ui, entry, scale);
        }
    }
}

fn draw_empty_recommend(ui: &mut egui::Ui, text: &str, scale: f32) {
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.set_width(RECOMMEND_WIDTH * scale);
        ui.add_space(RECOMMEND_BODY_HEIGHT / 2.0 * scale - 10.0 * scale);
        ui.add(
            Label::new(
                RichText::new(text)
                    .color(Theme::TEXT_MUTED)
                    .font(FontId::proportional(11.0 * scale)),
            )
            .selectable(false)
            .wrap_mode(egui::TextWrapMode::Extend),
        );
    });
}

fn draw_recommend_row(ui: &mut egui::Ui, entry: &RecommendEntry, scale: f32) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .corner_radius(CornerRadius::same((6.0 * scale) as u8))
        .inner_margin(Margin::symmetric((RECOMMEND_ROW_MARGIN_X * scale) as i8, 0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(
                recommend_row_inner_width() * scale,
                RECOMMEND_ROW_HEIGHT * scale,
            ));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0 * scale;
                draw_entry_badge(ui, entry, scale);
                draw_song_name(ui, entry, scale);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    draw_rate(ui, entry, scale)
                });
            });
        });
}

fn draw_entry_badge(ui: &mut egui::Ui, entry: &RecommendEntry, scale: f32) {
    let text = badge_text(entry);
    let width = if entry.floor_name.is_none() {
        28.0 * scale
    } else {
        36.0 * scale
    };
    let (cell, _) = ui.allocate_exact_size(
        Vec2::new(width, RECOMMEND_ROW_HEIGHT * scale),
        egui::Sense::hover(),
    );
    let rect = centered_badge_rect(cell, width, scale);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same((4.0 * scale) as u8),
        diff_color(&entry.difficulty),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.0 * scale),
        Color32::WHITE,
    );
}

fn draw_song_name(ui: &mut egui::Ui, entry: &RecommendEntry, scale: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(SONG_LABEL_WIDTH * scale, RECOMMEND_ROW_HEIGHT * scale),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add(
                Label::new(song_name_text(entry, scale))
                    .truncate()
                    .selectable(false),
            );
        },
    );
}

fn draw_rate(ui: &mut egui::Ui, entry: &RecommendEntry, scale: f32) {
    let Some(rate) = entry.rate else {
        ui.label(
            RichText::new("——")
                .color(Theme::TEXT_MUTED)
                .font(FontId::proportional(11.0 * scale)),
        );
        return;
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0 * scale;
        ui.add(
            Label::new(
                RichText::new(format!("{rate:.2}%"))
                    .color(rate_color(rate))
                    .font(FontId::proportional(11.0 * scale))
                    .strong(),
            )
            .selectable(false),
        );
        if entry.is_max_combo {
            draw_status_badge(ui, "M", Color32::from_rgb(48, 200, 255), scale);
        }
    });
}

fn draw_status_badge(ui: &mut egui::Ui, text: &str, color: Color32, scale: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0 * scale), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 8.0 * scale, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.0 * scale),
        Color32::WHITE,
    );
}

fn diff_label(diff: &str, scale: f32) -> Label {
    Label::new(
        RichText::new(diff.to_string())
            .color(diff_color(diff))
            .font(FontId::proportional(11.0 * scale))
            .strong(),
    )
}

fn pattern_floor_label(
    pattern: Option<&PatternTabInfo>,
    active: bool,
    exists: bool,
    scale: f32,
) -> Label {
    Label::new(
        RichText::new(pattern_label(pattern))
            .color(pattern_text_color(active, exists))
            .font(FontId::proportional(10.0 * scale))
            .strong(),
    )
}

fn song_name_text(entry: &RecommendEntry, scale: f32) -> RichText {
    RichText::new(&entry.song_name)
        .color(Theme::TEXT_BRIGHT)
        .font(FontId::proportional(11.0 * scale))
        .strong()
}

fn badge_text(entry: &RecommendEntry) -> String {
    if entry.floor_name.is_none() {
        entry.difficulty.clone()
    } else {
        format!("{} {}", entry.difficulty, entry.level.unwrap_or_default())
    }
}

fn tab_fill(active: bool, exists: bool) -> Color32 {
    if !exists {
        Theme::TAB_DIM_BG
    } else if active {
        Theme::TAB_ACTIVE_BG
    } else {
        Theme::TAB_INACTIVE_BG
    }
}

fn pattern_label(pattern: Option<&PatternTabInfo>) -> String {
    let Some(pattern) = pattern else {
        return "—".into();
    };
    pattern
        .floor_name
        .clone()
        .or_else(|| pattern.level.map(|level| format!("Lv{level}")))
        .unwrap_or_else(|| "—".into())
}

fn pattern_text_color(active: bool, exists: bool) -> Color32 {
    if active {
        Theme::TEXT_HINT
    } else if exists {
        Theme::TEXT_SECONDARY
    } else {
        Theme::TEXT_MUTED
    }
}

fn rate_color(rate: f64) -> Color32 {
    if rate >= 100.0 {
        Theme::TEXT_ACCENT
    } else if rate >= 99.8 {
        Theme::OK
    } else if rate >= 99.0 {
        Theme::TEXT_HINT
    } else {
        Theme::TEXT_BRIGHT
    }
}

fn recommend_row_inner_width() -> f32 {
    RECOMMEND_WIDTH - RECOMMEND_ROW_MARGIN_X * 2.0
}

fn centered_badge_rect(cell: Rect, width: f32, scale: f32) -> Rect {
    Rect::from_center_size(cell.center(), Vec2::new(width, BADGE_HEIGHT * scale))
}

#[cfg(test)]
mod tests {
    use super::{
        centered_badge_rect, pattern_label, recommend_row_inner_width, PatternTabInfo,
        BADGE_HEIGHT, RECOMMEND_ROW_HEIGHT,
    };
    use eframe::egui::{Pos2, Rect, Vec2};

    #[test]
    fn formats_pattern_tab_label() {
        let pattern = PatternTabInfo {
            diff: "SC".into(),
            level: Some(12),
            floor_name: Some("12.3".into()),
            gold: String::new(),
            note: String::new(),
            assist_key: String::new(),
        };

        assert_eq!(pattern_label(Some(&pattern)), "12.3");
    }

    #[test]
    fn recommendation_row_width_keeps_pyqt_margins_inside_row() {
        assert_eq!(recommend_row_inner_width(), 270.0);
    }

    #[test]
    fn badge_rect_is_vertically_centered_in_row() {
        let cell = Rect::from_min_size(Pos2::ZERO, Vec2::new(36.0, RECOMMEND_ROW_HEIGHT));
        let badge = centered_badge_rect(cell, 36.0, 1.0);

        assert_eq!(badge.height(), BADGE_HEIGHT);
        assert_eq!(badge.center().y, cell.center().y);
    }
}
