//! OS 대화상자(파일 선택·저장, 메시지 박스)를 지금 활성화된 Overmax 창에 소속시킨다.
//!
//! 설정·디버그 창은 게임 위에 보이도록 always-on-top이다. 소유자 없이 띄운 OS 대화상자는
//! 그 아래에 깔려 보이지 않는다. Windows에서 topmost 창이 소유한 창은 함께 topmost가 되므로,
//! 클릭 직후의 전경 창(= 버튼을 누른 Overmax 창)을 부모로 지정하면 대화상자가 그 위에 뜬다.

/// 부모가 지정된 파일 대화상자
pub fn file_dialog() -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new();
    match owner() {
        Some(owner) => dialog.set_parent(&owner),
        None => dialog,
    }
}

/// 부모가 지정된 메시지 대화상자
pub fn message_dialog() -> rfd::MessageDialog {
    let dialog = rfd::MessageDialog::new();
    match owner() {
        Some(owner) => dialog.set_parent(&owner),
        None => dialog,
    }
}

#[cfg(windows)]
fn owner() -> Option<windows_owner::Owner> {
    windows_owner::foreground_window_of_this_process()
}

#[cfg(not(windows))]
fn owner() -> Option<NoOwner> {
    None
}

#[cfg(not(windows))]
enum NoOwner {}

#[cfg(not(windows))]
impl raw_window_handle::HasWindowHandle for NoOwner {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        match *self {}
    }
}

#[cfg(not(windows))]
impl raw_window_handle::HasDisplayHandle for NoOwner {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        match *self {}
    }
}

#[cfg(windows)]
mod windows_owner {
    use std::num::NonZeroIsize;

    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
        Win32WindowHandle, WindowHandle,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    pub(super) struct Owner(NonZeroIsize);

    /// 전경 창이 이 프로세스의 창일 때만 돌려준다. 다른 프로그램 창을 부모로 잡지 않기 위함이다.
    pub(super) fn foreground_window_of_this_process() -> Option<Owner> {
        // SAFETY: 인자 없는 Win32 조회 함수이며, 반환 핸들은 소유권 없이 읽기만 한다.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != GetCurrentProcessId() {
                return None;
            }
            NonZeroIsize::new(hwnd as isize).map(Owner)
        }
    }

    impl HasWindowHandle for Owner {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw = RawWindowHandle::Win32(Win32WindowHandle::new(self.0));
            // SAFETY: 대화상자가 떠 있는 동안 이 창은 같은 UI 스레드에서 살아 있다.
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    impl HasDisplayHandle for Owner {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Ok(DisplayHandle::windows())
        }
    }
}
