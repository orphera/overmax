pub mod capture_engine;
pub use overmax_frame::captured_frame as frame;
pub use overmax_frame::utils as frame_utils;
#[cfg(all(target_os = "linux", feature = "portal-capture"))]
pub mod portal;
pub mod window_tracker;
