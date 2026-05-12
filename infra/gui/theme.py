"""Small Win32 GUI theme primitives."""

from __future__ import annotations

import win32api
import win32con

DEFAULT_FONT_FACE = "Segoe UI"
SYMBOL_FONT_FACE = "Segoe UI Symbol"
FONT_WEIGHT_BIAS = 75
FONT_QUALITY = win32con.CLEARTYPE_NATURAL_QUALITY

TEXT_SINGLE_LINE_FLAGS = (
    win32con.DT_SINGLELINE
    | win32con.DT_NOPREFIX
    | win32con.DT_VCENTER
)

def rgb(red: int, green: int, blue: int) -> int:
    return win32api.RGB(red, green, blue)


def hex_rgb(value: str) -> int:
    if len(value) != 7 or not value.startswith("#"):
        raise ValueError("hex color must use #RRGGBB format")
    try:
        red = int(value[1:3], 16)
        green = int(value[3:5], 16)
        blue = int(value[5:7], 16)
    except ValueError as exc:
        raise ValueError("hex color must use #RRGGBB format") from exc
    return rgb(red, green, blue)


# Design Tokens - Using hex_rgb for IDE color decorators
WINDOW_BG_HEX = "#F3F4F6"
WINDOW_BG = hex_rgb(WINDOW_BG_HEX)  # #F3F4F6

PANEL_BG_HEX = "#FFFFFF"
PANEL_BG = hex_rgb(PANEL_BG_HEX)    # #FFFFFF

TEXT_COLOR_HEX = "#1F2937"
TEXT_COLOR = hex_rgb(TEXT_COLOR_HEX) # #1F2937

MUTED_TEXT_HEX = "#6B7280"
MUTED_TEXT = hex_rgb(MUTED_TEXT_HEX) # #6B7280

BORDER_COLOR_HEX = "#E5E7EB"
BORDER_COLOR = hex_rgb(BORDER_COLOR_HEX) # #E5E7EB




def font_cell_height(size: int, overrides: dict[int, int] | None = None) -> int:
    if overrides is None:
        return size
    return overrides.get(size, size)


def font_weight(weight: int, bias: int = FONT_WEIGHT_BIAS) -> int:
    if weight <= win32con.FW_NORMAL:
        return weight
    return min(win32con.FW_HEAVY, weight + bias)


def create_font(height: int = -15, weight: int = win32con.FW_NORMAL) -> int:
    """Create a GDI font with specified height and weight."""
    import win32gui
    logfont = win32gui.LOGFONT()
    logfont.lfFaceName = DEFAULT_FONT_FACE
    logfont.lfHeight = height
    logfont.lfWeight = weight
    logfont.lfQuality = win32con.CLEARTYPE_NATURAL_QUALITY
    return win32gui.CreateFontIndirect(logfont)


def text_width(hwnd: int, font: int, text: str) -> int:
    """Calculate the pixel width of a text string using the specified font."""
    import win32gui
    hdc = win32gui.GetDC(hwnd)
    old_font = win32gui.SelectObject(hdc, font)
    try:
        width, _ = win32gui.GetTextExtentPoint32(hdc, text)
        return int(width)
    finally:
        win32gui.SelectObject(hdc, old_font)
        win32gui.ReleaseDC(hwnd, hdc)
