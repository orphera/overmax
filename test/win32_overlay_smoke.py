"""Minimal Win32 overlay spike.

This script is intentionally separate from production overlay code. It checks
whether a direct Win32 window can satisfy the main overlay window constraints
without pulling in Qt.
"""

from __future__ import annotations

import argparse
import ctypes
import sys

import win32api
import win32con
import win32gui

WDA_EXCLUDEFROMCAPTURE = 0x00000011
CLASS_NAME = "OvermaxWin32OverlaySmoke"
WINDOW_TITLE = "Overmax Win32 overlay smoke"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--show", action="store_true")
    return parser.parse_args()


def set_capture_exclusion(hwnd: int) -> bool:
    try:
        ctypes.windll.user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
        return True
    except Exception as exc:
        print(f"SetWindowDisplayAffinity failed: {exc}")
        return False


class Win32OverlaySmoke:
    def __init__(self) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self.capture_excluded = False
        self._register_class()

    def _register_class(self) -> None:
        wc = win32gui.WNDCLASS()
        wc.hInstance = self.hinst
        wc.lpszClassName = CLASS_NAME
        wc.lpfnWndProc = self._wnd_proc
        wc.hCursor = win32gui.LoadCursor(0, win32con.IDC_ARROW)
        wc.hbrBackground = win32con.COLOR_WINDOW + 1

        try:
            win32gui.RegisterClass(wc)
        except win32gui.error:
            # Re-running the smoke in the same interpreter can reuse the class.
            pass

    def create(self) -> int:
        ex_style = (
            win32con.WS_EX_LAYERED
            | win32con.WS_EX_TOPMOST
            | win32con.WS_EX_TOOLWINDOW
            | win32con.WS_EX_NOACTIVATE
        )
        self.hwnd = win32gui.CreateWindowEx(
            ex_style,
            CLASS_NAME,
            WINDOW_TITLE,
            win32con.WS_POPUP,
            120,
            120,
            360,
            170,
            0,
            0,
            self.hinst,
            None,
        )
        win32gui.SetLayeredWindowAttributes(self.hwnd, 0, 232, win32con.LWA_ALPHA)
        self.capture_excluded = set_capture_exclusion(self.hwnd)
        return self.hwnd

    def show_for(self, duration_ms: int) -> int:
        hwnd = self.create()
        win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
        win32gui.UpdateWindow(hwnd)
        ctypes.windll.user32.SetTimer(hwnd, 1, duration_ms, None)
        print(f"capture_excluded={self.capture_excluded}")
        return self._message_loop()

    def _message_loop(self) -> int:
        msg = win32gui.GetMessage(None, 0, 0)
        while msg[0] != 0:
            win32gui.TranslateMessage(msg[1])
            win32gui.DispatchMessage(msg[1])
            msg = win32gui.GetMessage(None, 0, 0)
        return 0

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_PAINT:
            self._paint(hwnd)
            return 0
        if msg == win32con.WM_TIMER:
            win32gui.DestroyWindow(hwnd)
            return 0
        if msg == win32con.WM_NCHITTEST:
            return win32con.HTCAPTION
        if msg == win32con.WM_DESTROY:
            win32gui.PostQuitMessage(0)
            return 0
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _paint(self, hwnd: int) -> None:
        hdc, paint_struct = win32gui.BeginPaint(hwnd)
        try:
            self._draw_panel(hdc)
        finally:
            win32gui.EndPaint(hwnd, paint_struct)

    def _draw_panel(self, hdc: int) -> None:
        brush = win32gui.CreateSolidBrush(win32api.RGB(18, 24, 38))
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, 8, 8, 352, 162, 24, 24)
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)

        self._draw_text(hdc, "Win32 overlay smoke", 24, 24, 330, 58)
        self._draw_text(hdc, "topmost / layered / noactivate", 24, 70, 330, 104)
        self._draw_text(hdc, "capture exclusion requested", 24, 112, 330, 144)

    def _draw_text(self, hdc: int, text: str, left: int, top: int, right: int, bottom: int) -> None:
        win32gui.SetBkMode(hdc, win32con.TRANSPARENT)
        win32gui.SetTextColor(hdc, win32api.RGB(230, 236, 255))
        win32gui.DrawText(hdc, text, -1, (left, top, right, bottom), win32con.DT_SINGLELINE)


def main() -> int:
    args = parse_args()

    if args.import_only:
        print("Win32 import ok")
        return 0

    if not args.show:
        print("Use --import-only or --show")
        return 2

    return Win32OverlaySmoke().show_for(2000)


if __name__ == "__main__":
    raise SystemExit(main())
