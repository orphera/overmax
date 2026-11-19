mod capture_engine;
#[cfg(target_os = "linux")]
mod linux;
mod window_tracker;
#[cfg(target_os = "windows")]
mod windows;

pub use capture_engine::{CaptureEngine, CaptureErrorAction};
pub use window_tracker::{FocusState, PresentationObservation, SharedPresentationObservation};

#[cfg(all(target_os = "linux", feature = "portal-capture"))]
pub use linux::portal::{
    connection::{Confirmation, ConnectionStatus, PortalConnection, PortalControl, ResetScope},
    PortalCapture, PortalStatus,
};
#[cfg(target_os = "linux")]
pub use linux::{
    restore_foreground_by_title, select_geometry_source, spawn_game_geometry_source,
    AdaptiveCaptureEngine, FocusObservation, FocusSource, GameGeometry, GameGeometrySource,
    GeometryChangedCallback, NoGeometrySource, SharedGameGeometry, WindowTracker,
};
#[cfg(target_os = "windows")]
pub use windows::{
    capture::hdr_pipeline, encode_wide, find_hwnd_by_title, restore_foreground_by_title,
    AdaptiveCaptureEngine, PreferredCaptureEngine, WindowTracker,
};
