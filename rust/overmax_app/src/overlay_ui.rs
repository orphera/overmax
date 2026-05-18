use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Label, Layout, Margin, RichText, Sense, Vec2,
};
use overmax_core::GameSessionState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const WIDTH: f32 = 360.0;
pub const HEIGHT: f32 = 326.0;

#[derive(Default, Clone, Copy)]
pub struct OverlayActions {
    pub start_drag: bool,
}

struct Theme;

impl Theme {
    const PANEL_BG: Color32 = Color32::from_rgb(18, 24, 38);
    const PANEL_STROKE: Color32 = Color32::from_rgb(18, 24, 38);
    const HEADER_BG: Color32 = Color32::from_rgb(30, 40, 62);
    const SECTION_BG: Color32 = Color32::from_rgb(22, 30, 48);
    const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 244, 255);
    const TEXT_SECONDARY: Color32 = Color32::from_rgb(80, 88, 112);
    const TEXT_ACCENT: Color32 = Color32::from_rgb(255, 210, 102);
    const OK: Color32 = Color32::from_rgb(0, 212, 255);
    const WARN: Color32 = Color32::from_rgb(255, 75, 75);
}

struct Px;

impl Px {
    const PANEL_MARGIN: i8 = 8;
    const PANEL_GAP: f32 = 6.0;
    const HEADER_RADIUS: u8 = 10;
    const HEADER_MARGIN_X: i8 = 12;
    const HEADER_MARGIN_Y: i8 = 8;
    const HEADER_ROW_GAP: f32 = 8.0;
    const HEADER_META_GAP: f32 = 4.0;
    const STATUS_DOT: f32 = 7.0;
    const MODE_BADGE_W: f32 = 28.0;
    const MODE_BADGE_H: f32 = 22.0;
    const SETTINGS_BTN: f32 = 24.0;
    const BODY_GAP: f32 = 6.0;
    const TAB_WIDTH: f32 = 52.0;
    const TAB_HEIGHT: f32 = 46.0;
    const TAB_GAP: f32 = 4.0;
    const TAB_RADIUS: u8 = 6;
    const TAB_PANEL_PAD_Y: f32 = 6.0;
    const RECOMMEND_PAD_Y: f32 = 8.0;
    const RECOMMEND_ROW_GAP: f32 = 4.0;
    const FOOTER_MARGIN_X: i8 = 10;
    const FOOTER_MARGIN_Y: i8 = 5;
    const INNER_WIDTH: f32 = WIDTH - 16.0;
    const RECOMMEND_WIDTH: f32 = Self::INNER_WIDTH - Self::TAB_WIDTH - Self::BODY_GAP;
    const BODY_HEIGHT: f32 =
        Self::TAB_PANEL_PAD_Y * 2.0 + Self::TAB_HEIGHT * 4.0 + Self::TAB_GAP * 3.0;
}

pub fn install_korean_font(ctx: &egui::Context) {
    let Some(font_bytes) = load_windows_korean_font() else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "malgun_gothic".to_string(),
        std::sync::Arc::new(FontData::from_owned(font_bytes)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "malgun_gothic".to_string());
    }
    ctx.set_fonts(fonts);
}

pub fn load_windows_korean_font() -> Option<Vec<u8>> {
    for path in [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\malgunsl.ttf",
        r"C:\Windows\Fonts\gulim.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

pub fn draw_overlay_panel(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    confidence: f32,
    settings_open: Arc<AtomicBool>,
    debug_open: Arc<AtomicBool>,
    sync_open: Arc<AtomicBool>,
) -> OverlayActions {
    let mut actions = OverlayActions::default();
    Frame::new()
        .fill(Theme::PANEL_BG)
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(Px::PANEL_MARGIN))
        .stroke(egui::Stroke::new(1.0, Theme::PANEL_STROKE))
        .show(ui, |ui| {
            ui.set_width(WIDTH - f32::from(Px::PANEL_MARGIN * 2));
            draw_header(ui, state, &settings_open, &mut actions);
            ui.add_space(Px::PANEL_GAP);
            draw_body(ui, state);
            ui.add_space(Px::PANEL_GAP);
            draw_footer(ui, confidence, &debug_open, &sync_open);
        });
    actions
}

fn draw_header(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    settings_open: &Arc<AtomicBool>,
    actions: &mut OverlayActions,
) {
    let header = Frame::new()
        .fill(Theme::HEADER_BG)
        .corner_radius(CornerRadius::same(Px::HEADER_RADIUS))
        .inner_margin(Margin::symmetric(Px::HEADER_MARGIN_X, Px::HEADER_MARGIN_Y))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = Px::HEADER_ROW_GAP;
                draw_status_lamp(ui, state.is_stable);
                draw_mode_badge(ui, state.mode.as_deref());
                ui.add(
                    Label::new(
                        RichText::new("곡을 선택하세요")
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(14.0))
                            .strong(),
                    )
                    .selectable(false),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let btn = Button::new(RichText::new("⚙").color(Theme::TEXT_SECONDARY))
                        .frame(false)
                        .min_size(Vec2::splat(Px::SETTINGS_BTN));
                    if ui.add(btn).clicked() {
                        let v = settings_open.load(Ordering::Relaxed);
                        settings_open.store(!v, Ordering::Relaxed);
                    }
                });
            });
            ui.add_space(Px::HEADER_META_GAP);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add(
                    Label::new(
                        RichText::new(meta_text(state))
                            .color(Theme::TEXT_ACCENT)
                            .font(FontId::proportional(10.0))
                            .strong(),
                    )
                    .selectable(false),
                );
            });
        });

    let drag_response = ui.interact(
        header.response.rect,
        ui.id().with("overlay_header_drag"),
        Sense::click_and_drag(),
    );
    if drag_response.drag_started() {
        actions.start_drag = true;
    }
}

fn draw_status_lamp(ui: &mut egui::Ui, stable: bool) {
    let color = if stable { Theme::OK } else { Theme::WARN };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(Px::STATUS_DOT), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn draw_mode_badge(ui: &mut egui::Ui, mode: Option<&str>) {
    let text = mode.unwrap_or("—");
    let color = match mode {
        Some("4B") => Color32::from_rgb(0x2D, 0x4F, 0x55),
        Some("5B") => Color32::from_rgb(0x44, 0xA9, 0xC6),
        Some("6B") => Color32::from_rgb(0xED, 0x94, 0x30),
        Some("8B") => Color32::from_rgb(0x1D, 0x14, 0x31),
        _ => Color32::from_rgb(0x6A, 0x4D, 0x3D),
    };

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(Px::MODE_BADGE_W, Px::MODE_BADGE_H),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, CornerRadius::same(3), color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        Theme::TEXT_PRIMARY,
    );
}

fn draw_body(ui: &mut egui::Ui, state: &GameSessionState) {
    ui.allocate_ui_with_layout(
        Vec2::new(Px::INNER_WIDTH, Px::BODY_HEIGHT),
        Layout::left_to_right(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.x = Px::BODY_GAP;
            draw_diff_tabs(ui, state.diff.as_deref());
            draw_recommend_placeholder(ui, Px::RECOMMEND_WIDTH, Px::BODY_HEIGHT);
        },
    );
}

fn draw_diff_tabs(ui: &mut egui::Ui, active: Option<&str>) {
    ui.set_width(Px::TAB_WIDTH);
    ui.vertical(|ui| {
        ui.add_space(Px::TAB_PANEL_PAD_Y);
        ui.spacing_mut().item_spacing.y = Px::TAB_GAP;
        for diff in ["NM", "HD", "MX", "SC"] {
            let fill = if active == Some(diff) {
                diff_color(diff)
            } else {
                Color32::from_rgb(28, 36, 54)
            };
            Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(Px::TAB_RADIUS))
                .inner_margin(Margin::same(0))
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(Px::TAB_WIDTH, Px::TAB_HEIGHT));
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(diff)
                                .color(Theme::TEXT_PRIMARY)
                                .font(FontId::proportional(11.0))
                                .strong(),
                        );
                        ui.label(
                            RichText::new("—")
                                .color(Color32::from_rgb(80, 88, 112))
                                .font(FontId::proportional(10.0))
                                .strong(),
                        );
                    });
                });
        }
        ui.add_space(Px::TAB_PANEL_PAD_Y);
    });
}

fn draw_recommend_placeholder(ui: &mut egui::Ui, width: f32, height: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.add_space(Px::RECOMMEND_PAD_Y);
            ui.spacing_mut().item_spacing.y = Px::RECOMMEND_ROW_GAP;
            ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                ui.add(
                    Label::new(
                        RichText::new("패턴을 감지하는 중...")
                            .color(Theme::TEXT_SECONDARY)
                            .font(FontId::proportional(11.0)),
                    )
                    .selectable(false),
                );
            });
            ui.add_space(Px::RECOMMEND_PAD_Y);
        },
    );
}

fn draw_footer(
    ui: &mut egui::Ui,
    confidence: f32,
    debug_open: &Arc<AtomicBool>,
    sync_open: &Arc<AtomicBool>,
) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(Px::FOOTER_MARGIN_X, Px::FOOTER_MARGIN_Y))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.small_button("debug").clicked() {
                    let v = debug_open.load(Ordering::Relaxed);
                    debug_open.store(!v, Ordering::Relaxed);
                }
                if ui.small_button("sync").clicked() {
                    let v = sync_open.load(Ordering::Relaxed);
                    sync_open.store(!v, Ordering::Relaxed);
                }
                ui.label(RichText::new("유사 구간 평균").color(Theme::TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("신뢰도 {:.0}%", confidence * 100.0))
                            .color(Theme::TEXT_SECONDARY)
                            .strong(),
                    );
                    ui.label(RichText::new("——").color(Theme::TEXT_SECONDARY));
                });
            });
        });
}

fn meta_text(state: &GameSessionState) -> String {
    match (state.mode.as_deref(), state.diff.as_deref()) {
        (Some(mode), Some(diff)) => format!("{mode} | {diff}"),
        _ => "—".to_string(),
    }
}

fn diff_color(diff: &str) -> Color32 {
    match diff {
        "NM" => Color32::from_rgb(0x4A, 0x90, 0xD9),
        "HD" => Color32::from_rgb(0xF5, 0xA6, 0x23),
        "MX" => Color32::from_rgb(0xD0, 0x02, 0x1B),
        "SC" => Color32::from_rgb(0x9B, 0x59, 0xB6),
        _ => Color32::WHITE,
    }
}

#[cfg(test)]
mod tests {
    use super::{diff_color, load_windows_korean_font, meta_text};
    use eframe::egui::Color32;
    use overmax_core::GameSessionState;

    #[test]
    fn formats_empty_meta_like_pyqt_header() {
        assert_eq!(meta_text(&GameSessionState::detecting()), "—");
    }

    #[test]
    fn uses_existing_diff_colors() {
        assert_eq!(diff_color("SC"), Color32::from_rgb(0x9B, 0x59, 0xB6));
    }

    #[test]
    fn finds_windows_korean_font_on_target_machine() {
        assert!(load_windows_korean_font().is_some());
    }
}
