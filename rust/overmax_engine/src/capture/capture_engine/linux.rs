use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;
use crate::capture::x11_capture::X11CaptureEngine;

pub struct AdaptiveCaptureEngine {
    backend: X11CaptureEngine,
}

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            backend: X11CaptureEngine::new()?,
        })
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        self.backend.capture_bgra(rect)
    }

    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        self.backend.capture_bgra_inplace(rect, out_frame)
    }
}
