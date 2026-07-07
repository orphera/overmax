use super::WindowRect;

pub struct WindowTracker;

impl WindowTracker {
    pub fn new(_title: &str) -> Self {
        Self
    }

    pub fn game_rect(&self) -> Option<WindowRect> {
        None
    }

    pub fn is_foreground(&self) -> bool {
        false
    }

    pub fn is_fullscreen(&self) -> bool {
        false
    }
}

pub fn restore_foreground_by_title(_title: &str) -> bool {
    false
}
