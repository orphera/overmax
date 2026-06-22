use overmax_core::SceneType;
use overmax_data::{GlobalRoiConfig, RoiRect as DataRoiRect};

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

impl From<DataRoiRect> for RoiRect {
    fn from(rect: DataRoiRect) -> Self {
        Self {
            x1: rect.x,
            y1: rect.y,
            x2: rect.x + rect.width,
            y2: rect.y + rect.height,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoiManager {
    width: i32,
    height: i32,
    scale: f32,
    offset_x: i32,
    offset_y: i32,
    current_scene: SceneType,
    config: GlobalRoiConfig,
}

impl RoiManager {
    pub fn new(width: i32, height: i32) -> Self {
        let mut manager = Self {
            width,
            height,
            scale: 1.0,
            offset_x: 0,
            offset_y: 0,
            current_scene: SceneType::Unknown,
            config: GlobalRoiConfig::default(),
        };
        manager.calculate_transform();
        manager
    }

    pub fn set_scene(&mut self, scene: SceneType) {
        self.current_scene = scene;
    }

    pub fn current_scene(&self) -> SceneType {
        self.current_scene
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
        if name == "logo" {
            return Some(self.transform_roi(RoiRect { x1: 20, y1: 15, x2: 340, y2: 90 }));
        }
        if name == "bottom_guide" {
            return Some(self.transform_roi(RoiRect { x1: 1300, y1: 950, x2: 1900, y2: 1030 }));
        }
        let roi = self.config.scenes.get(&self.current_scene)?.rois.get(name)?;
        Some(self.transform_roi(RoiRect::from(roi.clone())))
    }

    pub fn get_diff_panel_roi(&self, diff: &str) -> Option<RoiRect> {
        let offset = match diff {
            "NM" => 0,
            "HD" => 120,
            "MX" => 240,
            "SC" => 360,
            _ => return None,
        };
        let roi = self.config.scenes.get(&self.current_scene)?.rois.get("diff_panel")?;
        Some(self.transform_roi(RoiRect {
            x1: roi.x + offset,
            y1: roi.y,
            x2: roi.x + roi.width + offset,
            y2: roi.y + roi.height,
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
    use super::{RoiManager, RoiRect, SceneType};

    #[test]
    fn keeps_1080p_reference_coordinates() {
        let mut manager = RoiManager::new(1920, 1080);
        manager.set_scene(SceneType::Freestyle);
        assert_eq!(
            manager.get_roi("jacket"),
            Some(RoiRect {
                x1: 710,
                y1: 534,
                x2: 768,
                y2: 592
            })
        );
    }

    #[test]
    fn applies_letterbox_offset_for_16_10() {
        let mut manager = RoiManager::new(1920, 1200);
        manager.set_scene(SceneType::Freestyle);
        assert_eq!(manager.get_roi("logo").unwrap().y1, 75);
    }
}
