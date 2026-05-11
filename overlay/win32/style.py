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
TEXT_ACCENT = win32api.RGB(255, 209, 102)
STABLE = win32api.RGB(0, 212, 255)
UNSTABLE = win32api.RGB(255, 75, 75)

FONT_FACE = "Segoe UI"
TITLE_FONT_SIZE = 14
BODY_FONT_SIZE = 11
META_FONT_SIZE = 10

TEXT_FLAGS = (
    win32con.DT_SINGLELINE
    | win32con.DT_END_ELLIPSIS
    | win32con.DT_NOPREFIX
    | win32con.DT_VCENTER
)
