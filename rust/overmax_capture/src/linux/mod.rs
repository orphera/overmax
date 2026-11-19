//! Linux capture backends (XComposite/SHM, optional xdg-desktop-portal) and
//! X11/Wayland window tracking.

mod capture_engine;
mod geometry_source;
#[cfg(feature = "portal-capture")]
pub mod portal;
mod window_tracker;

pub use capture_engine::AdaptiveCaptureEngine;
pub use geometry_source::{
    select_geometry_source, spawn_game_geometry_source, GameGeometry, GameGeometrySource,
    GeometryChangedCallback, NoGeometrySource, SharedGameGeometry,
};
pub use window_tracker::{
    restore_foreground_by_title, FocusObservation, FocusSource, WindowTracker,
};
