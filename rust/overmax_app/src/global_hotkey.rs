//! Windows `RegisterHotKey` + message loop (same approach as Python `global_hotkey.py`).

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
};

fn vk_from_name(key: &str) -> Option<u32> {
    match key.trim().to_ascii_uppercase().as_str() {
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        _ => None,
    }
}

pub struct GlobalHotkey {
    stop_tx: Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl GlobalHotkey {
    pub fn spawn_toggle(key_name: &str, toggle: Arc<AtomicBool>) -> Option<Self> {
        let vk = vk_from_name(key_name)?;
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let join = std::thread::spawn(move || hotkey_loop(vk, toggle, stop_rx));
        Some(Self {
            stop_tx,
            join: Some(join),
        })
    }
}

impl Drop for GlobalHotkey {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn hotkey_loop(vk: u32, toggle: Arc<AtomicBool>, stop_rx: std::sync::mpsc::Receiver<()>) {
    const HOTKEY_ID: i32 = 1;
    let mods: HOT_KEY_MODIFIERS = 0;
    unsafe {
        if RegisterHotKey(null_mut(), HOTKEY_ID, mods, vk) == 0 {
            return;
        }
    }
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }
        unsafe {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_HOTKEY && msg.wParam == HOTKEY_ID as usize {
                    let cur = toggle.load(Ordering::Relaxed);
                    toggle.store(!cur, Ordering::Relaxed);
                }
                if msg.message == WM_QUIT {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    unsafe {
        UnregisterHotKey(null_mut(), HOTKEY_ID);
    }
}
