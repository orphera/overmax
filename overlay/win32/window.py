"""Win32 main overlay window candidate."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
from typing import Callable, Optional

import win32api
import win32con
import win32gui

from overlay.win32 import style
from infra.gui.back_buffer import draw_buffered
from overlay.win32.geometry import PositionDiagnostics, calculate_game_position
from overlay.win32.geometry import scale_for_dpi, scaled_window_size
from infra.gui.dpi import get_system_dpi, get_window_dpi, set_process_dpi_awareness
from infra.gui.input import client_point_in_rect, hit_test_from_lparam
from infra.gui.input import point_from_lparam, rect_from_lparam
from infra.gui.input import screen_point_from_lparam
from infra.gui.placement import ManualPlacement, move_resize_window, move_window
from infra.gui.placement import resize_window
from infra.gui.placement import window_position
from infra.gui.windowing import foreground_preserved_by_show
from infra.gui.windowing import create_window
from infra.gui.windowing import get_monitor_rect, has_ex_style, register_window_class
from infra.gui.windowing import required_styles_present, run_message_loop
from infra.gui.windowing import set_capture_exclusion, WindowCreateSpec
from overlay.win32.render import (
    RenderDiagnostics,
    TextLayoutDiagnostics,
    Win32OverlayRenderer,
    build_text_layout_diagnostics,
)
from overlay.win32.view_state import Win32OverlayViewState, default_view_state
from settings import SETTINGS

WM_DPICHANGED = 0x02E0
CLASS_NAME = "OvermaxWin32Overlay"
WINDOW_TITLE = "Overmax Win32 overlay"
MIN_CONFIDENCE_OPACITY = 0.3
SETTINGS_BUTTON_RECT = (316, 16, 340, 40)


@dataclass(frozen=True)
class WindowDiagnostics:
    capture_excluded: bool
    dpi: int
    monitor: tuple[int, int, int, int]
    rect: tuple[int, int, int, int]
    style_ok: bool
    noactivate: bool
    topmost: bool
    focus_preserved: bool
    ex_style: int


class Win32OverlayWindow:
    def __init__(self, view_state: Optional[Win32OverlayViewState] = None) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self.capture_excluded = False
        self._view_state = view_state or default_view_state()
        self._scale = _read_overlay_scale()
        self._dpi = get_system_dpi()
        self._base_opacity = _read_base_opacity()
        self._last_confidence = 1.0
        self._renderer = Win32OverlayRenderer(self._render_scale())
        self._placement = ManualPlacement()
        self._user_move_cb: Optional[Callable[[int, int], None]] = None
        self._settings_cb: Optional[Callable[[], None]] = None
        self._rounded_region_applied = False
        self._register_class()

    def create(self) -> int:
        if self.hwnd:
            return self.hwnd
        self.hwnd = create_window(self.hinst, self._create_spec())
        self._apply_opacity()
        self._refresh_dpi_metrics(self.hwnd)
        self._rounded_region_applied = self._apply_rounded_region(self.hwnd)
        self.capture_excluded = set_capture_exclusion(self.hwnd)
        return self.hwnd

    def update_view_state(self, view_state: Win32OverlayViewState) -> None:
        self._view_state = view_state
        if self.hwnd:
            win32gui.InvalidateRect(self.hwnd, None, False)

    def show(self) -> None:
        hwnd = self.create()
        if not self._can_show_overlay():
            self.hide()
            return
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
        if not self._can_show_overlay():
            print(f"capture_excluded={self.capture_excluded}")
            print("show_suppressed=True")
            return 0
        win32gui.ShowWindow(hwnd, win32con.SW_SHOWNOACTIVATE)
        win32gui.UpdateWindow(hwnd)
        ctypes.windll.user32.SetTimer(hwnd, 1, duration_ms, None)
        print(f"capture_excluded={self.capture_excluded}")
        print(f"dpi={get_window_dpi(hwnd)}")
        return self._message_loop()

    def set_user_move_callback(self, callback: Callable[[int, int], None]) -> None:
        self._user_move_cb = callback

    def set_settings_callback(self, callback: Callable[[], None]) -> None:
        self._settings_cb = callback

    def update_base_opacity(self, base_opacity: float) -> None:
        self._base_opacity = _clamp_float(base_opacity, 0.1, 1.0, 0.8)
        self._apply_opacity()

    def update_confidence(self, confidence: float) -> None:
        self._last_confidence = _clamp_float(confidence, 0.0, 1.0, 1.0)
        self._apply_opacity()

    def rebuild_ui(self, scale: float) -> None:
        self._scale = _clamp_float(scale, 0.1, 4.0, 1.0)
        self._renderer.set_scale(self._render_scale())
        if not self.hwnd:
            return
        self._refresh_dpi_metrics(self.hwnd)
        width, height = self._window_size()
        resize_window(self.hwnd, width, height)
        self._rounded_region_applied = self._apply_rounded_region(self.hwnd)
        win32gui.InvalidateRect(self.hwnd, None, False)

    def apply_saved_position(self, x: int, y: int) -> tuple[int, int]:
        self.create()
        return self._placement.apply(self.hwnd, x, y)

    def move_to_game_rect(self, left: int, top: int, width: int, height: int) -> None:
        if not self._placement.should_follow_anchor():
            return
        hwnd = self.create()
        monitor = get_monitor_rect(hwnd)
        dpi = self._refresh_dpi_metrics(hwnd)
        x, y = calculate_game_position((left, top, width, height), monitor, dpi, self._scale)
        move_window(hwnd, x, y)

    def simulate_user_move(self, x: int, y: int) -> tuple[int, int]:
        moved = self._placement.apply(self.hwnd, x, y)
        self._emit_user_move()
        return moved

    def draw(self, hdc: int) -> None:
        self._renderer.draw_panel(hdc, self._view_state)

    def drawing_size(self) -> tuple[int, int]:
        return self._window_size()

    def position_diagnostics(self) -> PositionDiagnostics:
        hwnd = self.create()
        monitor = get_monitor_rect(hwnd)
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
            self._renderer.draw_panel(hdc, self._view_state)
            self._renderer.select_font(hdc)
            text_extent = win32gui.GetTextExtentPoint32(hdc, self._view_state.title)
        finally:
            win32gui.ReleaseDC(hwnd, hdc)
        return RenderDiagnostics(
            self._alpha(),
            self._rounded_region_applied,
            self._renderer.font_created,
            style.FONT_QUALITY,
            text_extent,
            self._renderer.text_diagnostics,
        )

    def text_layout_diagnostics(self) -> TextLayoutDiagnostics:
        hwnd = self.create()
        hdc = win32gui.GetDC(hwnd)
        try:
            self._renderer.select_font(hdc)
            return build_text_layout_diagnostics(hdc, self._view_state, self._render_scale())
        finally:
            win32gui.ReleaseDC(hwnd, hdc)

    def diagnostics(self) -> WindowDiagnostics:
        hwnd = self.create()
        ex_style = win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE)
        return WindowDiagnostics(
            capture_excluded=self.capture_excluded,
            dpi=get_window_dpi(hwnd),
            monitor=get_monitor_rect(hwnd),
            rect=win32gui.GetWindowRect(hwnd),
            style_ok=required_styles_present(hwnd, self._window_ex_style()),
            noactivate=has_ex_style(ex_style, win32con.WS_EX_NOACTIVATE),
            topmost=has_ex_style(ex_style, win32con.WS_EX_TOPMOST),
            focus_preserved=foreground_preserved_by_show(hwnd, self._can_show_overlay()),
            ex_style=ex_style,
        )

    def destroy(self) -> None:
        self._renderer.destroy()

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)

    def _message_loop(self) -> int:
        return run_message_loop()

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_PAINT:
            self._paint(hwnd)
            return 0
        if msg == win32con.WM_ERASEBKGND:
            return 1
        if msg == win32con.WM_TIMER:
            win32gui.DestroyWindow(hwnd)
            return 0
        if msg == WM_DPICHANGED:
            self._handle_dpi_changed(hwnd, wparam, lparam)
            return 0
        if msg == win32con.WM_EXITSIZEMOVE:
            self._emit_user_move()
            return 0
        if msg == win32con.WM_NCHITTEST:
            if self._settings_button_hit(hwnd, lparam):
                return win32con.HTCLIENT
            return win32con.HTCAPTION
        if msg == win32con.WM_LBUTTONUP:
            self._emit_settings_request(lparam)
            return 0
        if msg == win32con.WM_SETCURSOR:
            cursor = (
                win32con.IDC_ARROW
                if hit_test_from_lparam(lparam) == win32con.HTCLIENT
                else win32con.IDC_SIZEALL
            )
            win32gui.SetCursor(win32gui.LoadCursor(0, cursor))
            return 1
        if msg == win32con.WM_DESTROY:
            self.destroy()
            win32gui.PostQuitMessage(0)
            return 0
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _emit_user_move(self) -> None:
        if self._user_move_cb is None:
            return
        left, top = window_position(self.hwnd)
        self._user_move_cb(left, top)

    def _emit_settings_request(self, lparam: int) -> None:
        if self._settings_cb is None:
            return
        if self._client_point_in_rect(point_from_lparam(lparam), SETTINGS_BUTTON_RECT):
            self._settings_cb()

    def _settings_button_hit(self, hwnd: int, lparam: int) -> bool:
        point = win32gui.ScreenToClient(hwnd, screen_point_from_lparam(lparam))
        return self._client_point_in_rect(point, SETTINGS_BUTTON_RECT)

    def _client_point_in_rect(
        self, point: tuple[int, int], rect: tuple[int, int, int, int]
    ) -> bool:
        return client_point_in_rect(point, rect, self._render_scale())

    def _apply_rounded_region(self, hwnd: int) -> bool:
        try:
            width, height = self._window_size()
            radius = max(1, round(24 * self._render_scale()))
            region = win32gui.CreateRoundRectRgn(0, 0, width, height, radius, radius)
            win32gui.SetWindowRgn(hwnd, region, True)
            return True
        except Exception as exc:
            print(f"SetWindowRgn failed: {exc}")
            return False

    def _paint(self, hwnd: int) -> None:
        hdc, paint_struct = win32gui.BeginPaint(hwnd)
        try:
            width, height = self._window_size()
            draw_buffered(hdc, width, height, self.draw, style.PANEL_BG)
        finally:
            win32gui.EndPaint(hwnd, paint_struct)

    def _refresh_dpi_metrics(self, hwnd: int) -> int:
        dpi = get_window_dpi(hwnd)
        if dpi == self._dpi:
            return dpi
        self._dpi = dpi
        self._renderer.set_scale(self._render_scale())
        self._resize_to_current_metrics(hwnd)
        return dpi

    def _handle_dpi_changed(self, hwnd: int, wparam: int, lparam: int) -> None:
        self._dpi = max(1, wparam & 0xFFFF)
        self._renderer.set_scale(self._render_scale())
        left, top, right, bottom = rect_from_lparam(lparam)
        move_resize_window(hwnd, left, top, right - left, bottom - top)
        self._rounded_region_applied = self._apply_rounded_region(hwnd)

    def _resize_to_current_metrics(self, hwnd: int) -> None:
        width, height = self._window_size()
        resize_window(hwnd, width, height)

    def _can_show_overlay(self) -> bool:
        return self.capture_excluded

    def _window_ex_style(self) -> int:
        return (
            win32con.WS_EX_LAYERED
            | win32con.WS_EX_TOPMOST
            | win32con.WS_EX_TOOLWINDOW
            | win32con.WS_EX_NOACTIVATE
        )

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title=WINDOW_TITLE,
            ex_style=self._window_ex_style(),
            style=win32con.WS_POPUP,
            position=(120, 120),
            size=self._window_size(),
        )

    def _window_size(self) -> tuple[int, int]:
        return scaled_window_size(self._dpi, self._scale)

    def _render_scale(self) -> float:
        return scale_for_dpi(self._dpi, self._scale)

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
