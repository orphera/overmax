"""Win32 debug log window candidate."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Optional

import win32api
import win32con
import win32gui

from infra.gui.theme import DEFAULT_FONT_FACE
from infra.gui.windowing import WindowCreateSpec, create_window, register_window_class
from infra.gui.windowing import set_capture_exclusion
from settings import SETTINGS

CLASS_NAME = "OvermaxWin32DebugWindow"
WINDOW_SIZE = (700, 400)
FILTER_TAGS = ("[ScreenCapture]", "[Overlay]", "[VArchive]", "[WindowTracker]", "[Main]")
BUTTON_IDS = {"pause": 1001, "clear": 1002, "roi": 1003}
FILTER_BASE_ID = 1100
WINDOW_BG = win32api.RGB(0xF3, 0xF4, 0xF6)
LOG_BG = win32api.RGB(0xFF, 0xFF, 0xFF)
TEXT_COLOR = win32api.RGB(0x1F, 0x29, 0x37)
CONTROL_GAP = 8
BUTTON_PADDING_X = 24
CHECK_PADDING_X = 30
MIN_BUTTON_WIDTH = 72
MIN_CHECK_WIDTH = 86


@dataclass(frozen=True)
class DebugWindowDiagnostics:
    hwnd_created: bool
    edit_created: bool
    capture_excluded: bool
    filter_count: int
    max_lines: int


class Win32DebugWindow:
    def __init__(self) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self._edit_hwnd = 0
        self._status_hwnd = 0
        self._pause_hwnd = 0
        self._clear_hwnd = 0
        self._roi_hwnd = 0
        self._filter_hwnds: dict[str, int] = {}
        self._font = 0
        self._window_brush = win32gui.CreateSolidBrush(WINDOW_BG)
        self._log_brush = win32gui.CreateSolidBrush(LOG_BG)
        self._paused = False
        self._line_count = 0
        self._lines: list[str] = []
        self._capture_excluded = False
        self._roi_toggle_cb: Optional[Callable[[bool], None]] = None
        self._register_class()

    def show(self) -> None:
        if not self._ensure_window():
            return
        win32gui.ShowWindow(self.hwnd, win32con.SW_SHOWNORMAL)
        win32gui.SetForegroundWindow(self.hwnd)

    def hide(self) -> None:
        if self.hwnd:
            win32gui.ShowWindow(self.hwnd, win32con.SW_HIDE)

    def is_visible(self) -> bool:
        return bool(self.hwnd and win32gui.IsWindowVisible(self.hwnd))

    def toggle(self) -> None:
        if self.is_visible():
            self.hide()
        else:
            self.show()

    def append_log(self, message: str) -> None:
        if self._paused or not self._tag_visible(message):
            return
        if not self._ensure_window():
            return
        self._lines.append(message)
        self._trim_lines()
        self._line_count = len(self._lines)
        self._refresh_log_text()
        self._set_status(f"총 {self._line_count}줄")

    def set_roi_toggle_callback(self, callback: Optional[Callable[[bool], None]]) -> None:
        self._roi_toggle_cb = callback
        if self._ensure_window():
            enabled = bool(callback)
            win32gui.EnableWindow(self._roi_hwnd, enabled)
            if not enabled:
                self._set_button_checked(self._roi_hwnd, False)
                self._set_roi_text(False)

    def diagnostics(self) -> DebugWindowDiagnostics:
        created = self._ensure_window()
        return DebugWindowDiagnostics(
            hwnd_created=bool(created and self.hwnd),
            edit_created=bool(self._edit_hwnd),
            capture_excluded=self._capture_excluded,
            filter_count=len(self._filter_hwnds),
            max_lines=self._max_lines(),
        )

    def _ensure_window(self) -> bool:
        if self.hwnd and win32gui.IsWindow(self.hwnd):
            return True
        self.hwnd = create_window(self.hinst, self._create_spec())
        if not self.hwnd:
            return False
        self._font = _create_font()
        self._create_controls()
        self._capture_excluded = set_capture_exclusion(self.hwnd)
        self._set_status("대기 중...")
        return True

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title=str(SETTINGS["debug_window"]["title"]),
            ex_style=win32con.WS_EX_TOPMOST | win32con.WS_EX_TOOLWINDOW,
            style=win32con.WS_OVERLAPPED | win32con.WS_CAPTION | win32con.WS_SYSMENU,
            position=_center_position(WINDOW_SIZE),
            size=WINDOW_SIZE,
        )

    def _create_controls(self) -> None:
        pause_width = self._button_width("일시정지")
        clear_width = self._button_width("지우기")
        clear_x = WINDOW_SIZE[0] - 52 - clear_width
        pause_x = clear_x - CONTROL_GAP - pause_width
        self._pause_hwnd = _button(self.hwnd, self.hinst, "일시정지", BUTTON_IDS["pause"], pause_x, 10, pause_width, 28)
        self._clear_hwnd = _button(self.hwnd, self.hinst, "지우기", BUTTON_IDS["clear"], clear_x, 10, clear_width, 28)
        self._roi_hwnd = _check(self.hwnd, self.hinst, "ROI 표시 OFF", BUTTON_IDS["roi"], 14, 42, self._check_width("ROI 표시 OFF"), 24)
        win32gui.EnableWindow(self._roi_hwnd, False)
        self._create_filters()
        self._edit_hwnd = _edit(self.hwnd, self.hinst, 12, 72, 660, 258)
        self._status_hwnd = _static(self.hwnd, self.hinst, "", 14, 338, 250, 22)
        for hwnd in self._all_child_hwnds():
            win32gui.SendMessage(hwnd, win32con.WM_SETFONT, self._font, True)

    def _create_filters(self) -> None:
        x = 14 + self._check_width("ROI 표시 OFF") + CONTROL_GAP
        for index, tag in enumerate(FILTER_TAGS):
            text = tag.strip("[]")
            width = self._check_width(text)
            hwnd = _check(self.hwnd, self.hinst, text, FILTER_BASE_ID + index, x, 42, width, 24)
            self._set_button_checked(hwnd, True)
            self._filter_hwnds[tag] = hwnd
            x += width + CONTROL_GAP

    def _all_child_hwnds(self) -> list[int]:
        return [
            self._pause_hwnd,
            self._clear_hwnd,
            self._roi_hwnd,
            self._edit_hwnd,
            self._status_hwnd,
            *self._filter_hwnds.values(),
        ]

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_COMMAND:
            self._handle_command(win32api.LOWORD(wparam))
            return 0
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._window_brush)
            return 1
        if msg == win32con.WM_CTLCOLORSTATIC and lparam == self._edit_hwnd:
            return self._paint_control_background(wparam, self._log_brush, LOG_BG)
        if msg in (win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return self._paint_control_background(wparam, self._window_brush, WINDOW_BG)
        if msg == win32con.WM_CTLCOLOREDIT:
            return self._paint_control_background(wparam, self._log_brush, LOG_BG)
        if msg == win32con.WM_DESTROY and hwnd == self.hwnd:
            self.hwnd = 0
            return 0
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _paint_control_background(self, hdc: int, brush: int, color: int) -> int:
        win32gui.SetBkColor(hdc, color)
        win32gui.SetTextColor(hdc, TEXT_COLOR)
        return brush

    def _handle_command(self, control_id: int) -> None:
        if control_id == BUTTON_IDS["pause"]:
            self._toggle_pause()
        elif control_id == BUTTON_IDS["clear"]:
            self._clear()
        elif control_id == BUTTON_IDS["roi"]:
            self._toggle_roi()

    def _toggle_pause(self) -> None:
        self._paused = not self._paused
        win32gui.SetWindowText(self._pause_hwnd, "재개" if self._paused else "일시정지")

    def _clear(self) -> None:
        self._lines.clear()
        self._line_count = 0
        win32gui.SetWindowText(self._edit_hwnd, "")
        self._set_status("로그 지워짐")

    def _toggle_roi(self) -> None:
        checked = self._button_checked(self._roi_hwnd)
        self._set_roi_text(checked)
        if self._roi_toggle_cb:
            self._roi_toggle_cb(checked)

    def _tag_visible(self, message: str) -> bool:
        for tag, hwnd in self._filter_hwnds.items():
            if tag in message:
                return self._button_checked(hwnd)
        return True

    def _trim_lines(self) -> None:
        overflow = len(self._lines) - self._max_lines()
        if overflow > 0:
            del self._lines[: max(overflow, 50)]

    def _refresh_log_text(self) -> None:
        win32gui.SetWindowText(self._edit_hwnd, "\r\n".join(self._lines))
        win32gui.SendMessage(self._edit_hwnd, win32con.EM_SETSEL, -1, -1)
        win32gui.SendMessage(self._edit_hwnd, win32con.EM_SCROLLCARET, 0, 0)

    def _set_status(self, text: str) -> None:
        win32gui.SetWindowText(self._status_hwnd, text)

    def _set_roi_text(self, checked: bool) -> None:
        win32gui.SetWindowText(self._roi_hwnd, "ROI 표시 ON" if checked else "ROI 표시 OFF")

    def _button_width(self, text: str) -> int:
        return max(MIN_BUTTON_WIDTH, _text_width(self.hwnd, self._font, text) + BUTTON_PADDING_X)

    def _check_width(self, text: str) -> int:
        return max(MIN_CHECK_WIDTH, _text_width(self.hwnd, self._font, text) + CHECK_PADDING_X)

    def _button_checked(self, hwnd: int) -> bool:
        return win32gui.SendMessage(hwnd, win32con.BM_GETCHECK, 0, 0) == win32con.BST_CHECKED

    def _set_button_checked(self, hwnd: int, checked: bool) -> None:
        state = win32con.BST_CHECKED if checked else win32con.BST_UNCHECKED
        win32gui.SendMessage(hwnd, win32con.BM_SETCHECK, state, 0)

    def _max_lines(self) -> int:
        return int(SETTINGS["debug_window"]["max_lines"])

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)


def _button(parent: int, hinst: int, text: str, control_id: int, x: int, y: int, w: int, h: int) -> int:
    return _control(parent, hinst, "BUTTON", text, win32con.BS_PUSHBUTTON, control_id, x, y, w, h)


def _check(parent: int, hinst: int, text: str, control_id: int, x: int, y: int, w: int, h: int) -> int:
    return _control(parent, hinst, "BUTTON", text, win32con.BS_AUTOCHECKBOX, control_id, x, y, w, h)


def _static(parent: int, hinst: int, text: str, x: int, y: int, w: int, h: int) -> int:
    return _control(parent, hinst, "STATIC", text, win32con.SS_LEFT, 0, x, y, w, h)


def _edit(parent: int, hinst: int, x: int, y: int, w: int, h: int) -> int:
    style = win32con.ES_MULTILINE | win32con.ES_READONLY | win32con.ES_AUTOVSCROLL | win32con.WS_VSCROLL
    return _control(parent, hinst, "EDIT", "", style, 0, x, y, w, h)


def _control(parent: int, hinst: int, cls: str, text: str, style: int, control_id: int, x: int, y: int, w: int, h: int) -> int:
    return win32gui.CreateWindowEx(
        0, cls, text, win32con.WS_CHILD | win32con.WS_VISIBLE | style,
        x, y, w, h, parent, control_id, hinst, None,
    )


def _create_font() -> int:
    logfont = win32gui.LOGFONT()
    logfont.lfFaceName = DEFAULT_FONT_FACE
    logfont.lfHeight = -14
    logfont.lfWeight = win32con.FW_NORMAL
    logfont.lfQuality = win32con.CLEARTYPE_NATURAL_QUALITY
    return win32gui.CreateFontIndirect(logfont)


def _text_width(hwnd: int, font: int, text: str) -> int:
    hdc = win32gui.GetDC(hwnd)
    old_font = win32gui.SelectObject(hdc, font)
    try:
        width, _height = win32gui.GetTextExtentPoint32(hdc, text)
        return int(width)
    finally:
        win32gui.SelectObject(hdc, old_font)
        win32gui.ReleaseDC(hwnd, hdc)


def _center_position(size: tuple[int, int]) -> tuple[int, int]:
    width, height = size
    screen_width = win32api.GetSystemMetrics(win32con.SM_CXSCREEN)
    screen_height = win32api.GetSystemMetrics(win32con.SM_CYSCREEN)
    return ((screen_width - width) // 2, (screen_height - height) // 2)
