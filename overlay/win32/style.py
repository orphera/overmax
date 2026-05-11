"""Shared visual constants for the Win32 overlay renderer."""

from __future__ import annotations

import win32api
import win32con

PANEL_BG = win32api.RGB(18, 24, 38)
HEADER_BG = win32api.RGB(30, 40, 62)
ROW_BG = win32api.RGB(36, 46, 70)
FOOTER_BG = win32api.RGB(22, 30, 48)
BADGE_BG = win32api.RGB(46, 68, 118)
BORDER = win32api.RGB(48, 58, 78)

TEXT_MAIN = win32api.RGB(240, 244, 255)
TEXT_BODY = win32api.RGB(232, 238, 255)
TEXT_MUTED = win32api.RGB(80, 88, 112)
TEXT_TAB = win32api.RGB(180, 203, 255)
TEXT_TAB_MUTED = win32api.RGB(136, 145, 167)
TEXT_ACCENT = win32api.RGB(255, 209, 102)
TEXT_RATE_HIGH = win32api.RGB(184, 220, 255)
TEXT_RATE_MID = win32api.RGB(126, 200, 227)
TEXT_RATE_SOFT = win32api.RGB(181, 234, 215)
TEXT_RATE_LOW = win32api.RGB(255, 153, 153)
STABLE = win32api.RGB(0, 212, 255)
UNSTABLE = win32api.RGB(255, 75, 75)
MAX_COMBO = win32api.RGB(74, 216, 184)
PERFECT_PLAY = win32api.RGB(255, 215, 0)

TAB_BG = win32api.RGB(26, 36, 58)
TAB_ACTIVE_BG = win32api.RGB(70, 91, 138)
DIFF_COLORS = {
    "NM": win32api.RGB(74, 144, 217),
    "HD": win32api.RGB(245, 166, 35),
    "MX": win32api.RGB(208, 2, 27),
    "SC": win32api.RGB(155, 89, 182),
}
MODE_COLORS = {
    "4B": win32api.RGB(45, 79, 85),
    "5B": win32api.RGB(68, 169, 198),
    "6B": win32api.RGB(237, 148, 48),
    "8B": win32api.RGB(29, 20, 49),
}

FONT_FACE = "Tahoma"
TITLE_FONT_SIZE = 14
BODY_FONT_SIZE = 11
META_FONT_SIZE = 10

PANEL_RECT = (0, 0, 360, 337)
HEADER_RECT = (8, 8, 352, 74)
SETTINGS_RECT = (316, 16, 340, 40)
BODY_TOP = 80
TAB_TOP = 86
TAB_HEIGHT = 46
TAB_GAP = 4
ROW_TOP = 88
ROW_HEIGHT = 30
ROW_GAP = 3
FOOTER_RECT = (8, 297, 352, 327)

TEXT_FLAGS = (
    win32con.DT_SINGLELINE
    | win32con.DT_END_ELLIPSIS
    | win32con.DT_NOPREFIX
    | win32con.DT_VCENTER
)
