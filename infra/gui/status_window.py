"""Small Win32 status window for worker progress messages."""

from __future__ import annotations

import time
from dataclasses import dataclass

import win32api
import win32con
import win32gui

from infra.gui.theme import DEFAULT_FONT_FACE
from infra.gui.windowing import WindowCreateSpec, create_window, register_window_class

CLASS_NAME = "OvermaxWin32StatusWindow"
WINDOW_SIZE = (380, 132)
LABEL_MARGIN = 18
BACKGROUND_COLOR = win32api.GetSysColor(win32con.COLOR_BTNFACE)
TEXT_COLOR = win32api.GetSysColor(win32con.COLOR_BTNTEXT)


@dataclass(frozen=True)
class StatusWindowDiagnostics:
    hwnd_created: bool
    label_created: bool
    topmost: bool
    close_disabled: bool


class Win32StatusWindow:
    def __init__(self, title: str = "Overmax Update") -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.title = title
        self.hwnd = 0
        self._label_hwnd = 0
        self._font = 0
        self._background_brush = win32gui.CreateSolidBrush(BACKGROUND_COLOR)
        self._register_class()

    def show(self, message: str) -> bool:
        if not self._ensure_window():
            return False
        self.update(message)
        win32gui.ShowWindow(self.hwnd, win32con.SW_SHOWNORMAL)
        self.pump(120)
        return True

    def update(self, message: str) -> None:
        if not self._ensure_window():
            return
        win32gui.SetWindowText(self._label_hwnd, message)
        win32gui.InvalidateRect(self.hwnd, None, True)
        self.pump(80)

    def close(self) -> None:
        if self.hwnd:
            win32gui.DestroyWindow(self.hwnd)
            self.hwnd = 0
        if self._font:
            win32gui.DeleteObject(self._font)
            self._font = 0
        if self._background_brush:
            win32gui.DeleteObject(self._background_brush)
            self._background_brush = 0
        self._label_hwnd = 0
        self.pump(40)

    def pump(self, millis: int = 30) -> None:
        deadline = time.time() + max(0, millis) / 1000.0
        while time.time() < deadline:
            self._pump_waiting_messages()
            time.sleep(0.01)

    def diagnostics(self) -> StatusWindowDiagnostics:
        created = self._ensure_window()
        ex_style = win32gui.GetWindowLong(self.hwnd, win32con.GWL_EXSTYLE) if created else 0
        style = win32gui.GetWindowLong(self.hwnd, win32con.GWL_STYLE) if created else 0
        return StatusWindowDiagnostics(
            hwnd_created=bool(created and self.hwnd),
            label_created=bool(self._label_hwnd),
            topmost=(ex_style & win32con.WS_EX_TOPMOST) == win32con.WS_EX_TOPMOST,
            close_disabled=(style & win32con.WS_SYSMENU) == 0,
        )

    def _ensure_window(self) -> bool:
        if self.hwnd and win32gui.IsWindow(self.hwnd):
            return True
        self.hwnd = create_window(self.hinst, self._create_spec())
        if not self.hwnd:
            return False
        self._label_hwnd = self._create_label()
        self._font = self._create_font()
        win32gui.SendMessage(self._label_hwnd, win32con.WM_SETFONT, self._font, True)
        return bool(self._label_hwnd)

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title=self.title,
            ex_style=win32con.WS_EX_TOOLWINDOW,
            style=win32con.WS_POPUP | win32con.WS_CAPTION,
            position=_center_position(WINDOW_SIZE),
            size=WINDOW_SIZE,
        )

    def _create_label(self) -> int:
        left = LABEL_MARGIN
        top = LABEL_MARGIN
        width = WINDOW_SIZE[0] - LABEL_MARGIN * 2
        height = WINDOW_SIZE[1] - LABEL_MARGIN * 2 - 18
        return win32gui.CreateWindowEx(
            0,
            "STATIC",
            "",
            win32con.WS_CHILD | win32con.WS_VISIBLE | win32con.SS_LEFT,
            left,
            top,
            width,
            height,
            self.hwnd,
            0,
            self.hinst,
            None,
        )

    def _create_font(self) -> int:
        logfont = win32gui.LOGFONT()
        logfont.lfFaceName = DEFAULT_FONT_FACE
        logfont.lfHeight = -15
        logfont.lfWeight = win32con.FW_NORMAL
        logfont.lfQuality = win32con.CLEARTYPE_NATURAL_QUALITY
        return win32gui.CreateFontIndirect(logfont)

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_CLOSE:
            return 0
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._background_brush)
            return 1
        if msg == win32con.WM_CTLCOLORSTATIC:
            win32gui.SetBkColor(wparam, BACKGROUND_COLOR)
            win32gui.SetTextColor(wparam, TEXT_COLOR)
            return self._background_brush
        if msg == win32con.WM_DESTROY and hwnd == self.hwnd:
            self.hwnd = 0
            return 0
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _pump_waiting_messages(self) -> None:
        win32gui.PumpWaitingMessages()


def _center_position(size: tuple[int, int]) -> tuple[int, int]:
    width, height = size
    screen_width = win32api.GetSystemMetrics(win32con.SM_CXSCREEN)
    screen_height = win32api.GetSystemMetrics(win32con.SM_CYSCREEN)
    return ((screen_width - width) // 2, (screen_height - height) // 2)
