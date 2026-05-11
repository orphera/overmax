"""Minimal Win32 overlay spike.

This script is intentionally separate from production overlay code. It checks
whether a direct Win32 window can satisfy the main overlay window constraints
without pulling in Qt.
"""

from __future__ import annotations

import argparse
import ctypes
import sys
from dataclasses import dataclass

import win32api
import win32con
import win32gui

WDA_EXCLUDEFROMCAPTURE = 0x00000011
CLASS_NAME = "OvermaxWin32OverlaySmoke"
WINDOW_TITLE = "Overmax Win32 overlay smoke"
DEFAULT_DURATION_MS = 3000


@dataclass(frozen=True)
class WindowDiagnostics:
    capture_excluded: bool
    dpi: int
    monitor: tuple[int, int, int, int]
    rect: tuple[int, int, int, int]
    style_ok: bool
    ex_style: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--show", action="store_true")
    parser.add_argument("--duration-ms", type=int, default=DEFAULT_DURATION_MS)
    return parser.parse_args()


def set_process_dpi_awareness() -> None:
    try:
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except Exception:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            pass


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
        self._font = 0
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

    def diagnostics(self) -> WindowDiagnostics:
        hwnd = self.create()
        return WindowDiagnostics(
            capture_excluded=self.capture_excluded,
            dpi=self._get_window_dpi(hwnd),
            monitor=self._get_monitor_rect(hwnd),
            rect=win32gui.GetWindowRect(hwnd),
            style_ok=self._required_styles_present(hwnd),
            ex_style=win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE),
        )

    def show_for(self, duration_ms: int) -> int:
        hwnd = self.create()
        win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
        win32gui.UpdateWindow(hwnd)
        ctypes.windll.user32.SetTimer(hwnd, 1, duration_ms, None)
        print(f"capture_excluded={self.capture_excluded}")
        print(f"dpi={self._get_window_dpi(hwnd)}")
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
        if msg == win32con.WM_SETCURSOR:
            win32gui.SetCursor(win32gui.LoadCursor(0, win32con.IDC_SIZEALL))
            return 1
        if msg == win32con.WM_DESTROY:
            self._destroy_font()
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

        self._draw_text(hdc, "RESPECT V", 24, 22, 125, 46, win32api.RGB(255, 209, 102))
        self._draw_badge(hdc, "6B MX", 270, 20, 334, 48)
        self._draw_text(hdc, "Win32 overlay smoke", 24, 50, 330, 78)
        self._draw_text(hdc, "01  sample recommendation", 32, 88, 330, 112)
        self._draw_text(hdc, "02  drag / noactivate / topmost", 32, 116, 330, 140)
        self._draw_footer(hdc)

    def _draw_badge(self, hdc: int, text: str, left: int, top: int, right: int, bottom: int) -> None:
        brush = win32gui.CreateSolidBrush(win32api.RGB(46, 68, 118))
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, left, top, right, bottom, 12, 12)
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)
        self._draw_text(hdc, text, left + 8, top + 4, right, bottom)

    def _draw_footer(self, hdc: int) -> None:
        pen = win32gui.CreatePen(win32con.PS_SOLID, 1, win32api.RGB(48, 58, 78))
        old_pen = win32gui.SelectObject(hdc, pen)
        try:
            win32gui.MoveToEx(hdc, 24, 148)
            win32gui.LineTo(hdc, 334, 148)
        finally:
            win32gui.SelectObject(hdc, old_pen)
            win32gui.DeleteObject(pen)
        self._draw_text(hdc, "capture excluded / move by dragging", 24, 150, 330, 166)

    def _draw_text(
        self,
        hdc: int,
        text: str,
        left: int,
        top: int,
        right: int,
        bottom: int,
        color: int = win32api.RGB(230, 236, 255),
    ) -> None:
        self._select_font(hdc)
        win32gui.SetBkMode(hdc, win32con.TRANSPARENT)
        win32gui.SetTextColor(hdc, color)
        win32gui.DrawText(hdc, text, -1, (left, top, right, bottom), win32con.DT_SINGLELINE)

    def _select_font(self, hdc: int) -> None:
        if not self._font:
            logfont = win32gui.LOGFONT()
            logfont.lfFaceName = "Segoe UI"
            logfont.lfHeight = -15
            logfont.lfWeight = win32con.FW_SEMIBOLD
            logfont.lfQuality = win32con.CLEARTYPE_QUALITY
            self._font = win32gui.CreateFontIndirect(logfont)
        win32gui.SelectObject(hdc, self._font)

    def _destroy_font(self) -> None:
        if self._font:
            win32gui.DeleteObject(self._font)
            self._font = 0

    def _get_window_dpi(self, hwnd: int) -> int:
        try:
            return int(ctypes.windll.user32.GetDpiForWindow(hwnd))
        except Exception:
            return 96

    def _get_monitor_rect(self, hwnd: int) -> tuple[int, int, int, int]:
        monitor = win32api.MonitorFromWindow(hwnd, win32con.MONITOR_DEFAULTTONEAREST)
        info = win32api.GetMonitorInfo(monitor)
        return tuple(info["Monitor"])

    def _required_styles_present(self, hwnd: int) -> bool:
        ex_style = win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE)
        required = (
            win32con.WS_EX_LAYERED
            | win32con.WS_EX_TOPMOST
            | win32con.WS_EX_TOOLWINDOW
            | win32con.WS_EX_NOACTIVATE
        )
        return (ex_style & required) == required


def print_diagnostics(diagnostics: WindowDiagnostics) -> None:
    print(f"capture_excluded={diagnostics.capture_excluded}")
    print(f"style_ok={diagnostics.style_ok}")
    print(f"dpi={diagnostics.dpi}")
    print(f"rect={diagnostics.rect}")
    print(f"monitor={diagnostics.monitor}")
    print(f"ex_style=0x{diagnostics.ex_style:08X}")


def main() -> int:
    args = parse_args()

    if args.import_only:
        print("Win32 import ok")
        return 0

    set_process_dpi_awareness()

    if args.diagnostics:
        print_diagnostics(Win32OverlaySmoke().diagnostics())
        return 0

    if not args.show:
        print("Use --import-only, --diagnostics, or --show")
        return 2

    return Win32OverlaySmoke().show_for(args.duration_ms)


if __name__ == "__main__":
    raise SystemExit(main())
