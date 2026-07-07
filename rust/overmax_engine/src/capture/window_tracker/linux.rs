use super::WindowRect;

pub struct WindowTracker {
    _title: String,
}

impl WindowTracker {
    pub fn new(title: &str) -> Self {
        Self {
            _title: title.to_string(),
        }
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
