use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::{WindowRect, WindowSnapshot};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub use linux::AdaptiveCaptureEngine;
#[cfg(target_os = "windows")]
pub use windows::{AdaptiveCaptureEngine, PreferredCaptureEngine};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureErrorAction {
    Retry,
    Reconnect,
    Stop,
}

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

    fn error_action(&self) -> CaptureErrorAction {
        CaptureErrorAction::Retry
    }

    #[cfg(target_os = "windows")]
    fn set_preferred_engine(&mut self, _preferred: windows::PreferredCaptureEngine) {}

    #[cfg(target_os = "windows")]
    fn set_enable_gpu_atlas(&mut self, _enable: bool) {}

    #[cfg(target_os = "windows")]
    fn set_hdr_sdr_white_level(&mut self, _level: Option<f32>) {}
}
