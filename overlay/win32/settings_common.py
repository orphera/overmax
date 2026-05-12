"""Common Win32 control helpers for settings surfaces."""

from __future__ import annotations

import win32api
import win32con
import win32gui

from infra.gui.theme import DEFAULT_FONT_FACE


def button(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "BUTTON", text, win32con.BS_PUSHBUTTON, control_id, rect)


def check(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "BUTTON", text, win32con.BS_AUTOCHECKBOX, control_id, rect)


def static(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "STATIC", text, win32con.SS_LEFT, 0, rect)


def edit(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int]) -> int:
    return control(parent, hinst, "EDIT", text, win32con.WS_BORDER | win32con.ES_AUTOHSCROLL, 0, rect)


def trackbar(parent: int, hinst: int, class_name: str, rect: tuple[int, int, int, int]) -> int:
    style = win32con.WS_TABSTOP
    if hasattr(win32con, "TBS_AUTOTICKS"):
        style |= win32con.TBS_AUTOTICKS
    return control(parent, hinst, class_name, "", style, 0, rect)


def control(parent: int, hinst: int, cls: str, text: str, style: int, control_id: int, rect: tuple[int, int, int, int]) -> int:
    x, y, width, height = rect
    return win32gui.CreateWindowEx(
        0,
        cls,
        text,
        win32con.WS_CHILD | win32con.WS_VISIBLE | style,
        x,
        y,
        width,
        height,
        parent,
        control_id,
        hinst,
        None,
    )


def show_many(hwnds: list[int], visible: bool) -> None:
    flag = win32con.SW_SHOW if visible else win32con.SW_HIDE
    for hwnd in hwnds:
        if hwnd:
            win32gui.ShowWindow(hwnd, flag)


def create_font(height: int = -15) -> int:
    logfont = win32gui.LOGFONT()
    logfont.lfFaceName = DEFAULT_FONT_FACE
    logfont.lfHeight = height
    logfont.lfWeight = win32con.FW_NORMAL
    logfont.lfQuality = win32con.CLEARTYPE_NATURAL_QUALITY
    return win32gui.CreateFontIndirect(logfont)


def text_width(hwnd: int, font: int, text: str) -> int:
    hdc = win32gui.GetDC(hwnd)
    old_font = win32gui.SelectObject(hdc, font)
    try:
        width, _height = win32gui.GetTextExtentPoint32(hdc, text)
        return int(width)
    finally:
        win32gui.SelectObject(hdc, old_font)
        win32gui.ReleaseDC(hwnd, hdc)


def center_position(size: tuple[int, int]) -> tuple[int, int]:
    width, height = size
    screen_width = win32api.GetSystemMetrics(win32con.SM_CXSCREEN)
    screen_height = win32api.GetSystemMetrics(win32con.SM_CYSCREEN)
    return ((screen_width - width) // 2, (screen_height - height) // 2)
