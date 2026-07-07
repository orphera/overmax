use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;

pub struct X11CaptureEngine;

impl X11CaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl CaptureEngine for X11CaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        let mut frame = CapturedFrame::default();
        self.capture_bgra_inplace(rect, &mut frame)?;
        Ok(frame)
    }

    fn capture_bgra_inplace(
        &mut self,
        _rect: WindowRect,
        _out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        Err("X11 capture is not implemented yet".to_string())
    }
}
