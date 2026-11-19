pub mod capture_engine;
pub use overmax_frame::captured_frame as frame;
pub use overmax_frame::utils as frame_utils;
#[cfg(all(target_os = "linux", feature = "portal-capture"))]
pub mod portal;
pub mod window_tracker;

#[cfg(target_os = "windows")]
pub use capture_engine::{windows::hdr_pipeline, PreferredCaptureEngine};
pub use capture_engine::{AdaptiveCaptureEngine, CaptureEngine, CaptureErrorAction};
#[cfg(all(target_os = "linux", feature = "portal-capture"))]
pub use portal::{
    connection::{Confirmation, ConnectionStatus, PortalConnection, PortalControl, ResetScope},
    PortalCapture, PortalStatus,
};
#[cfg(target_os = "windows")]
pub use window_tracker::{encode_wide, find_hwnd_by_title};
pub use window_tracker::{
    restore_foreground_by_title, FocusState, PresentationObservation,
    SharedPresentationObservation, WindowTracker,
};
#[cfg(target_os = "linux")]
pub use window_tracker::{
    spawn_game_geometry_source, FocusObservation, FocusSource, GameGeometry, SharedGameGeometry,
};
