#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusState {
    Focused,
    Background,
    Unknown,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusSource {
    WaylandForeignToplevel,
    EwmhFocusedState,
    EwmhActiveWindow,
    XInputFocus,
    None,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusObservation {
    pub state: FocusState,
    pub source: FocusSource,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationObservation {
    pub focus: FocusState,
    pub fullscreen: Option<bool>,
    pub generation: u64,
    pub committed_at: std::time::Instant,
}

pub type SharedPresentationObservation =
    std::sync::Arc<std::sync::Mutex<Option<PresentationObservation>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowSnapshot {
    pub window: u64,
    pub rect: WindowRect,
    pub foreground: bool,
    pub fullscreen: bool,
}

impl WindowRect {
    pub fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[cfg(target_os = "linux")]
mod geometry_source;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use geometry_source::{
    select_geometry_source, spawn_game_geometry_source, GameGeometry, GameGeometrySource,
    GeometryChangedCallback, NoGeometrySource, SharedGameGeometry,
};
#[cfg(target_os = "linux")]
pub use linux::{restore_foreground_by_title, WindowTracker};
#[cfg(target_os = "windows")]
pub use windows::{encode_wide, find_hwnd_by_title, restore_foreground_by_title, WindowTracker};

#[cfg(test)]
mod tests {
    use super::WindowRect;

    #[test]
    fn test_window_rect_validity() {
        let valid_rect = WindowRect {
            left: 100,
            top: 50,
            width: 1920,
            height: 1080,
        };
        assert!(valid_rect.is_valid());

        let zero_rect = WindowRect {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
        };
        assert!(!zero_rect.is_valid());
    }
}
