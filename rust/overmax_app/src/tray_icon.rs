//! Windows system tray icon for the native Rust app.

use crate::ui_command::UiCommand;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    HMENU, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, MSG, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_RBUTTONUP,
    WNDCLASSW,
};

const TRAY_ID: u32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;
const CMD_SETTINGS: usize = 1002;
const CMD_SYNC: usize = 1003;
const CMD_DEBUG: usize = 1004;
const CMD_EXIT: usize = 1005;

static ACTIONS: OnceLock<TrayActions> = OnceLock::new();

pub struct TrayIcon {
    hwnd: Arc<AtomicIsize>,
    thread: Option<JoinHandle<()>>,
}

struct TrayActions {
    command_tx: Sender<UiCommand>,
}

impl TrayIcon {
    pub fn spawn(command_tx: Sender<UiCommand>) -> Self {
        let _ = ACTIONS.set(TrayActions { command_tx });
        let hwnd = Arc::new(AtomicIsize::new(0));
        let thread_hwnd = hwnd.clone();
        let thread = thread::spawn(move || unsafe {
            run_tray_loop(thread_hwnd);
        });
        Self {
            hwnd,
            thread: Some(thread),
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let hwnd = self.hwnd.load(Ordering::Relaxed);
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
            }
        }
        if hwnd != 0 {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        } else if let Some(thread) = self.thread.take() {
            drop(thread);
        }
    }
}

unsafe fn run_tray_loop(shared_hwnd: Arc<AtomicIsize>) {
    let class_name = wide("OvermaxTrayWindow");
    let hinstance = GetModuleHandleW(null());
    let wnd = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    RegisterClassW(&wnd);

    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        class_name.as_ptr(),
        0,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        null_mut(),
        null_mut(),
        hinstance,
        null_mut(),
    );
    if hwnd.is_null() {
        return;
    }
    shared_hwnd.store(hwnd as isize, Ordering::Relaxed);
    add_notify_icon(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    delete_notify_icon(hwnd);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        TRAY_CALLBACK => {
            handle_tray_event(hwnd, (lparam & 0xffff) as u32);
            0
        }
        WM_COMMAND => {
            handle_menu_command(wparam & 0xffff);
            0
        }
        WM_CLOSE => {
            delete_notify_icon(hwnd);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn add_notify_icon(hwnd: HWND) {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
        ..Default::default()
    };
    write_wide_fixed(&mut data.szTip, "Overmax");
    Shell_NotifyIconW(NIM_ADD, &data);
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &data);
}

unsafe fn delete_notify_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };
    Shell_NotifyIconW(NIM_DELETE, &data);
}

unsafe fn handle_tray_event(hwnd: HWND, event: u32) {
    if event == WM_RBUTTONUP {
        show_context_menu(hwnd);
    }
}

unsafe fn show_context_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    append_item(menu, CMD_SETTINGS, "설정");
    append_item(menu, CMD_SYNC, "V-Archive 동기화");
    append_item(menu, CMD_DEBUG, "디버그 로그");
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append_item(menu, CMD_EXIT, "종료");

    let mut point = POINT::default();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    if cmd > 0 {
        handle_menu_command(cmd as usize);
    }
}

unsafe fn append_item(menu: HMENU, id: usize, label: &str) {
    let text = wide(label);
    AppendMenuW(menu, MF_STRING, id, text.as_ptr());
}

fn handle_menu_command(cmd: usize) {
    let Some(actions) = ACTIONS.get() else {
        return;
    };
    match cmd {
        CMD_SETTINGS => send_command(actions, UiCommand::OpenSettings),
        CMD_SYNC => send_command(actions, UiCommand::OpenSync),
        CMD_DEBUG => send_command(actions, UiCommand::OpenDebug),
        CMD_EXIT => send_command(actions, UiCommand::Exit),
        _ => {}
    }
}

fn send_command(actions: &TrayActions, command: UiCommand) {
    let _ = actions.command_tx.send(command);
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn write_wide_fixed<const N: usize>(target: &mut [u16; N], text: &str) {
    let source = wide(text);
    let copy_len = source.len().min(N);
    target[..copy_len].copy_from_slice(&source[..copy_len]);
}
