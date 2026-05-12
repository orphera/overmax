"""Native Win32 system tray icon implementation."""

from __future__ import annotations

import threading
import win32api
import win32con
import win32gui
from typing import Callable, NamedTuple

WM_TRAYICON = win32con.WM_USER + 20


class TrayMenuItem(NamedTuple):
    text: str
    callback: Callable[[], None]
    is_default: bool = False


class Win32TrayIcon:
    def __init__(
        self,
        tooltip: str,
        menu_items: list[TrayMenuItem],
        on_double_click: Callable[[], None] | None = None,
    ) -> None:
        self._tooltip = tooltip
        self._menu_items = menu_items
        self._on_double_click = on_double_click
        self._hwnd = 0
        self._hicon = 0
        self._running = False
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._thread = threading.Thread(target=self._run_loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._running = False
        if self._hwnd:
            win32gui.PostMessage(self._hwnd, win32con.WM_CLOSE, 0, 0)

    def _run_loop(self) -> None:
        hinst = win32api.GetModuleHandle(None)
        
        # Register window class for the hidden message window
        wc = win32gui.WNDCLASS()
        wc.hInstance = hinst
        wc.lpszClassName = "OvermaxTrayIconHolder"
        wc.lpfnWndProc = self._wnd_proc
        
        try:
            class_atom = win32gui.RegisterClass(wc)
        except Exception:
            class_atom = "OvermaxTrayIconHolder"

        self._hwnd = win32gui.CreateWindow(
            class_atom,
            "OvermaxTrayIcon",
            0, 0, 0, 0, 0,
            0, 0, hinst, None
        )
        
        # Load default icon
        self._hicon = win32gui.LoadIcon(0, win32con.IDI_APPLICATION)
        
        # Add tray icon
        nid = (self._hwnd, 0, win32gui.NIF_ICON | win32gui.NIF_MESSAGE | win32gui.NIF_TIP,
               WM_TRAYICON, self._hicon, self._tooltip)
        win32gui.Shell_NotifyIcon(win32gui.NIM_ADD, nid)
        
        win32gui.PumpMessages()
        
        # Cleanup
        win32gui.Shell_NotifyIcon(win32gui.NIM_DELETE, (self._hwnd, 0))
        win32gui.DestroyWindow(self._hwnd)

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == WM_TRAYICON:
            if lparam == win32con.WM_LBUTTONDBLCLK:
                if self._on_double_click:
                    self._on_double_click()
            elif lparam == win32con.WM_RBUTTONUP:
                self._show_menu()
            return 0
        
        if msg == win32con.WM_COMMAND:
            id = win32api.LOWORD(wparam)
            if 0 <= id < len(self._menu_items):
                self._menu_items[id].callback()
            return 0
            
        if msg == win32con.WM_DESTROY:
            win32gui.PostQuitMessage(0)
            return 0
            
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _show_menu(self) -> None:
        menu = win32gui.CreatePopupMenu()
        for i, item in enumerate(self._menu_items):
            if not item.text:
                win32gui.AppendMenu(menu, win32con.MF_SEPARATOR, 0, "")
                continue
            
            flags = win32con.MF_STRING
            if item.is_default:
                flags |= win32con.MF_DEFAULT
            win32gui.AppendMenu(menu, flags, i, item.text)

        pos = win32gui.GetCursorPos()
        # SetForegroundWindow is required for the menu to close on focus loss
        win32gui.SetForegroundWindow(self._hwnd)
        win32gui.TrackPopupMenu(menu, win32con.TPM_LEFTALIGN, pos[0], pos[1], 0, self._hwnd, None)
        win32gui.PostMessage(self._hwnd, win32con.WM_NULL, 0, 0)
