"""Visual constants for the Win32 overlay renderer."""

from __future__ import annotations

import win32con

from infra.gui import theme

PANEL_BG = theme.hex_rgb("#121826")
HEADER_BG = theme.hex_rgb("#1E283E")
ROW_BG = theme.hex_rgb("#242E46")
FOOTER_BG = theme.hex_rgb("#161E30")
BADGE_BG = theme.hex_rgb("#2E4476")
BORDER = theme.hex_rgb("#303A4E")

TEXT_MAIN = theme.hex_rgb("#F0F4FF")
TEXT_BODY = theme.hex_rgb("#E8EEFF")
TEXT_MUTED = theme.hex_rgb("#505870")
TEXT_TAB = theme.hex_rgb("#B4CBFF")
TEXT_TAB_MUTED = theme.hex_rgb("#8891A7")
TEXT_ACCENT = theme.hex_rgb("#FFD166")
TEXT_RATE_HIGH = theme.hex_rgb("#B8DCFF")
TEXT_RATE_MID = theme.hex_rgb("#7EC8E3")
TEXT_RATE_SOFT = theme.hex_rgb("#B5EAD7")
TEXT_RATE_LOW = theme.hex_rgb("#FF9999")
STABLE = theme.hex_rgb("#00D4FF")
UNSTABLE = theme.hex_rgb("#FF4B4B")
MAX_COMBO = theme.hex_rgb("#4AD8B8")
PERFECT_PLAY = theme.hex_rgb("#FFD700")

TAB_BG = theme.hex_rgb("#1A243A")
TAB_ACTIVE_BG = theme.hex_rgb("#465B8A")
DIFF_COLORS = {
    "NM": theme.hex_rgb("#4A90D9"),
    "HD": theme.hex_rgb("#F5A623"),
    "MX": theme.hex_rgb("#D0021B"),
    "SC": theme.hex_rgb("#9B59B6"),
}
MODE_COLORS = {
    "4B": theme.hex_rgb("#2D4F55"),
    "5B": theme.hex_rgb("#44A9C6"),
    "6B": theme.hex_rgb("#ED9430"),
    "8B": theme.hex_rgb("#1D1431"),
}

FONT_FACE = theme.DEFAULT_FONT_FACE
ICON_FONT_FACE = theme.SYMBOL_FONT_FACE
TITLE_FONT_SIZE = 14
BODY_FONT_SIZE = 11
META_FONT_SIZE = 10
MODE_BADGE_FONT_SIZE = 12
STATUS_BADGE_FONT_SIZE = 9

TITLE_FONT_WEIGHT = win32con.FW_BOLD
BODY_FONT_WEIGHT = win32con.FW_SEMIBOLD
BADGE_FONT_WEIGHT = win32con.FW_BOLD
STATUS_BADGE_FONT_WEIGHT = win32con.FW_EXTRABOLD
MODE_BADGE_FONT_WEIGHT = win32con.FW_HEAVY
FOOTER_LABEL_FONT_WEIGHT = win32con.FW_NORMAL
FONT_WEIGHT_BIAS = 75
FONT_QUALITY = theme.FONT_QUALITY

FONT_CELL_HEIGHTS = {
    META_FONT_SIZE: 11,
    BODY_FONT_SIZE: 12,
}


def font_cell_height(size: int) -> int:
    return theme.font_cell_height(size, FONT_CELL_HEIGHTS)


def font_weight(weight: int) -> int:
    return theme.font_weight(weight, FONT_WEIGHT_BIAS)

PANEL_RECT = (0, 0, 360, 321)
HEADER_RECT = (8, 8, 352, 74)
SETTINGS_RECT = (316, 16, 340, 40)
BODY_TOP = 78
TAB_TOP = 79
TAB_HEIGHT = 46
TAB_GAP = 4
ROW_LEFT = 66
ROW_RIGHT = 352
ROW_TOP = 80
ROW_HEIGHT = 30
ROW_GAP = 3
FOOTER_RECT = (8, 280, 352, 310)

TEXT_FLAGS = theme.TEXT_SINGLE_LINE_FLAGS
