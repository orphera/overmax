use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Layout, Margin, RichText, Vec2, ViewportBuilder,
};
use overmax_core::GameSessionState;
use std::sync::Arc;

const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 230.0;

pub fn run_overlay() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Overmax")
            .with_inner_size([WIDTH, HEIGHT])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Overmax",
        options,
        Box::new(|cc| {
            install_korean_font(&cc.egui_ctx);
            Ok(Box::new(OverlayApp::default()))
        }),
    )
}

fn install_korean_font(ctx: &egui::Context) {
    let Some(font_bytes) = load_windows_korean_font() else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("malgun_gothic".to_string(), Arc::new(FontData::from_owned(font_bytes)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "malgun_gothic".to_string());
    }
    ctx.set_fonts(fonts);
}

fn load_windows_korean_font() -> Option<Vec<u8>> {
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

struct OverlayApp {
    state: GameSessionState,
    confidence: f32,
}

impl Default for OverlayApp {
    fn default() -> Self {
        Self {
            state: GameSessionState::detecting(),
            confidence: 0.0,
        }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(WIDTH, HEIGHT));
                draw_panel(ui, &self.state, self.confidence);
            });
    }
}

fn draw_panel(ui: &mut egui::Ui, state: &GameSessionState, confidence: f32) {
    Frame::new()
        .fill(Color32::from_rgb(18, 24, 38))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(WIDTH - 16.0);
            draw_header(ui, state);
            ui.add_space(6.0);
            draw_body(ui, state);
            ui.add_space(6.0);
            draw_footer(ui, confidence);
        });
}

fn draw_header(ui: &mut egui::Ui, state: &GameSessionState) {
    Frame::new()
        .fill(Color32::from_rgb(30, 40, 62))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_status_lamp(ui, state.is_stable);
                draw_mode_badge(ui, state.mode.as_deref());
                ui.label(
                    RichText::new("곡을 선택하세요")
                        .color(Color32::from_rgb(240, 244, 255))
                        .font(FontId::proportional(14.0))
                        .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("⚙").color(Color32::from_rgb(80, 88, 112)));
                });
            });
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new(meta_text(state))
                        .color(Color32::from_rgb(255, 209, 102))
                        .font(FontId::proportional(10.0))
                        .strong(),
                );
            });
        });
}

fn draw_status_lamp(ui: &mut egui::Ui, stable: bool) {
    let color = if stable {
        Color32::from_rgb(0, 212, 255)
    } else {
        Color32::from_rgb(255, 75, 75)
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), egui::Sense::hover());
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
    Frame::new()
        .fill(color)
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .color(Color32::from_rgb(240, 244, 255))
                    .font(FontId::proportional(12.0))
                    .strong(),
            );
        });
}

fn draw_body(ui: &mut egui::Ui, state: &GameSessionState) {
    ui.horizontal(|ui| {
        draw_diff_tabs(ui, state.diff.as_deref());
        ui.add_space(6.0);
        draw_recommend_placeholder(ui);
    });
}

fn draw_diff_tabs(ui: &mut egui::Ui, active: Option<&str>) {
    ui.vertical(|ui| {
        for diff in ["NM", "HD", "MX", "SC"] {
            let fill = if active == Some(diff) {
                diff_color(diff)
            } else {
                Color32::from_rgb(36, 46, 70)
            };
            Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.label(RichText::new(diff).color(Color32::WHITE).strong());
                });
            ui.add_space(3.0);
        }
    });
}

fn draw_recommend_placeholder(ui: &mut egui::Ui) {
    Frame::new()
        .fill(Color32::from_rgb(36, 46, 70))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(250.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new("패턴을 감지하는 중...")
                        .color(Color32::from_rgb(80, 88, 112))
                        .font(FontId::proportional(11.0)),
                );
            });
        });
}

fn draw_footer(ui: &mut egui::Ui, confidence: f32) {
    Frame::new()
        .fill(Color32::from_rgb(22, 30, 48))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("유사 구간 평균").color(Color32::from_rgb(80, 88, 112)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("신뢰도 {:.0}%", confidence * 100.0))
                            .color(Color32::from_rgb(80, 88, 112))
                            .strong(),
                    );
                    ui.label(RichText::new("——").color(Color32::from_rgb(80, 88, 112)));
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
