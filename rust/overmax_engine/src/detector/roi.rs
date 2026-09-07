use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{crop_roi, ImageView};
use crate::detector::roi_config::{GlobalRoiConfig, RawRoiRect};
use overmax_core::{Difficulty, SceneType};

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

impl RoiRect {
    pub const fn from_raw(rect: RawRoiRect) -> Self {
        Self {
            x1: rect.x,
            y1: rect.y,
            x2: rect.x + rect.width,
            y2: rect.y + rect.height,
        }
    }

    #[inline]
    pub const fn width(&self) -> i32 {
        self.x2 - self.x1
    }

    #[inline]
    pub const fn height(&self) -> i32 {
        self.y2 - self.y1
    }

    pub fn crop<'a>(&self, frame: &'a CapturedFrame) -> Option<ImageView<'a>> {
        crop_roi(frame, *self)
    }

    pub fn and_then<'a, T>(
        &self,
        frame: &'a CapturedFrame,
        f: impl FnOnce(&ImageView<'a>) -> Option<T>,
    ) -> Option<T> {
        self.crop(frame).as_ref().and_then(f)
    }

    pub fn with_margin(&self, margin: i32) -> Self {
        Self {
            x1: self.x1 - margin,
            y1: self.y1 - margin,
            x2: self.x2 + margin,
            y2: self.y2 + margin,
        }
    }
}

impl From<RawRoiRect> for RoiRect {
    fn from(rect: RawRoiRect) -> Self {
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
    is_atlas: bool,
    pub(crate) config: GlobalRoiConfig,
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
            is_atlas: width == 512 && height == 512,
            config: GlobalRoiConfig::default(),
        };
        if !manager.is_atlas {
            manager.calculate_transform();
        }
        manager
    }

    pub fn set_scene(&mut self, scene: SceneType) {
        self.current_scene = scene;
    }

    pub fn current_scene(&self) -> SceneType {
        self.current_scene
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn offset_y(&self) -> i32 {
        self.offset_y
    }

    pub fn is_atlas_mode(&self) -> bool {
        self.is_atlas
    }

    pub fn set_atlas_mode(&mut self, is_atlas: bool) {
        self.is_atlas = is_atlas;
    }

    pub fn update_window_size(&mut self, width: i32, height: i32) {
        if width == 512 && height == 512 {
            self.is_atlas = true;
            self.width = width;
            self.height = height;
            self.scale = 1.0;
            self.offset_x = 0;
            self.offset_y = 0;
            return;
        }
        self.is_atlas = false;
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.calculate_transform();
    }

    /// 지정된 씬의 ROI 영역을 반환합니다.
    pub fn get_roi_for_scene(&self, name: &str, scene: SceneType) -> Option<RoiRect> {
        if self.is_atlas {
            return crate::detector::atlas_translator::AtlasTranslator::get_roi_for_scene(
                name, scene,
            );
        }
        let roi = self.config.scenes.get(&scene)?.rois.get(name)?;
        Some(self.transform_roi(RoiRect::from(*roi)))
    }

    pub fn get_roi(&self, name: &str) -> Option<RoiRect> {
        self.get_roi_for_scene(name, self.current_scene)
    }

    pub fn and_then_roi<'a, T>(
        &self,
        frame: &'a CapturedFrame,
        name: &str,
        f: impl FnOnce(&ImageView<'a>) -> Option<T>,
    ) -> Option<T> {
        self.get_roi(name).and_then(|roi| roi.and_then(frame, f))
    }

    pub fn get_diff_panel_roi_for_scene(
        &self,
        diff: Difficulty,
        scene: SceneType,
    ) -> Option<RoiRect> {
        if self.is_atlas {
            return crate::detector::atlas_translator::AtlasTranslator::get_diff_panel_roi_for_scene(
                diff, scene,
            );
        }
        let offset = match diff {
            Difficulty::NM => 0,
            Difficulty::HD => 120,
            Difficulty::MX => 240,
            Difficulty::SC => 360,
        };
        let roi = self.config.scenes.get(&scene)?.rois.get("diff_panel")?;
        Some(self.transform_roi(RoiRect {
            x1: roi.x + offset,
            y1: roi.y,
            x2: roi.x + roi.width + offset,
            y2: roi.y + roi.height,
        }))
    }

    pub fn get_diff_panel_roi(&self, diff: Difficulty) -> Option<RoiRect> {
        self.get_diff_panel_roi_for_scene(diff, self.current_scene)
    }

    fn calculate_transform(&mut self) {
        if self.width <= 0 || self.height <= 0 {
            return;
        }

        const ASPECT_EPSILON: f32 = 0.005;

        let current_aspect = self.width as f32 / self.height as f32;
        if (current_aspect - REF_ASPECT).abs() < ASPECT_EPSILON {
            self.scale = self.width as f32 / REF_WIDTH as f32;
            self.offset_x = 0;
            self.offset_y = 0;
        } else if current_aspect > REF_ASPECT {
            self.scale = self.height as f32 / REF_HEIGHT as f32;
            self.offset_x = ((self.width as f32 - REF_WIDTH as f32 * self.scale) / 2.0) as i32;
            self.offset_y = 0;
        } else {
            self.scale = self.width as f32 / REF_WIDTH as f32;
            self.offset_x = 0;
            self.offset_y = ((self.height as f32 - REF_HEIGHT as f32 * self.scale) / 2.0) as i32;
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
                y1: 533,
                x2: 770,
                y2: 593
            })
        );
    }

    #[test]
    fn applies_letterbox_offset_for_16_10() {
        let mut manager = RoiManager::new(1920, 1200);
        manager.set_scene(SceneType::Freestyle);
        assert_eq!(manager.get_roi("jacket").unwrap().y1, 593);
    }

    #[test]
    fn handles_near_16_9_windowed_resolutions_with_epsilon() {
        // 3838x2159 is slightly off from 16:9 due to window frame/border (aspect = 1.77767...)
        let manager = RoiManager::new(3838, 2159);
        assert_eq!(manager.offset_x, 0);
        assert_eq!(manager.offset_y, 0);
    }

    #[test]
    fn test_roi_manager_atlas_mode_delegates_to_atlas_translator() {
        let mut manager = RoiManager::new(1920, 1080);
        assert!(!manager.is_atlas_mode());

        // 512x512 아틀라스 프레임 수신 시 아틀라스 모드로 자동 전환
        manager.update_window_size(512, 512);
        assert!(manager.is_atlas_mode());

        // 아틀라스 점프 테이블 좌표(0, 0, 407, 94) 반환 검증
        let atlas_roi = manager.get_roi_for_scene("score", SceneType::ResultFreestyle);
        assert_eq!(
            atlas_roi,
            Some(RoiRect {
                x1: 0,
                y1: 0,
                x2: 407,
                y2: 94
            })
        );

        // 다시 1080p 프레임 수신 시 표준 모드로 자동 복귀
        manager.update_window_size(1920, 1080);
        assert!(!manager.is_atlas_mode());
        let ref_roi = manager.get_roi_for_scene("score", SceneType::ResultFreestyle);
        assert_eq!(
            ref_roi,
            Some(RoiRect {
                x1: 759,
                y1: 710,
                x2: 1166,
                y2: 804
            })
        );
    }
}
