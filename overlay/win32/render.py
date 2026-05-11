"""GDI renderer for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass

import win32con
import win32gui

from overlay.win32 import style
from overlay.win32.view_state import (
    Win32OverlayViewState,
    Win32PatternTab,
    Win32Recommendation,
)

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
        self._draw_tabs(hdc, view_state)
        for index, line in enumerate(view_state.recommendations[:6], start=1):
            self._draw_recommendation(hdc, index, line)
        self._draw_footer(hdc, view_state.footer)

    def select_font(
        self,
        hdc: int,
        size: int = style.BODY_FONT_SIZE,
        weight: int = win32con.FW_SEMIBOLD,
        face: str = style.FONT_FACE,
    ) -> None:
        key = (face, size, weight)
        if key not in self._fonts:
            self._fonts[key] = self._create_font(size, weight, face)
        win32gui.SelectObject(hdc, self._fonts[key])

    def destroy(self) -> None:
        for font in self._fonts.values():
            win32gui.DeleteObject(font)
        self._fonts.clear()

    @property
    def font_created(self) -> bool:
        return bool(self._fonts)

    def _1s(self, value: int) -> int:
        return max(1, round(value * self._scale))

    def _draw_background(self, hdc: int) -> None:
        self._draw_round_rect(hdc, (0, 0, 360, 324), 28, style.PANEL_BG)

    def _draw_header(self, hdc: int, view_state: Win32OverlayViewState) -> None:
        self._draw_round_rect(hdc, (8, 8, 352, 74), 20, style.HEADER_BG)
        lamp_color = self._lamp_color(view_state.is_stable)
        self._draw_lamp(hdc, 20, 32, lamp_color)
        self._draw_mode_badge(hdc, _mode_text(view_state.mode_diff), 36, 23, 64, 45)
        self._draw_text(
            hdc, view_state.title, 74, 20, 318, 44,
            style.TEXT_MAIN, style.HEADER_BG, style.TITLE_FONT_SIZE, win32con.FW_BOLD,
        )
        self._draw_text(
            hdc, "—", 24, 48, 336, 68,
            style.TEXT_ACCENT, style.HEADER_BG, style.META_FONT_SIZE,
            align_center=True,
        )
        self._draw_text(
            hdc, "⚙", 326, 22, 346, 44,
            style.TEXT_MAIN, style.HEADER_BG, 15, win32con.FW_NORMAL,
            align_center=True, face=style.EMOJI_FONT_FACE,
        )

    def _draw_tabs(self, hdc: int, view_state: Win32OverlayViewState) -> None:
        tabs = view_state.tabs or _empty_tabs()
        for index, tab in enumerate(tabs[:4]):
            self._draw_tab(hdc, index, tab, view_state.active_diff)

    def _draw_tab(
        self, hdc: int, index: int, tab: Win32PatternTab, active_diff: str
    ) -> None:
        top = 80 + (index * 50)
        bg = style.TAB_ACTIVE_BG if tab.difficulty == active_diff else style.TAB_BG
        label_color = style.DIFF_COLORS.get(tab.difficulty, style.TEXT_MAIN)
        floor_color = _tab_floor_color(tab, active_diff)
        self._draw_round_rect(hdc, (8, top, 60, top + 46), 10, bg)
        self._draw_text(
            hdc, tab.difficulty, 8, top + 7, 60, top + 23,
            label_color, bg, style.BODY_FONT_SIZE, win32con.FW_BOLD,
            align_center=True,
        )
        self._draw_text(
            hdc, tab.label, 8, top + 24, 60, top + 40,
            floor_color, bg, style.META_FONT_SIZE, win32con.FW_SEMIBOLD,
            align_center=True,
        )

    def _draw_recommendation(
        self, hdc: int, index: int, line: Win32Recommendation | str
    ) -> None:
        top = 81 + ((index - 1) * 31)
        self._draw_round_rect(hdc, (73, top, 352, top + 28), 8, style.ROW_BG)
        if isinstance(line, Win32Recommendation):
            self._draw_recommendation_entry(hdc, top, line)
            return
        self._draw_text(
            hdc, str(line), 80, top + 2, 342, top + 26,
            style.TEXT_BODY, style.ROW_BG, style.BODY_FONT_SIZE,
        )

    def _draw_recommendation_entry(
        self, hdc: int, top: int, entry: Win32Recommendation
    ) -> None:
        badge = f"{entry.difficulty} {entry.level}".strip()
        self._draw_diff_badge(hdc, badge, entry.difficulty, 74, top + 3, 110, top + 25)
        self._draw_text(
            hdc, entry.song_name, 118, top + 2, 270, top + 26,
            style.TEXT_BODY, style.ROW_BG, style.BODY_FONT_SIZE,
        )
        self._draw_status_badge(hdc, 272, top + 6, entry)
        self._draw_rate(hdc, 294, top + 2, 344, top + 26, entry.rate)

    def _draw_lamp(self, hdc: int, x: int, y: int, color: int) -> None:
        brush = win32gui.CreateSolidBrush(color)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.Ellipse(hdc, *self._rect(x, y, x + 7, y + 7))
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
            align_center=True,
        )

    def _draw_mode_badge(
        self, hdc: int, text: str, left: int, top: int, right: int, bottom: int
    ) -> None:
        color = style.MODE_COLORS.get(text, style.BADGE_BG)
        self._draw_round_rect(hdc, (left, top, right, bottom), 6, color)
        self._draw_text(
            hdc, text or "—", left + 3, top + 1, right - 3, bottom - 1,
            style.TEXT_MAIN, color, style.BODY_FONT_SIZE, win32con.FW_BOLD,
            align_center=True,
        )

    def _draw_footer(self, hdc: int, footer: str) -> None:
        self._draw_round_rect(hdc, (8, 290, 352, 316), 8, style.FOOTER_BG)
        self._draw_text(
            hdc, "유사 구간 평균", 18, 292, 140, 314,
            style.TEXT_MUTED, style.FOOTER_BG, style.META_FONT_SIZE,
        )
        self._draw_text(
            hdc, footer, 154, 292, 342, 314,
            style.TEXT_MAIN, style.FOOTER_BG, style.BODY_FONT_SIZE, win32con.FW_BOLD,
            align_right=True,
        )

    def _draw_diff_badge(
        self, hdc: int, text: str, difficulty: str, left: int, top: int,
        right: int, bottom: int,
    ) -> None:
        color = style.DIFF_COLORS.get(difficulty, style.BADGE_BG)
        self._draw_round_rect(hdc, (left, top, right, bottom), 8, color)
        self._draw_text(
            hdc, text or "—", left + 2, top, right - 2, bottom,
            style.TEXT_MAIN, color, style.META_FONT_SIZE, win32con.FW_BOLD,
            align_center=True,
        )

    def _draw_status_badge(
        self, hdc: int, left: int, top: int, entry: Win32Recommendation
    ) -> None:
        status = _status_badge(entry)
        if status is None:
            return
        text, color = status
        self._draw_round_rect(hdc, (left, top, left + 16, top + 16), 16, color)
        self._draw_text(
            hdc, text, left, top, left + 16, top + 16,
            style.TEXT_MAIN, color, 9, win32con.FW_BOLD,
            align_center=True,
        )

    def _draw_rate(
        self, hdc: int, left: int, top: int, right: int, bottom: int,
        rate: float | None,
    ) -> None:
        if rate is None:
            text, color = "——", style.TEXT_MUTED
        else:
            text, color = f"{rate:.2f}%", _rate_color(rate)
        self._draw_text(
            hdc, text, left, top, right, bottom,
            color, style.ROW_BG, style.BODY_FONT_SIZE, win32con.FW_BOLD,
            align_right=True,
        )

    def _draw_round_rect(
        self,
        hdc: int,
        rect: tuple[int, int, int, int],
        radius: int,
        color: int,
    ) -> None:
        old_pen = win32gui.SelectObject(hdc, win32gui.GetStockObject(win32con.NULL_PEN))
        brush = win32gui.CreateSolidBrush(color)
        old_brush = win32gui.SelectObject(hdc, brush)
        try:
            win32gui.RoundRect(hdc, *self._rect(*rect), self._s(radius), self._s(radius))
        finally:
            win32gui.SelectObject(hdc, old_brush)
            win32gui.SelectObject(hdc, old_pen)
            win32gui.DeleteObject(brush)

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
        align_right: bool = False,
        align_center: bool = False,
        face: str = style.FONT_FACE,
    ) -> None:
        self.select_font(hdc, size, weight, face)
        win32gui.SetBkMode(hdc, win32con.OPAQUE)
        win32gui.SetBkColor(hdc, bg_color)
        win32gui.SetTextColor(hdc, color)
        align_flag = _text_align_flag(align_right, align_center)
        flags = style.TEXT_FLAGS | align_flag
        win32gui.DrawText(hdc, text, -1, self._rect(left, top, right, bottom), flags)

    def _rect(self, left: int, top: int, right: int, bottom: int) -> tuple[int, int, int, int]:
        return self._s(left), self._s(top), self._s(right), self._s(bottom)

    def _lamp_color(self, is_stable: bool) -> int:
        if is_stable:
            return style.STABLE
        return style.UNSTABLE

    def _create_font(self, size: int, weight: int, face: str) -> int:
        logfont = win32gui.LOGFONT()
        logfont.lfFaceName = face
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
        _measure_case(hdc, "title", view_state.title, 242, 24, scale),
        _measure_case(hdc, "mode", _mode_text(view_state.mode_diff), 28, 20, scale),
        _measure_case(hdc, "footer", view_state.footer, 174, 22, scale),
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
    recommendations: list[Win32Recommendation | str],
    scale: float,
) -> list[TextLayoutCase]:
    cases: list[TextLayoutCase] = []
    for index, line in enumerate(recommendations[:6], start=1):
        text = _recommendation_text(line)
        cases.append(_measure_case(hdc, f"recommendation_{index}", text, 152, 24, scale))
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


def _mode_text(mode_diff: str) -> str:
    return mode_diff.split(" ", 1)[0] if mode_diff else "—"


def _recommendation_text(line: Win32Recommendation | str) -> str:
    if isinstance(line, Win32Recommendation):
        return line.song_name
    return str(line)


def _empty_tabs() -> list[Win32PatternTab]:
    return [
        Win32PatternTab("NM", "—", False),
        Win32PatternTab("HD", "—", False),
        Win32PatternTab("MX", "—", False),
        Win32PatternTab("SC", "—", False),
    ]


def _tab_floor_color(tab: Win32PatternTab, active_diff: str) -> int:
    if tab.difficulty == active_diff:
        return style.TEXT_TAB
    if tab.exists:
        return style.TEXT_TAB_MUTED
    return style.TEXT_MUTED


def _status_badge(entry: Win32Recommendation) -> tuple[str, int] | None:
    if entry.rate is not None and entry.rate >= 100.0:
        return "P", style.PERFECT_PLAY
    if entry.is_max_combo:
        return "M", style.MAX_COMBO
    return None


def _rate_color(rate: float) -> int:
    if rate >= 100.0:
        return style.PERFECT_PLAY
    if rate >= 99.0:
        return style.TEXT_RATE_HIGH
    if rate >= 95.0:
        return style.TEXT_RATE_MID
    if rate >= 90.0:
        return style.TEXT_RATE_SOFT
    return style.TEXT_RATE_LOW


def _text_align_flag(align_right: bool, align_center: bool) -> int:
    if align_right:
        return win32con.DT_RIGHT
    if align_center:
        return win32con.DT_CENTER
    return 0
