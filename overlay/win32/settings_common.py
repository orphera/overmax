"""Common Win32 control helpers and layout engine for settings surfaces."""

from __future__ import annotations

from dataclasses import dataclass
import win32api
import win32con
import win32gui

from infra.gui.theme import DEFAULT_FONT_FACE

# Design Tokens - Unified to Pure White to match Tab Control background
WINDOW_BG = win32api.RGB(0xFF, 0xFF, 0xFF)
PANEL_BG = win32api.RGB(0xFF, 0xFF, 0xFF)
TEXT_COLOR = win32api.RGB(0x1F, 0x29, 0x37)
MUTED_TEXT_COLOR = win32api.RGB(0x6B, 0x72, 0x80)
BORDER_COLOR = win32api.RGB(0xE5, 0xE7, 0xEB)

@dataclass
class LayoutPadding:
    left: int = 0
    top: int = 0
    right: int = 0
    bottom: int = 0

class LayoutContext:
    """Helper to manage relative positioning and spacing in Win32 surfaces."""
    def __init__(self, rect: tuple[int, int, int, int], padding: LayoutPadding | None = None):
        self.base_x, self.base_y, self.width, self.height = rect
        self.padding = padding or LayoutPadding(16, 16, 16, 16)
        self.current_y = self.padding.top
        self.default_gap = 8

    def next_rect(self, height: int, width: int | None = None, gap: int | None = None) -> tuple[int, int, int, int]:
        if gap is None:
            gap = self.default_gap
        
        target_width = width if width is not None else (self.width - self.padding.left - self.padding.right)
        rect = (self.base_x + self.padding.left, self.base_y + self.current_y, target_width, height)
        self.current_y += height + gap
        return rect

    def add_gap(self, gap: int) -> None:
        self.current_y += gap

    def section_title(self, parent: int, hinst: int, text: str, font: int) -> int:
        rect = self.next_rect(24, gap=4)
        hwnd = static(parent, hinst, text, rect)
        win32gui.SendMessage(hwnd, win32con.WM_SETFONT, font, True)
        return hwnd

def button(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "BUTTON", text, win32con.BS_PUSHBUTTON, control_id, rect)

def check(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "BUTTON", text, win32con.BS_AUTOCHECKBOX, control_id, rect)

def static(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int], style: int = win32con.SS_LEFT) -> int:
    return control(parent, hinst, "STATIC", text, style, 0, rect)

def edit(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "EDIT", text, win32con.WS_BORDER | win32con.ES_AUTOHSCROLL, 0, rect)

def trackbar(parent: int, hinst: int, class_name: str, rect: tuple[int, int, int, int]) -> int:
    style = win32con.WS_TABSTOP
    if hasattr(win32con, "TBS_AUTOTICKS"):
        style |= win32con.TBS_AUTOTICKS
    return control(parent, hinst, class_name, "", style, 0, rect)

def tabs(parent: int, hinst: int, items: list[str], rect: tuple[int, int, int, int], control_id: int) -> int:
    hwnd = control(parent, hinst, "SysTabControl32", "", win32con.WS_CLIPSIBLINGS, control_id, rect)
    
    TCM_INSERTITEMW = 0x1300 + 62
    import ctypes
    from ctypes import wintypes
    
    class TCITEMW(ctypes.Structure):
        _fields_ = [
            ("mask", wintypes.UINT),
            ("dwState", wintypes.DWORD),
            ("dwStateMask", wintypes.DWORD),
            ("pszText", wintypes.LPWSTR),
            ("cchTextMax", ctypes.c_int),
            ("iImage", ctypes.c_int),
            ("lParam", wintypes.LPARAM),
        ]
    
    TCIF_TEXT = 0x0001
    for i, text in enumerate(items):
        item = TCITEMW()
        item.mask = TCIF_TEXT
        item.pszText = text
        win32gui.SendMessage(hwnd, TCM_INSERTITEMW, i, ctypes.addressof(item))
        
    return hwnd

def control(parent: int, hinst: int, cls: str, text: str, style: int, control_id: int, rect: tuple[int, int, int, int]) -> int:
    x, y, width, height = rect
    return win32gui.CreateWindowEx(
        0, cls, text, win32con.WS_CHILD | win32con.WS_VISIBLE | style,
        x, y, width, height, parent, control_id, hinst, None
    )

def show_many(hwnds: list[int], visible: bool) -> None:
    flag = win32con.SW_SHOW if visible else win32con.SW_HIDE
    for hwnd in hwnds:
        if hwnd: win32gui.ShowWindow(hwnd, flag)

def create_font(height: int = -15, weight: int = win32con.FW_NORMAL) -> int:
    logfont = win32gui.LOGFONT()
    logfont.lfFaceName = DEFAULT_FONT_FACE
    logfont.lfHeight = height
    logfont.lfWeight = weight
    logfont.lfQuality = win32con.CLEARTYPE_NATURAL_QUALITY
    return win32gui.CreateFontIndirect(logfont)

def text_width(hwnd: int, font: int, text: str) -> int:
    hdc = win32gui.GetDC(hwnd)
    old_font = win32gui.SelectObject(hdc, font)
    try:
        width, _ = win32gui.GetTextExtentPoint32(hdc, text)
        return int(width)
    finally:
        win32gui.SelectObject(hdc, old_font)
        win32gui.ReleaseDC(hwnd, hdc)

def center_position(size: tuple[int, int]) -> tuple[int, int]:
    w, h = size
    sw = win32api.GetSystemMetrics(win32con.SM_CXSCREEN)
    sh = win32api.GetSystemMetrics(win32con.SM_CYSCREEN)
    return ((sw - w) // 2, (sh - h) // 2)
