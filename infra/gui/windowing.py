"""Thin Win32 window helpers shared by display surfaces."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
from typing import Callable

import win32api
import win32con
import win32gui

WDA_EXCLUDEFROMCAPTURE = 0x00000011


@dataclass(frozen=True)
class WindowCreateSpec:
    class_name: str
    title: str
    ex_style: int
    style: int
    position: tuple[int, int]
    size: tuple[int, int]
    parent: int = 0


def register_window_class(
    hinst: int,
    class_name: str,
    wnd_proc: Callable[[int, int, int, int], int],
) -> None:
    wc = win32gui.WNDCLASS()
    wc.hInstance = hinst
    wc.lpszClassName = class_name
    wc.lpfnWndProc = wnd_proc
    wc.hCursor = win32gui.LoadCursor(0, win32con.IDC_ARROW)
    wc.hbrBackground = 0
    try:
        win32gui.RegisterClass(wc)
    except win32gui.error:
        pass


def create_window(hinst: int, spec: WindowCreateSpec) -> int:
    x, y = spec.position
    width, height = spec.size
    return win32gui.CreateWindowEx(
        spec.ex_style,
        spec.class_name,
        spec.title,
        spec.style,
        x,
        y,
        width,
        height,
        spec.parent,
        0,
        hinst,
        None,
    )


def run_message_loop() -> int:
    msg = win32gui.GetMessage(None, 0, 0)
    while msg[0] != 0:
        win32gui.TranslateMessage(msg[1])
        win32gui.DispatchMessage(msg[1])
        msg = win32gui.GetMessage(None, 0, 0)
    return 0


def set_capture_exclusion(hwnd: int) -> bool:
    try:
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        if user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE):
            return True
        error = ctypes.get_last_error()
        print(f"SetWindowDisplayAffinity failed: winerror={error}")
    except Exception as exc:
        print(f"SetWindowDisplayAffinity failed: {exc}")
    return False


def get_monitor_rect(hwnd: int) -> tuple[int, int, int, int]:
    monitor = win32api.MonitorFromWindow(hwnd, win32con.MONITOR_DEFAULTTONEAREST)
    info = win32api.GetMonitorInfo(monitor)
    return tuple(info["Monitor"])


def has_ex_style(ex_style: int, flag: int) -> bool:
    return (ex_style & flag) == flag


def required_styles_present(hwnd: int, required: int) -> bool:
    ex_style = win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE)
    return (ex_style & required) == required


def foreground_preserved_by_show(hwnd: int, can_show: bool) -> bool:
    if not can_show:
        return False
    before = win32gui.GetForegroundWindow()
    win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
    win32gui.UpdateWindow(hwnd)
    after = win32gui.GetForegroundWindow()
    win32gui.ShowWindow(hwnd, win32con.SW_HIDE)
    return before == after
