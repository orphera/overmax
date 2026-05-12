"""Back-buffered paint helper for the Win32 overlay."""

from __future__ import annotations

from typing import Callable

import win32con
import win32gui

from overlay.win32 import style


def draw_buffered(
    target_hdc: int,
    width: int,
    height: int,
    draw: Callable[[int], None],
) -> None:
    """Draw a complete frame off-screen, then copy it to the window DC."""
    memory_dc = win32gui.CreateCompatibleDC(target_hdc)
    bitmap = win32gui.CreateCompatibleBitmap(target_hdc, width, height)
    old_bitmap = win32gui.SelectObject(memory_dc, bitmap)
    try:
        _fill_background(memory_dc, width, height)
        draw(memory_dc)
        _bit_blt(target_hdc, width, height, memory_dc)
    finally:
        win32gui.SelectObject(memory_dc, old_bitmap)
        win32gui.DeleteObject(bitmap)
        win32gui.DeleteDC(memory_dc)


def _fill_background(hdc: int, width: int, height: int) -> None:
    brush = win32gui.CreateSolidBrush(style.PANEL_BG)
    try:
        win32gui.FillRect(hdc, (0, 0, width, height), brush)
    finally:
        win32gui.DeleteObject(brush)


def _bit_blt(target_hdc: int, width: int, height: int, source_hdc: int) -> None:
    win32gui.BitBlt(
        target_hdc, 0, 0, width, height, source_hdc, 0, 0, win32con.SRCCOPY
    )
