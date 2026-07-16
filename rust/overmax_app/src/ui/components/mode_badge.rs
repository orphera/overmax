use crate::ui::overlay_theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, FontId, Vec2};

pub struct ModeBadge<'a> {
    mode: Option<&'a str>,
    scale: f32,
    width: Option<f32>,
    height: Option<f32>,
}

impl<'a> ModeBadge<'a> {
    pub fn new(mode: Option<&'a str>) -> Self {
        Self {
            mode,
            scale: 1.0,
            width: None,
            height: None,
        }
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn mode_color(mode: &str) -> Color32 {
        match mode {
            "4B" => Color32::from_rgb(0x2D, 0x4F, 0x55),
            "5B" => Color32::from_rgb(0x44, 0xA9, 0xC6),
            "6B" => Color32::from_rgb(0xED, 0x94, 0x30),
            "8B" => Color32::from_rgb(0x1D, 0x14, 0x31),
            _ => Color32::from_rgb(0x6A, 0x4D, 0x3D),
        }
    }
}

impl<'a> egui::Widget for ModeBadge<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let text = self.mode.unwrap_or("—");
        let color = self
            .mode
            .map_or(Color32::from_rgb(0x6A, 0x4D, 0x3D), Self::mode_color);

        let px = crate::ui::overlay_ui::Px::new(self.scale);
        let width = self.width.unwrap_or(px.mode_badge_w());
        let height = self.height.unwrap_or(px.mode_badge_h());

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, CornerRadius::same((3.0 * self.scale) as u8), color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                FontId::proportional(12.0 * self.scale),
                Theme::TEXT_PRIMARY,
            );
        }

        response
    }
}
