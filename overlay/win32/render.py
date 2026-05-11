"""GDI renderer for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass

import win32con
import win32gui

from overlay.win32 import style
from overlay.win32.view_state import Win32OverlayViewState

BADGE_BG = style.BADGE_BG
PANEL_BG = style.PANEL_BG
TEXT_FLAGS = style.TEXT_FLAGS


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
    def __init__(self, scale: float = 1.0) -> None:
        self._fonts: dict[tuple[int, int], int] = {}
        self._scale = scale

    def set_scale(self, scale: float) -> None:
        if abs(self._scale - scale) < 0.001:
            return
        self._scale = max(0.1, scale)
        self.destroy()

    def draw_panel(self, hdc: int, view_state: Win32OverlayViewState) -> None:
        self._draw_background(hdc)
        self._draw_header(hdc, view_state)
        for index, line in enumerate(view_state.recommendations[:2], start=1):
            self._draw_recommendation(hdc, index, line)
        self._draw_footer(hdc, view_state.footer)

    def select_font(
        self,
        hdc: int,
        size: int = style.BODY_FONT_SIZE,
        weight: int = win32con.FW_SEMIBOLD,
    ) -> None:
        key = (size, weight)
        if key not in self._fonts:
            self._fonts[key] = self._create_font(size, weight)
        win32gui.SelectObject(hdc, self._fonts[key])

    def destroy(self) -> None:
        for font in self._fonts.values():
            win32gui.DeleteObject(font)
        self._fonts.clear()

    @property
    def font_created(self) -> bool:
        return bool(self._fonts)

    def _s(self, value: int) -> int:
        return max(1, round(value * self._scale))

    def _draw_background(self, hdc: int) -> None:
        self._draw_round_rect(hdc, (8, 8, 352, 162), 24, style.PANEL_BG)

    def _draw_header(self, hdc: int, view_state: Win32OverlayViewState) -> None:
        self._draw_round_rect(hdc, (16, 16, 344, 74), 20, style.HEADER_BG)
        lamp_color = self._lamp_color(view_state.is_stable)
        self._draw_lamp(hdc, self._s(28), self._s(32), lamp_color)
        self._draw_badge(hdc, view_state.mode_diff, 44, 23, 108, 45)
        self._draw_text(
            hdc, view_state.title, 116, 22, 304, 46,
            style.TEXT_MAIN, style.HEADER_BG, style.TITLE_FONT_SIZE, win32con.FW_BOLD,
        )
        self._draw_text(
            hdc, "ui_payload -> Win32 renderer", 28, 48, 330, 68,
            style.TEXT_ACCENT, style.HEADER_BG, style.META_FONT_SIZE,
        )

    def _draw_recommendation(self, hdc: int, index: int, line: str) -> None:
        top = 78 + ((index - 1) * 32)
        self._draw_round_rect(hdc, (16, top, 344, top + 28), 12, style.ROW_BG)
        self._draw_text(
            hdc, f"{index:02d}  {line}", 24, top + 2, 334, top + 26,
            style.TEXT_BODY, style.ROW_BG, style.BODY_FONT_SIZE,
        )

    def _draw_lamp(self, hdc: int, x: int, y: int, color: int) -> None:
        brush = win32gui.CreateSolidBrush(color)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.Ellipse(hdc, x, y, x + self._s(7), y + self._s(7))
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.DeleteObject(brush)

    def _draw_badge(
        self,
        hdc: int,
        text: str,
        left: int,
        top: int,
        right: int,
        bottom: int,
    ) -> None:
        self._draw_round_rect(hdc, (left, top, right, bottom), 6, style.BADGE_BG)
        self._draw_text(
            hdc, text or "—", left + 3, top + 1, right - 3, bottom - 1,
            style.TEXT_MAIN, style.BADGE_BG, style.BODY_FONT_SIZE, win32con.FW_BOLD,
        )

    def _draw_footer(self, hdc: int, footer: str) -> None:
        self._draw_round_rect(hdc, (16, 142, 344, 162), 8, style.FOOTER_BG)
        self._draw_text(
            hdc, footer, 26, 142, 334, 162,
            style.TEXT_MUTED, style.FOOTER_BG, style.META_FONT_SIZE,
        )

    def _draw_round_rect(
        self,
        hdc: int,
        rect: tuple[int, int, int, int],
        radius: int,
        color: int,
    ) -> None:
        pen = win32gui.CreatePen(win32con.PS_SOLID, 1, style.BORDER)
        old_pen = win32gui.SelectObject(hdc, pen)
        brush = win32gui.CreateSolidBrush(color)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, *self._rect(*rect), self._s(radius), self._s(radius))
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.SelectObject(hdc, old_pen)
            win32gui.DeleteObject(brush)
            win32gui.DeleteObject(pen)

    def _draw_text(
        self,
        hdc: int,
        text: str,
        left: int,
        top: int,
        right: int,
        bottom: int,
        color: int = style.TEXT_BODY,
        bg_color: int = style.PANEL_BG,
        size: int = style.BODY_FONT_SIZE,
        weight: int = win32con.FW_SEMIBOLD,
    ) -> None:
        self.select_font(hdc, size, weight)
        win32gui.SetBkMode(hdc, win32con.OPAQUE)
        win32gui.SetBkColor(hdc, bg_color)
        win32gui.SetTextColor(hdc, color)
        win32gui.DrawText(hdc, text, -1, self._rect(left, top, right, bottom), style.TEXT_FLAGS)

    def _rect(self, left: int, top: int, right: int, bottom: int) -> tuple[int, int, int, int]:
        return self._s(left), self._s(top), self._s(right), self._s(bottom)

    def _lamp_color(self, is_stable: bool) -> int:
        if is_stable:
            return style.STABLE
        return style.UNSTABLE

    def _create_font(self, size: int, weight: int) -> int:
        logfont = win32gui.LOGFONT()
        logfont.lfFaceName = style.FONT_FACE
        logfont.lfHeight = -self._s(size)
        logfont.lfWeight = weight
        logfont.lfQuality = win32con.CLEARTYPE_QUALITY
        return win32gui.CreateFontIndirect(logfont)


def build_text_layout_diagnostics(
    hdc: int,
    view_state: Win32OverlayViewState,
    scale: float = 1.0,
) -> TextLayoutDiagnostics:
    cases = [
        _measure_case(hdc, "title", view_state.title, 188, 24, scale),
        _measure_case(hdc, "mode_diff", view_state.mode_diff, 58, 20, scale),
        _measure_case(hdc, "subtitle", "ui_payload -> Win32 renderer", 306, 28, scale),
        _measure_case(hdc, "footer", view_state.footer, 308, 20, scale),
    ]
    cases.extend(_measure_recommendation_cases(hdc, view_state.recommendations, scale))
    return TextLayoutDiagnostics(cases)


def text_layout_diagnostics_ok(diagnostics: TextLayoutDiagnostics) -> bool:
    return diagnostics.all_fit_height and len(diagnostics.overflowing_cases) > 0


def render_diagnostics_ok(diagnostics: RenderDiagnostics) -> bool:
    text_width, text_height = diagnostics.text_extent
    return (
        1 <= diagnostics.alpha <= 255
        and diagnostics.rounded_region
        and diagnostics.font_created
        and diagnostics.font_quality == win32con.CLEARTYPE_QUALITY
        and text_width > 0
        and text_height > 0
    )

def _measure_recommendation_cases(
    hdc: int,
    recommendations: list[str],
    scale: float,
) -> list[TextLayoutCase]:
    cases: list[TextLayoutCase] = []
    for index, line in enumerate(recommendations[:2], start=1):
        cases.append(_measure_case(hdc, f"recommendation_{index}", line, 310, 24, scale))
    return cases


def _measure_case(
    hdc: int,
    name: str,
    text: str,
    width: int,
    height: int,
    scale: float,
) -> TextLayoutCase:
    text_width, text_height = win32gui.GetTextExtentPoint32(hdc, text)
    scaled_width = max(1, round(width * scale))
    scaled_height = max(1, round(height * scale))
    return TextLayoutCase(
        name=name,
        width=scaled_width,
        text_width=text_width,
        height=scaled_height,
        text_height=text_height,
        fits_width=text_width <= scaled_width,
        fits_height=text_height <= scaled_height,
    )
