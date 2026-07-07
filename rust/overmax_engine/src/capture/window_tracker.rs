#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowRect {
    #[allow(dead_code)]
    pub fn abs(self, rx: f32, ry: f32) -> (i32, i32) {
        (
            self.left + (self.width as f32 * rx) as i32,
            self.top + (self.height as f32 * ry) as i32,
        )
    }

    #[allow(dead_code)]
    pub fn abs_rect(self, rx1: f32, ry1: f32, rx2: f32, ry2: f32) -> WindowRect {
        let (left, top) = self.abs(rx1, ry1);
        let (right, bottom) = self.abs(rx2, ry2);
        WindowRect {
            left,
            top,
            width: right - left,
            height: bottom - top,
        }
    }

    pub fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{restore_foreground_by_title, WindowTracker};
#[cfg(target_os = "windows")]
pub use windows::{encode_wide, find_hwnd_by_title, restore_foreground_by_title, WindowTracker};

#[cfg(test)]
mod tests {
    use super::WindowRect;

    #[test]
    fn converts_ratio_points_to_absolute_pixels() {
        let rect = WindowRect {
            left: 100,
            top: 50,
            width: 1920,
            height: 1080,
        };

        assert_eq!(rect.abs(0.5, 0.25), (1060, 320));
    }

    #[test]
    fn converts_ratio_rect_to_capture_rect() {
        let rect = WindowRect {
            left: 10,
            top: 20,
            width: 100,
            height: 80,
        };

        assert_eq!(
            rect.abs_rect(0.1, 0.25, 0.6, 0.75),
            WindowRect {
                left: 20,
                top: 40,
                width: 50,
                height: 40,
            }
        );
    }
}
