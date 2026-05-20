use eframe::egui::{self, Color32};

pub struct Theme;

impl Theme {
    // Backgrounds
    pub const PANEL_BG: Color32 = Color32::from_rgb(18, 24, 38);
    pub const PANEL_STROKE: Color32 = Color32::from_rgb(18, 24, 38);
    pub const HEADER_BG: Color32 = Color32::from_rgb(30, 40, 62);
    pub const SECTION_BG: Color32 = Color32::from_rgb(22, 30, 48);
    pub const ROW_BG: Color32 = Color32::from_rgb(36, 46, 70);

    // Tab Backgrounds
    pub const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(63, 80, 117);
    pub const TAB_INACTIVE_BG: Color32 = Color32::from_rgb(28, 36, 54);
    pub const TAB_DIM_BG: Color32 = Color32::from_rgb(20, 26, 40);

    // Text Colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 244, 255);      // #F0F4FF
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(136, 145, 167);   // #8891A7
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(80, 88, 112);         // #505870
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(255, 209, 102);      // #FFD166
    pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(232, 238, 255);      // #E8EEFF
    pub const TEXT_HINT: Color32 = Color32::from_rgb(180, 203, 255);        // #B4CBFF

    // Status Colors
    pub const OK: Color32 = Color32::from_rgb(0, 212, 255);
    pub const WARN: Color32 = Color32::from_rgb(255, 75, 75);

    // Log Category Colors
    pub const LOG_CAPTURE: Color32 = Color32::from_rgb(126, 200, 227);
    pub const LOG_OVERLAY: Color32 = Color32::from_rgb(181, 234, 215);
    pub const LOG_VARCHIVE: Color32 = Color32::from_rgb(255, 214, 165);
    pub const LOG_WINDOW: Color32 = Color32::from_rgb(201, 177, 255);
    pub const LOG_MAIN: Color32 = Color32::from_rgb(255, 255, 181);
    pub const LOG_DEFAULT: Color32 = Color32::from_rgb(204, 204, 204);

    // Button States
    pub const BTN_PAUSED: Color32 = Color32::from_rgb(90, 58, 26);

    pub const CARD: Color32 = Color32::from_rgb(22, 30, 48); // SECTION_BG
    pub const STROKE: Color32 = Color32::from_rgb(28, 36, 54); // TAB_INACTIVE_BG
    
    // Unified Rounding (8-bit CornerRadius)
    pub const R_SM: u8 = 6;
    pub const R_MD: u8 = 10;
    pub const R_LG: u8 = 14;

    // Action Colors
    pub const PRIMARY: Color32 = Color32::from_rgb(255, 209, 102); // TEXT_ACCENT
    pub const PRIMARY_HOVER: Color32 = Color32::from_rgb(255, 220, 140);
    pub const SECONDARY: Color32 = Color32::from_rgb(63, 80, 117); // TAB_ACTIVE_BG
    pub const SECONDARY_HOVER: Color32 = Color32::from_rgb(80, 100, 140);
    pub const DANGER: Color32 = Color32::from_rgb(255, 75, 75); // WARN

    // Layout & Sizing
    pub const LABEL_WIDTH: f32 = 100.0;
    pub const CONTROL_HEIGHT: f32 = 32.0;
    pub const ROW_SPACING: f32 = 12.0;

    // Font Sizes (unscaled)
    pub const FONT_HEADING: f32 = 24.0;
    pub const FONT_BODY: f32 = 15.0;
    pub const FONT_SMALL: f32 = 13.0;
    pub const FONT_TINY: f32 = 11.0;
}

pub fn apply_secondary_window_style(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        // Typography
        let mut families = std::collections::BTreeMap::new();
        families.insert(egui::TextStyle::Body, egui::FontId::new(Theme::FONT_BODY, egui::FontFamily::Proportional));
        families.insert(egui::TextStyle::Button, egui::FontId::new(Theme::FONT_BODY, egui::FontFamily::Proportional));
        families.insert(egui::TextStyle::Heading, egui::FontId::new(Theme::FONT_HEADING, egui::FontFamily::Proportional));
        families.insert(egui::TextStyle::Monospace, egui::FontId::new(Theme::FONT_BODY, egui::FontFamily::Monospace));
        families.insert(egui::TextStyle::Small, egui::FontId::new(Theme::FONT_SMALL, egui::FontFamily::Proportional));
        s.text_styles = families;

        s.visuals.widgets.inactive.bg_fill = Theme::TAB_INACTIVE_BG;
        s.visuals.widgets.hovered.bg_fill = Theme::TAB_ACTIVE_BG;
        s.visuals.widgets.active.bg_fill = Theme::PRIMARY;
        s.visuals.selection.bg_fill = Theme::SECONDARY;
        
        s.visuals.window_corner_radius = Theme::R_LG.into();
        s.visuals.window_shadow = egui::Shadow::NONE;
        
        s.spacing.item_spacing = egui::vec2(12.0, 12.0);
        s.spacing.button_padding = egui::vec2(12.0, 8.0);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.bar_inner_margin = 2.0;
    });
}

/// Renders a professional "Pill" tab bar.
pub fn render_pill_tabs(ui: &mut egui::Ui, _id_source: &str, labels: &[&str], active: &mut usize) {
    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 4.0;
        
        egui::Frame::new()
            .fill(Theme::TAB_INACTIVE_BG)
            .corner_radius(egui::CornerRadius::same(Theme::R_SM))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                for (idx, label) in labels.iter().enumerate() {
                    let is_active = *active == idx;
                    let text_color = if is_active { Theme::TEXT_ACCENT } else { Theme::TEXT_SECONDARY };
                    let bg_fill = if is_active { Theme::TAB_ACTIVE_BG } else { egui::Color32::TRANSPARENT };
                    
                    let response = ui.add(
                        egui::Button::new(
                            egui::RichText::new(*label)
                                .size(Theme::FONT_BODY)
                                .color(text_color)
                                .strong()
                        )
                        .fill(bg_fill)
                        .corner_radius(egui::CornerRadius::same(Theme::R_SM))
                        .stroke(egui::Stroke::NONE)
                    );
                    
                    if response.clicked() {
                        *active = idx;
                    }
                }
            });
    });
}
