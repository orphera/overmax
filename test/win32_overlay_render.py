"""Render diagnostics for the Win32 overlay smoke."""

from __future__ import annotations

from dataclasses import dataclass

import win32api
import win32con
import win32gui

from win32_overlay_payload_sample import Win32OverlayViewState

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


def print_render_diagnostics(diagnostics: RenderDiagnostics) -> None:
    print(f"alpha={diagnostics.alpha}")
    print(f"rounded_region={diagnostics.rounded_region}")
    print(f"font_created={diagnostics.font_created}")
    print(f"font_quality={diagnostics.font_quality}")
    print(f"text_extent={diagnostics.text_extent}")


def print_text_layout_diagnostics(diagnostics: TextLayoutDiagnostics) -> None:
    for case in diagnostics.cases:
        print(
            "{name}=width:{text_width}/{width} height:{text_height}/{height} "
            "fits_width:{fits_width} fits_height:{fits_height}".format(
                name=case.name,
                text_width=case.text_width,
                width=case.width,
                text_height=case.text_height,
                height=case.height,
                fits_width=case.fits_width,
                fits_height=case.fits_height,
            )
        )
    print(f"overflowing_cases={len(diagnostics.overflowing_cases)}")


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
