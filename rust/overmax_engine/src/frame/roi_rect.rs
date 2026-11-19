use crate::capture::frame_utils::{crop_roi, ImageView};
use crate::frame::CapturedFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRoiRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

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
