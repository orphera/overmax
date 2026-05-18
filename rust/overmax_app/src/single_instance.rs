//! Named mutex so only one Overmax process runs (matches Python `OvermaxSingleInstanceMutex`).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};

const MUTEX_NAME: &str = "OvermaxSingleInstanceMutex";

pub struct SingleInstanceGuard {
    handle: HANDLE,
}

impl SingleInstanceGuard {
    /// Returns `None` if another instance holds the mutex (shows message box, caller should exit).
    pub fn try_acquire() -> Option<Self> {
        let wide: Vec<u16> = OsStr::new(MUTEX_NAME)
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            let h = CreateMutexW(null(), 0, wide.as_ptr());
            if h.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                show_already_running();
                return None;
            }
            Some(Self { handle: h })
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn show_already_running() {
    const TITLE: &str = "Overmax";
    const MSG: &str = "이미 Overmax가 실행 중입니다. 기존 인스턴스를 종료한 뒤 다시 실행하세요.";
    let title: Vec<u16> = OsStr::new(TITLE).encode_wide().chain(Some(0)).collect();
    let msg: Vec<u16> = OsStr::new(MSG).encode_wide().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}
