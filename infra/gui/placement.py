"""Window placement helpers for small Win32 GUI surfaces."""

from __future__ import annotations

from dataclasses import dataclass

import win32con
import win32gui

POSITION_FLAGS = (
    win32con.SWP_NOACTIVATE | win32con.SWP_NOSIZE | win32con.SWP_NOZORDER
)
RESIZE_FLAGS = win32con.SWP_NOACTIVATE | win32con.SWP_NOZORDER


@dataclass
class ManualPlacement:
    enabled: bool = False

    def apply(self, hwnd: int, x: int, y: int) -> tuple[int, int]:
        self.enabled = True
        move_window(hwnd, x, y)
        return window_position(hwnd)

    def should_follow_anchor(self) -> bool:
        return not self.enabled


def move_window(hwnd: int, x: int, y: int) -> None:
    win32gui.SetWindowPos(hwnd, 0, x, y, 0, 0, POSITION_FLAGS)


def resize_window(hwnd: int, width: int, height: int) -> None:
    left, top = window_position(hwnd)
    move_resize_window(hwnd, left, top, width, height)


def move_resize_window(hwnd: int, x: int, y: int, width: int, height: int) -> None:
    win32gui.SetWindowPos(hwnd, 0, x, y, width, height, RESIZE_FLAGS)


def window_position(hwnd: int) -> tuple[int, int]:
    return win32gui.GetWindowRect(hwnd)[:2]
