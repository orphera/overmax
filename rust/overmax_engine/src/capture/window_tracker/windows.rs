use super::WindowRect;

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
            use windows_sys::Win32::Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongW, GWL_STYLE, WS_POPUP,
            };

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
