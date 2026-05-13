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

pub struct WindowTracker {
    title: Vec<u16>,
}

impl WindowTracker {
    pub fn new(title: &str) -> Self {
        Self {
            title: encode_wide(title),
        }
    }

    pub fn game_rect(&self) -> Option<WindowRect> {
        self.find_hwnd().and_then(client_rect_for_hwnd)
    }

    pub fn is_foreground(&self) -> bool {
        let Some(hwnd) = self.find_hwnd() else {
            return false;
        };
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == hwnd }
    }

    fn find_hwnd(&self) -> Option<windows_sys::Win32::Foundation::HWND> {
        let hwnd = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
                std::ptr::null(),
                self.title.as_ptr(),
            )
        };
        (!hwnd.is_null()).then_some(hwnd)
    }
}

fn client_rect_for_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<WindowRect> {
    let mut rect = windows_sys::Win32::Foundation::RECT::default();
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    let ok = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) != 0
            && windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut point) != 0
    };
    let out = WindowRect {
        left: point.x,
        top: point.y,
        width: rect.right - rect.left,
        height: rect.bottom - rect.top,
    };
    (ok && out.is_valid()).then_some(out)
}

fn encode_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

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
