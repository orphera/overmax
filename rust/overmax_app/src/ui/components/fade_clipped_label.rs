use eframe::egui::{self, Color32, FontId, Rect, RichText, Vec2};

pub struct FadeClippedLabel<'a> {
    text: &'a str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
    bg_color: Color32,
    scale: f32,
}

impl<'a> FadeClippedLabel<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            font_id: FontId::proportional(14.0),
            color: Color32::WHITE,
            max_width: f32::INFINITY,
            bg_color: Color32::BLACK,
            scale: 1.0,
        }
    }

    pub fn font(mut self, font_id: FontId) -> Self {
        self.font_id = font_id;
        self
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn max_width(mut self, max_width: f32) -> Self {
        self.max_width = max_width;
        self
    }

    pub fn bg_color(mut self, bg_color: Color32) -> Self {
        self.bg_color = bg_color;
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

impl<'a> egui::Widget for FadeClippedLabel<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let widget_text: egui::WidgetText = RichText::new(self.text)
            .color(self.color)
            .font(self.font_id)
            .strong()
            .into();

        let galley = widget_text.into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::FontSelection::Default,
        );
        let title_w = galley.size().x;
        let height = galley.size().y;

        if title_w <= self.max_width {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(title_w, height), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter().galley(rect.min, galley, self.color);
            }
            response
        } else {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(self.max_width, height), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                let old_clip_rect = ui.clip_rect();
                let clip_rect = rect.intersect(old_clip_rect);

                let clipped_painter = ui.painter().with_clip_rect(clip_rect);
                clipped_painter.galley(rect.min, galley, self.color);

                let fade_w = 28.0 * self.scale;
                let fade_rect =
                    Rect::from_min_max(egui::pos2(rect.max.x - fade_w, rect.min.y), rect.max);

                let mut mesh = egui::Mesh::default();
                // 투명한 검은색(TRANSPARENT) 대신 배경색의 알파만 0으로 바꾼 색을 지정하여
                // RGBA 채널 보간 시 탁한 회색빛 노이즈가 튀는 현상(Black bleeding)을 해결
                let c_start = Color32::from_rgba_unmultiplied(
                    self.bg_color.r(),
                    self.bg_color.g(),
                    self.bg_color.b(),
                    0,
                );
                let c_end = self.bg_color;

                mesh.vertices.push(egui::epaint::Vertex {
                    pos: fade_rect.left_top(),
                    color: c_start,
                    uv: egui::pos2(0.0, 0.0),
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: fade_rect.right_top(),
                    color: c_end,
                    uv: egui::pos2(1.0, 0.0),
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: fade_rect.right_bottom(),
                    color: c_end,
                    uv: egui::pos2(1.0, 1.0),
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: fade_rect.left_bottom(),
                    color: c_start,
                    uv: egui::pos2(0.0, 1.0),
                });

                mesh.indices.push(0);
                mesh.indices.push(1);
                mesh.indices.push(2);
                mesh.indices.push(0);
                mesh.indices.push(2);
                mesh.indices.push(3);

                ui.painter().add(egui::Shape::mesh(mesh));
            }
            response
        }
    }
}
