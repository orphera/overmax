"""Pixel-level render smoke for the Win32 overlay candidate."""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

import win32api
import win32con
import win32gui

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from win32_overlay_geometry import BASE_HEIGHT, BASE_WIDTH
from win32_overlay_payload_sample import long_payload_view_state
from win32_overlay_smoke import Win32OverlaySmoke

BLACK = win32api.RGB(0, 0, 0)
PANEL_BG = win32api.RGB(18, 24, 38)


@dataclass(frozen=True)
class PixelDiagnostics:
    total_pixels: int
    non_blank_pixels: int
    panel_bg_pixels: int
    bright_text_pixels: int
    accent_pixels: int
    cyan_pixels: int
    divider_pixels: int
    unique_colors: int


def main() -> int:
    diagnostics = render_pixel_diagnostics()
    print_pixel_diagnostics(diagnostics)
    return 0 if pixel_diagnostics_ok(diagnostics) else 1


def render_pixel_diagnostics() -> PixelDiagnostics:
    screen_dc = win32gui.GetDC(0)
    memory_dc = win32gui.CreateCompatibleDC(screen_dc)
    bitmap = win32gui.CreateCompatibleBitmap(screen_dc, BASE_WIDTH, BASE_HEIGHT)
    old_bitmap = win32gui.SelectObject(memory_dc, bitmap)
    smoke = Win32OverlaySmoke(long_payload_view_state())
    try:
        _fill_background(memory_dc)
        smoke._draw_panel(memory_dc)
        return sample_pixels(memory_dc)
    finally:
        smoke._destroy_font()
        win32gui.SelectObject(memory_dc, old_bitmap)
        win32gui.DeleteObject(bitmap)
        win32gui.DeleteDC(memory_dc)
        win32gui.ReleaseDC(0, screen_dc)


def _fill_background(hdc: int) -> None:
    brush = win32gui.CreateSolidBrush(BLACK)
    try:
        win32gui.FillRect(hdc, (0, 0, BASE_WIDTH, BASE_HEIGHT), brush)
    finally:
        win32gui.DeleteObject(brush)


def sample_pixels(hdc: int) -> PixelDiagnostics:
    counts: dict[str, int] = {
        "non_blank": 0,
        "panel_bg": 0,
        "bright_text": 0,
        "accent": 0,
        "cyan": 0,
        "divider": 0,
    }
    unique_colors: set[int] = set()
    for y in range(BASE_HEIGHT):
        for x in range(BASE_WIDTH):
            _sample_pixel(win32gui.GetPixel(hdc, x, y), counts, unique_colors)
    return PixelDiagnostics(
        total_pixels=BASE_WIDTH * BASE_HEIGHT,
        non_blank_pixels=counts["non_blank"],
        panel_bg_pixels=counts["panel_bg"],
        bright_text_pixels=counts["bright_text"],
        accent_pixels=counts["accent"],
        cyan_pixels=counts["cyan"],
        divider_pixels=counts["divider"],
        unique_colors=len(unique_colors),
    )


def _sample_pixel(color: int, counts: dict[str, int], unique_colors: set[int]) -> None:
    unique_colors.add(color)
    red, green, blue = _rgb(color)
    if color != BLACK:
        counts["non_blank"] += 1
    if color == PANEL_BG:
        counts["panel_bg"] += 1
    if red > 180 and green > 180 and blue > 180:
        counts["bright_text"] += 1
    if red > 200 and 140 <= green <= 230 and blue < 160:
        counts["accent"] += 1
    if red < 90 and green > 150 and blue > 170:
        counts["cyan"] += 1
    if 40 <= red <= 60 and 50 <= green <= 70 and 70 <= blue <= 90:
        counts["divider"] += 1


def _rgb(color: int) -> tuple[int, int, int]:
    return color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF


def pixel_diagnostics_ok(diagnostics: PixelDiagnostics) -> bool:
    return (
        diagnostics.non_blank_pixels > 20_000
        and diagnostics.panel_bg_pixels > 10_000
        and diagnostics.bright_text_pixels > 50
        and diagnostics.accent_pixels > 20
        and diagnostics.cyan_pixels > 10
        and diagnostics.divider_pixels > 100
        and diagnostics.unique_colors >= 6
    )


def print_pixel_diagnostics(diagnostics: PixelDiagnostics) -> None:
    print(f"total_pixels={diagnostics.total_pixels}")
    print(f"non_blank_pixels={diagnostics.non_blank_pixels}")
    print(f"panel_bg_pixels={diagnostics.panel_bg_pixels}")
    print(f"bright_text_pixels={diagnostics.bright_text_pixels}")
    print(f"accent_pixels={diagnostics.accent_pixels}")
    print(f"cyan_pixels={diagnostics.cyan_pixels}")
    print(f"divider_pixels={diagnostics.divider_pixels}")
    print(f"unique_colors={diagnostics.unique_colors}")


if __name__ == "__main__":
    raise SystemExit(main())
