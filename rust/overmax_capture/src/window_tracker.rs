pub use overmax_frame::{WindowRect, WindowSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusState {
    Focused,
    Background,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationObservation {
    /// Local observation lifetime, not a Portal, X11 or cross-client window ID.
    /// Changes when the unique target changes or becomes unavailable/ambiguous.
    pub target_epoch: u64,
    pub focus: FocusState,
    pub fullscreen: Option<bool>,
    pub generation: u64,
    pub committed_at: std::time::Instant,
}

pub type SharedPresentationObservation =
    std::sync::Arc<std::sync::Mutex<Option<PresentationObservation>>>;

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
