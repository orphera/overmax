use crate::ui::overlay_theme::Theme;
use eframe::egui::{self, Vec2};

pub struct StatusLamp {
    stable: bool,
    scale: f32,
    height: Option<f32>,
    dot_size: Option<f32>,
}

impl StatusLamp {
    pub fn new(stable: bool) -> Self {
        Self {
            stable,
            scale: 1.0,
            height: None,
            dot_size: None,
        }
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn dot_size(mut self, dot_size: f32) -> Self {
        self.dot_size = Some(dot_size);
        self
    }
}

impl egui::Widget for StatusLamp {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let color = if self.stable { Theme::OK } else { Theme::WARN };
        let width = self.dot_size.unwrap_or(7.0 * self.scale);
        let height = self.height.unwrap_or(18.0 * self.scale);

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter()
                .circle_filled(rect.center(), 3.5 * self.scale, color);
        }

        response
    }
}
