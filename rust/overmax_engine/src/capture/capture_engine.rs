use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::{WindowRect, WindowSnapshot};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::AdaptiveCaptureEngine;
#[cfg(target_os = "windows")]
pub use windows::AdaptiveCaptureEngine;

pub trait CaptureEngine: Send + Sync {
    fn set_target(&mut self, _target: Option<WindowSnapshot>) -> Result<(), String> {
        Ok(())
    }

    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String>;
}
