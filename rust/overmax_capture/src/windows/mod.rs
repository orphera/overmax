//! Windows capture backends (DXGI/GDI) and Win32 window tracking.

pub mod capture;
mod window_tracker;

pub use capture::{AdaptiveCaptureEngine, PreferredCaptureEngine};
pub use window_tracker::{
    encode_wide, find_hwnd_by_title, restore_foreground_by_title, WindowTracker,
};
