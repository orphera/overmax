//! 업로드 버튼의 자동 업로드 대기 연출: 아래에서 위로 `PRIMARY` 색이 차오른다.

use crate::ui::overlay_theme::Theme;
use eframe::egui::{layers::ShapeIdx, CornerRadius, Rect, Shape, Ui};

/// 버튼 배경을 그릴 슬롯. 버튼보다 먼저 예약해야 버튼 텍스트가 그 위에 그려진다.
pub(crate) struct UploadFillSlots {
    base: ShapeIdx,
    fill: ShapeIdx,
}

impl UploadFillSlots {
    pub(crate) fn reserve(ui: &Ui) -> Self {
        Self {
            base: ui.painter().add(Shape::Noop),
            fill: ui.painter().add(Shape::Noop),
        }
    }

    /// `rect` 버튼 영역 중 아래쪽 `progress` 비율만큼 채운다.
    pub(crate) fn paint(self, ui: &Ui, rect: Rect, radius: CornerRadius, progress: f32) {
        let painter = ui.painter();
        painter.set(
            self.base,
            Shape::rect_filled(rect, radius, Theme::SECTION_BG),
        );

        let mut filled = rect;
        filled.min.y = rect.max.y - rect.height() * progress.clamp(0.0, 1.0);
        painter
            .with_clip_rect(filled)
            .set(self.fill, Shape::rect_filled(rect, radius, Theme::PRIMARY));
    }
}
