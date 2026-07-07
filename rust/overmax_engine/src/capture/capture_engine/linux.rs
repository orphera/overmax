use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;

pub struct AdaptiveCaptureEngine;

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn capture_bgra(&mut self, _rect: WindowRect) -> Result<CapturedFrame, String> {
        Err("X11 capture is not implemented yet".to_string())
    }

    fn capture_bgra_inplace(
        &mut self,
        _rect: WindowRect,
        _out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        Err("X11 capture is not implemented yet".to_string())
    }
}
