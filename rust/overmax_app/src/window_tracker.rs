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
        let fg = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if fg == hwnd {
            return true;
        }

        // 보조창(설정, 동기화 등)이 활성화된 경우에도 오버레이 동작을 유지하도록 함
        let mut fg_pid = 0u32;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, &mut fg_pid);
            let my_pid = windows_sys::Win32::System::Threading::GetCurrentProcessId();
            fg_pid == my_pid
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        let Some(hwnd) = self.find_hwnd() else {
            return false;
        };
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongW, GWL_STYLE, WS_POPUP};
            use windows_sys::Win32::Graphics::Gdi::{MonitorFromWindow, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST};
            
            let style = GetWindowLongW(hwnd, GWL_STYLE);
            if (style as u32 & WS_POPUP) == 0 {
                return false;
            }
            
            let mut rect = windows_sys::Win32::Foundation::RECT::default();
            if windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
            
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return false;
            }
            
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                rcMonitor: windows_sys::Win32::Foundation::RECT::default(),
                rcWork: windows_sys::Win32::Foundation::RECT::default(),
                dwFlags: 0,
            };
            
            if GetMonitorInfoW(monitor, &mut monitor_info) == 0 {
                return false;
            }
            
            let win_width = rect.right - rect.left;
            let win_height = rect.bottom - rect.top;
            let mon_width = monitor_info.rcMonitor.right - monitor_info.rcMonitor.left;
            let mon_height = monitor_info.rcMonitor.bottom - monitor_info.rcMonitor.top;
            
            win_width == mon_width && win_height == mon_height
        }
    }

    fn find_hwnd(&self) -> Option<windows_sys::Win32::Foundation::HWND> {
        find_hwnd_by_title(&self.title)
    }
}

pub fn restore_foreground_by_title(title: &str) -> bool {
    let title = encode_wide(title);
    let Some(hwnd) = find_hwnd_by_title(&title) else {
        return false;
    };
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd) != 0 }
}

pub fn find_hwnd_by_title(title: &[u16]) -> Option<windows_sys::Win32::Foundation::HWND> {
    let hwnd = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(std::ptr::null(), title.as_ptr())
    };
    (!hwnd.is_null()).then_some(hwnd)
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

pub fn encode_wide(value: &str) -> Vec<u16> {
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
