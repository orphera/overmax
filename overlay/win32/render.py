"""GDI renderer for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass

import win32api
import win32con
import win32gui

from overlay.win32.view_state import Win32OverlayViewState

BADGE_BG = win32api.RGB(46, 68, 118)
PANEL_BG = win32api.RGB(18, 24, 38)
TEXT_FLAGS = (
    win32con.DT_SINGLELINE
    | win32con.DT_END_ELLIPSIS
    | win32con.DT_NOPREFIX
)


@dataclass(frozen=True)
class RenderDiagnostics:
    alpha: int
    rounded_region: bool
    font_created: bool
    font_quality: int
    text_extent: tuple[int, int]


@dataclass(frozen=True)
class TextLayoutCase:
    name: str
    width: int
    text_width: int
    height: int
    text_height: int
    fits_width: bool
    fits_height: bool


@dataclass(frozen=True)
class TextLayoutDiagnostics:
    cases: list[TextLayoutCase]

    @property
    def all_fit_height(self) -> bool:
        return all(case.fits_height for case in self.cases)

    @property
    def overflowing_cases(self) -> list[TextLayoutCase]:
        return [case for case in self.cases if not case.fits_width]


class Win32OverlayRenderer:
    def __init__(self) -> None:
        self._font = 0

    def draw_panel(self, hdc: int, view_state: Win32OverlayViewState) -> None:
        self._draw_background(hdc)
        lamp_color = self._lamp_color(view_state.is_stable)
        self._draw_lamp(hdc, 26, 31, lamp_color)
        self._draw_text(hdc, view_state.title, 42, 20, 240, 46, win32api.RGB(255, 209, 102))
        self._draw_badge(hdc, view_state.mode_diff, 270, 20, 334, 48)
        self._draw_text(hdc, "ui_payload -> Win32 renderer", 24, 50, 330, 78)
        for index, line in enumerate(view_state.recommendations[:2], start=1):
            top = 88 + ((index - 1) * 28)
            self._draw_text(hdc, f"{index:02d}  {line}", 32, top, 330, top + 24)
        self._draw_footer(hdc, view_state.footer)

    def select_font(self, hdc: int) -> None:
        if not self._font:
            logfont = win32gui.LOGFONT()
            logfont.lfFaceName = "Segoe UI"
            logfont.lfHeight = -15
            logfont.lfWeight = win32con.FW_SEMIBOLD
            logfont.lfQuality = win32con.CLEARTYPE_QUALITY
            self._font = win32gui.CreateFontIndirect(logfont)
        win32gui.SelectObject(hdc, self._font)

    def destroy(self) -> None:
        if self._font:
            win32gui.DeleteObject(self._font)
            self._font = 0

    @property
    def font_created(self) -> bool:
        return bool(self._font)

    def _draw_background(self, hdc: int) -> None:
        brush = win32gui.CreateSolidBrush(PANEL_BG)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, 8, 8, 352, 162, 24, 24)
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)

    def _draw_lamp(self, hdc: int, x: int, y: int, color: int) -> None:
        brush = win32gui.CreateSolidBrush(color)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.Ellipse(hdc, x, y, x + 7, y + 7)
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)

    def _draw_badge(self, hdc: int, text: str, left: int, top: int, right: int, bottom: int) -> None:
        brush = win32gui.CreateSolidBrush(BADGE_BG)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, left, top, right, bottom, 12, 12)
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)
        self._draw_text(hdc, text, left + 8, top + 4, right, bottom, bg_color=BADGE_BG)

    def _draw_footer(self, hdc: int, footer: str) -> None:
        pen = win32gui.CreatePen(win32con.PS_SOLID, 1, win32api.RGB(48, 58, 78))
        old_pen = win32gui.SelectObject(hdc, pen)
        try:
            win32gui.MoveToEx(hdc, 24, 148)
            win32gui.LineTo(hdc, 334, 148)
        finally:
            win32gui.SelectObject(hdc, old_pen)
            win32gui.DeleteObject(pen)
        self._draw_text(hdc, footer, 24, 150, 330, 170)

    def _draw_text(
        self,
        hdc: int,
        text: str,
        left: int,
        top: int,
        right: int,
        bottom: int,
        color: int = win32api.RGB(230, 236, 255),
        bg_color: int = PANEL_BG,
    ) -> None:
        self.select_font(hdc)
        win32gui.SetBkMode(hdc, win32con.OPAQUE)
        win32gui.SetBkColor(hdc, bg_color)
        win32gui.SetTextColor(hdc, color)
        win32gui.DrawText(hdc, text, -1, (left, top, right, bottom), TEXT_FLAGS)

    def _lamp_color(self, is_stable: bool) -> int:
        if is_stable:
            return win32api.RGB(0, 212, 255)
        return win32api.RGB(255, 75, 75)


def build_text_layout_diagnostics(
    hdc: int,
    view_state: Win32OverlayViewState,
) -> TextLayoutDiagnostics:
    cases = [
        _measure_case(hdc, "title", view_state.title, 240, 26),
        _measure_case(hdc, "mode_diff", view_state.mode_diff, 56, 24),
        _measure_case(hdc, "subtitle", "ui_payload -> Win32 renderer", 306, 28),
        _measure_case(hdc, "footer", view_state.footer, 306, 20),
    ]
    cases.extend(_measure_recommendation_cases(hdc, view_state.recommendations))
    return TextLayoutDiagnostics(cases)


def text_layout_diagnostics_ok(diagnostics: TextLayoutDiagnostics) -> bool:
    return diagnostics.all_fit_height and len(diagnostics.overflowing_cases) > 0


def render_diagnostics_ok(diagnostics: RenderDiagnostics) -> bool:
    text_width, text_height = diagnostics.text_extent
    return (
        diagnostics.alpha == 232
        and diagnostics.rounded_region
        and diagnostics.font_created
        and diagnostics.font_quality == win32con.CLEARTYPE_QUALITY
        and text_width > 0
        and text_height > 0
    )

def _measure_recommendation_cases(
    hdc: int,
    recommendations: list[str],
) -> list[TextLayoutCase]:
    cases: list[TextLayoutCase] = []
    for index, line in enumerate(recommendations[:2], start=1):
        cases.append(_measure_case(hdc, f"recommendation_{index}", line, 298, 24))
    return cases


def _measure_case(
    hdc: int,
    name: str,
    text: str,
    width: int,
    height: int,
) -> TextLayoutCase:
    text_width, text_height = win32gui.GetTextExtentPoint32(hdc, text)
    return TextLayoutCase(
        name=name,
        width=width,
        text_width=text_width,
        height=height,
        text_height=text_height,
        fits_width=text_width <= width,
        fits_height=text_height <= height,
    )
