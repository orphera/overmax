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


def font_cell_height(size: int, overrides: dict[int, int] | None = None) -> int:
    if overrides is None:
        return size
    return overrides.get(size, size)


def font_weight(weight: int, bias: int = FONT_WEIGHT_BIAS) -> int:
    if weight <= win32con.FW_NORMAL:
        return weight
    return min(win32con.FW_HEAVY, weight + bias)
