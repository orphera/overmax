use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
pub use windows::AdaptiveCaptureEngine;
#[cfg(target_os = "linux")]
pub use linux::AdaptiveCaptureEngine;

pub trait CaptureEngine: Send + Sync {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String>;
}
