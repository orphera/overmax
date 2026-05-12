"""Generic Win32 child control creation helpers."""

from __future__ import annotations
import ctypes
from ctypes import wintypes
import win32con
import win32gui

def button(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    """Create a push button."""
    return control(parent, hinst, "BUTTON", text, win32con.BS_PUSHBUTTON, control_id, rect)

def check(parent: int, hinst: int, text: str, control_id: int, rect: tuple[int, int, int, int]) -> int:
    """Create an auto-checkbox."""
    return control(parent, hinst, "BUTTON", text, win32con.BS_AUTOCHECKBOX, control_id, rect)

def static(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int], style: int = win32con.SS_LEFT) -> int:
    """Create a static text label."""
    return control(parent, hinst, "STATIC", text, style, 0, rect)

def edit(parent: int, hinst: int, text: str, rect: tuple[int, int, int, int]) -> int:
    """Create a single-line edit control."""
    return control(parent, hinst, "EDIT", text, win32con.WS_BORDER | win32con.ES_AUTOHSCROLL, 0, rect)

def trackbar(parent: int, hinst: int, class_name: str, rect: tuple[int, int, int, int]) -> int:
    """Create a trackbar (slider) control."""
    style = win32con.WS_TABSTOP
    if hasattr(win32con, "TBS_AUTOTICKS"):
        style |= win32con.TBS_AUTOTICKS
    return control(parent, hinst, class_name, "", style, 0, rect)

def tabs(parent: int, hinst: int, items: list[str], rect: tuple[int, int, int, int], control_id: int) -> int:
    """Create a native Tab Control (SysTabControl32)."""
    hwnd = control(parent, hinst, "SysTabControl32", "", win32con.WS_CLIPSIBLINGS, control_id, rect)
    
    # TCM_INSERTITEMW constant
    TCM_INSERTITEMW = 0x1300 + 62
    
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

def control(
    parent: int, 
    hinst: int, 
    cls: str, 
    text: str, 
    style: int, 
    control_id: int, 
    rect: tuple[int, int, int, int]
) -> int:
    """Foundation wrapper for CreateWindowEx for child controls."""
    x, y, width, height = rect
    return win32gui.CreateWindowEx(
        0, cls, text, win32con.WS_CHILD | win32con.WS_VISIBLE | style,
        x, y, width, height, parent, control_id, hinst, None
    )

def show_many(hwnds: list[int], visible: bool) -> None:
    """Show or hide a list of windows."""
    flag = win32con.SW_SHOW if visible else win32con.SW_HIDE
    for hwnd in hwnds:
        if hwnd: 
            win32gui.ShowWindow(hwnd, flag)

def get_edit_text(hwnd: int) -> str:
    """Get text from an edit or static control."""
    return win32gui.GetWindowText(hwnd)

def set_edit_text(hwnd: int, text: str) -> None:
    """Set text for an edit or static control."""
    win32gui.SetWindowText(hwnd, text)

def get_button_checked(hwnd: int) -> bool:
    """Check if a checkbox or radio button is checked."""
    return win32gui.SendMessage(hwnd, win32con.BM_GETCHECK, 0, 0) == win32con.BST_CHECKED

def set_button_checked(hwnd: int, checked: bool) -> None:
    """Set the checked state of a checkbox or radio button."""
    state = win32con.BST_CHECKED if checked else win32con.BST_UNCHECKED
    win32gui.SendMessage(hwnd, win32con.BM_SETCHECK, state, 0)
