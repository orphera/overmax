"""Win32 main overlay window candidate."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
from typing import Callable, Optional

import win32api
import win32con
import win32gui

from overlay.win32.geometry import (
    BASE_HEIGHT,
    BASE_WIDTH,
    PositionDiagnostics,
    calculate_game_position,
)
from overlay.win32.render import (
    RenderDiagnostics,
    TextLayoutDiagnostics,
    Win32OverlayRenderer,
    build_text_layout_diagnostics,
)
from overlay.win32.view_state import Win32OverlayViewState, default_view_state
from settings import SETTINGS

WDA_EXCLUDEFROMCAPTURE = 0x00000011
CLASS_NAME = "OvermaxWin32Overlay"
WINDOW_TITLE = "Overmax Win32 overlay"
MIN_CONFIDENCE_OPACITY = 0.3


@dataclass(frozen=True)
class WindowDiagnostics:
    capture_excluded: bool
    dpi: int
    monitor: tuple[int, int, int, int]
    rect: tuple[int, int, int, int]
    style_ok: bool
    ex_style: int


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


class Win32OverlayWindow:
    def __init__(self, view_state: Optional[Win32OverlayViewState] = None) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self.capture_excluded = False
        self._view_state = view_state or default_view_state()
        self._scale = _read_overlay_scale()
        self._base_opacity = _read_base_opacity()
        self._last_confidence = 1.0
        self._renderer = Win32OverlayRenderer(self._scale)
        self._manual_position = False
        self._user_move_cb: Optional[Callable[[int, int], None]] = None
        self._rounded_region_applied = False
        self._register_class()

    def create(self) -> int:
        if self.hwnd:
            return self.hwnd
        ex_style = self._window_ex_style()
        self.hwnd = win32gui.CreateWindowEx(
            ex_style, CLASS_NAME, WINDOW_TITLE, win32con.WS_POPUP,
            120, 120, *self._window_size(), 0, 0, self.hinst, None,
        )
        self._apply_opacity()
        self._rounded_region_applied = self._apply_rounded_region(self.hwnd)
        self.capture_excluded = set_capture_exclusion(self.hwnd)
        return self.hwnd

    def update_view_state(self, view_state: Win32OverlayViewState) -> None:
        self._view_state = view_state
        if self.hwnd:
            win32gui.InvalidateRect(self.hwnd, None, True)

    def show(self) -> None:
        hwnd = self.create()
        win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
        win32gui.UpdateWindow(hwnd)

    def hide(self) -> None:
        if self.hwnd:
            win32gui.ShowWindow(self.hwnd, win32con.SW_HIDE)

    def toggle_visibility(self) -> None:
        if self.is_visible():
            self.hide()
        else:
            self.show()

    def is_visible(self) -> bool:
        return bool(self.hwnd and win32gui.IsWindowVisible(self.hwnd))

    def show_for(self, duration_ms: int) -> int:
        hwnd = self.create()
        win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
        win32gui.UpdateWindow(hwnd)
        ctypes.windll.user32.SetTimer(hwnd, 1, duration_ms, None)
        print(f"capture_excluded={self.capture_excluded}")
        print(f"dpi={self._get_window_dpi(hwnd)}")
        return self._message_loop()

    def set_user_move_callback(self, callback: Callable[[int, int], None]) -> None:
        self._user_move_cb = callback

    def update_base_opacity(self, base_opacity: float) -> None:
        self._base_opacity = _clamp_float(base_opacity, 0.1, 1.0, 0.8)
        self._apply_opacity()

    def update_confidence(self, confidence: float) -> None:
        self._last_confidence = _clamp_float(confidence, 0.0, 1.0, 1.0)
        self._apply_opacity()

    def rebuild_ui(self, scale: float) -> None:
        self._scale = _clamp_float(scale, 0.1, 4.0, 1.0)
        self._renderer.set_scale(self._scale)
        if not self.hwnd:
            return
        x, y, _, _ = win32gui.GetWindowRect(self.hwnd)
        width, height = self._window_size()
        win32gui.SetWindowPos(
            self.hwnd, 0, x, y, width, height,
            win32con.SWP_NOACTIVATE | win32con.SWP_NOZORDER,
        )
        self._rounded_region_applied = self._apply_rounded_region(self.hwnd)
        win32gui.InvalidateRect(self.hwnd, None, True)

    def apply_saved_position(self, x: int, y: int) -> tuple[int, int]:
        self.create()
        self._manual_position = True
        win32gui.SetWindowPos(
            self.hwnd, 0, x, y, 0, 0,
            win32con.SWP_NOACTIVATE | win32con.SWP_NOSIZE | win32con.SWP_NOZORDER,
        )
        return win32gui.GetWindowRect(self.hwnd)[:2]

    def move_to_game_rect(self, left: int, top: int, width: int, height: int) -> None:
        if self._manual_position:
            return
        hwnd = self.create()
        monitor = self._get_monitor_rect(hwnd)
        dpi = self._get_window_dpi(hwnd)
        x, y = calculate_game_position((left, top, width, height), monitor, dpi, self._scale)
        win32gui.SetWindowPos(
            hwnd, 0, x, y, 0, 0,
            win32con.SWP_NOACTIVATE | win32con.SWP_NOSIZE | win32con.SWP_NOZORDER,
        )

    def simulate_user_move(self, x: int, y: int) -> tuple[int, int]:
        self._manual_position = True
        win32gui.SetWindowPos(
            self.hwnd, 0, x, y, 0, 0,
            win32con.SWP_NOACTIVATE | win32con.SWP_NOSIZE | win32con.SWP_NOZORDER,
        )
        self._emit_user_move()
        return win32gui.GetWindowRect(self.hwnd)[:2]

    def draw(self, hdc: int) -> None:
        self._renderer.draw_panel(hdc, self._view_state)

    def drawing_size(self) -> tuple[int, int]:
        return self._window_size()

    def position_diagnostics(self) -> PositionDiagnostics:
        hwnd = self.create()
        monitor = self._get_monitor_rect(hwnd)
        calculated = calculate_game_position((200, 120, 1280, 720), monitor)
        saved = self.apply_saved_position(calculated[0] + 24, calculated[1] + 18)
        callback_positions: list[tuple[int, int]] = []
        self.set_user_move_callback(lambda x, y: callback_positions.append((x, y)))
        moved = self.simulate_user_move(saved[0] + 12, saved[1] + 10)
        return PositionDiagnostics(
            calculated, saved, moved, callback_positions[-1], monitor
        )

    def render_diagnostics(self) -> RenderDiagnostics:
        hwnd = self.create()
        hdc = win32gui.GetDC(hwnd)
        try:
            self._renderer.select_font(hdc)
            text_extent = win32gui.GetTextExtentPoint32(hdc, self._view_state.title)
        finally:
            win32gui.ReleaseDC(hwnd, hdc)
        return RenderDiagnostics(
            self._alpha(),
            self._rounded_region_applied,
            self._renderer.font_created,
            win32con.CLEARTYPE_QUALITY,
            text_extent,
        )

    def text_layout_diagnostics(self) -> TextLayoutDiagnostics:
        hwnd = self.create()
        hdc = win32gui.GetDC(hwnd)
        try:
            self._renderer.select_font(hdc)
            return build_text_layout_diagnostics(hdc, self._view_state, self._scale)
        finally:
            win32gui.ReleaseDC(hwnd, hdc)

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

    def destroy(self) -> None:
        self._renderer.destroy()

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
            pass

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
        if msg == win32con.WM_EXITSIZEMOVE:
            self._emit_user_move()
            return 0
        if msg == win32con.WM_NCHITTEST:
            return win32con.HTCAPTION
        if msg == win32con.WM_SETCURSOR:
            win32gui.SetCursor(win32gui.LoadCursor(0, win32con.IDC_SIZEALL))
            return 1
        if msg == win32con.WM_DESTROY:
            self.destroy()
            win32gui.PostQuitMessage(0)
            return 0
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _emit_user_move(self) -> None:
        if self._user_move_cb is None:
            return
        left, top, _, _ = win32gui.GetWindowRect(self.hwnd)
        self._user_move_cb(left, top)

    def _apply_rounded_region(self, hwnd: int) -> bool:
        try:
            width, height = self._window_size()
            radius = max(1, round(24 * self._scale))
            region = win32gui.CreateRoundRectRgn(0, 0, width, height, radius, radius)
            win32gui.SetWindowRgn(hwnd, region, True)
            return True
        except Exception as exc:
            print(f"SetWindowRgn failed: {exc}")
            return False

    def _paint(self, hwnd: int) -> None:
        hdc, paint_struct = win32gui.BeginPaint(hwnd)
        try:
            self.draw(hdc)
        finally:
            win32gui.EndPaint(hwnd, paint_struct)

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
        required = self._window_ex_style()
        return (ex_style & required) == required

    def _window_ex_style(self) -> int:
        return (
            win32con.WS_EX_LAYERED
            | win32con.WS_EX_TOPMOST
            | win32con.WS_EX_TOOLWINDOW
            | win32con.WS_EX_NOACTIVATE
        )

    def _window_size(self) -> tuple[int, int]:
        return (
            max(1, round(BASE_WIDTH * self._scale)),
            max(1, round(BASE_HEIGHT * self._scale)),
        )

    def _apply_opacity(self) -> None:
        if self.hwnd:
            win32gui.SetLayeredWindowAttributes(
                self.hwnd, 0, self._alpha(), win32con.LWA_ALPHA
            )

    def _alpha(self) -> int:
        confidence = MIN_CONFIDENCE_OPACITY + (
            (1.0 - MIN_CONFIDENCE_OPACITY) * self._last_confidence
        )
        return max(1, min(255, round(255 * self._base_opacity * confidence)))


def _read_overlay_scale() -> float:
    return _clamp_float(SETTINGS.get("overlay", {}).get("scale", 1.0), 0.1, 4.0, 1.0)


def _read_base_opacity() -> float:
    value = SETTINGS.get("overlay", {}).get("base_opacity", 0.8)
    return _clamp_float(value, 0.1, 1.0, 0.8)


def _clamp_float(value: object, low: float, high: float, fallback: float) -> float:
    try:
        return max(low, min(high, float(value)))
    except (TypeError, ValueError):
        return fallback
