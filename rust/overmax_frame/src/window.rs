#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

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
