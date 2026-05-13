const REF_WIDTH: i32 = 1920;
const REF_HEIGHT: i32 = 1080;
const REF_ASPECT: f32 = REF_WIDTH as f32 / REF_HEIGHT as f32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoiRect {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct RoiManager {
    width: i32,
    height: i32,
    scale: f32,
    offset_x: i32,
    offset_y: i32,
}

impl RoiManager {
    pub fn new(width: i32, height: i32) -> Self {
        let mut manager = Self {
            width,
            height,
            scale: 1.0,
            offset_x: 0,
            offset_y: 0,
        };
        manager.calculate_transform();
        manager
    }

    pub fn update_window_size(&mut self, width: i32, height: i32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.calculate_transform();
    }

    pub fn get_roi(&self, name: &str) -> Option<RoiRect> {
        let roi = match name {
            "logo" => RoiRect { x1: 167, y1: 23, x2: 303, y2: 49 },
            "jacket" => RoiRect { x1: 710, y1: 534, x2: 768, y2: 592 },
            "rate" => RoiRect { x1: 176, y1: 583, x2: 270, y2: 605 },
            "btn_mode" => RoiRect { x1: 80, y1: 130, x2: 85, y2: 135 },
            "max_combo_badge" => RoiRect { x1: 409, y1: 587, x2: 445, y2: 620 },
            "diff_panel" => RoiRect { x1: 98, y1: 488, x2: 208, y2: 516 },
            _ => return None,
        };
        Some(self.transform_roi(roi))
    }

    pub fn get_diff_panel_roi(&self, diff: &str) -> Option<RoiRect> {
        let offset = match diff {
            "NM" => 0,
            "HD" => 120,
            "MX" => 240,
            "SC" => 360,
            _ => return None,
        };
        Some(self.transform_roi(RoiRect {
            x1: 98 + offset,
            y1: 488,
            x2: 208 + offset,
            y2: 516,
        }))
    }

    fn calculate_transform(&mut self) {
        if self.width <= 0 || self.height <= 0 {
            return;
        }

        let current_aspect = self.width as f32 / self.height as f32;
        if current_aspect > REF_ASPECT {
            self.scale = self.height as f32 / REF_HEIGHT as f32;
            self.offset_x = ((self.width as f32 - REF_WIDTH as f32 * self.scale) / 2.0) as i32;
            self.offset_y = 0;
        } else if current_aspect < REF_ASPECT {
            self.scale = self.width as f32 / REF_WIDTH as f32;
            self.offset_x = 0;
            self.offset_y = ((self.height as f32 - REF_HEIGHT as f32 * self.scale) / 2.0) as i32;
        } else {
            self.scale = self.width as f32 / REF_WIDTH as f32;
            self.offset_x = 0;
            self.offset_y = 0;
        }
    }

    fn transform_roi(&self, roi: RoiRect) -> RoiRect {
        let (x1, y1) = self.transform_point(roi.x1, roi.y1);
        let (x2, y2) = self.transform_point(roi.x2, roi.y2);
        RoiRect { x1, y1, x2, y2 }
    }

    fn transform_point(&self, x: i32, y: i32) -> (i32, i32) {
        (
            self.offset_x + (x as f32 * self.scale) as i32,
            self.offset_y + (y as f32 * self.scale) as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{RoiManager, RoiRect};

    #[test]
    fn keeps_1080p_reference_coordinates() {
        let manager = RoiManager::new(1920, 1080);
        assert_eq!(
            manager.get_roi("jacket"),
            Some(RoiRect { x1: 710, y1: 534, x2: 768, y2: 592 })
        );
    }

    #[test]
    fn applies_letterbox_offset_for_16_10() {
        let manager = RoiManager::new(1920, 1200);
        assert_eq!(manager.get_roi("logo").unwrap().y1, 83);
    }
}
