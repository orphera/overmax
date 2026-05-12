"""Layout engine for relative positioning in Win32 surfaces."""

from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING

import win32con
import win32gui

if TYPE_CHECKING:
    from .controls import static

@dataclass
class LayoutPadding:
    """Internal spacing for a layout area."""
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
        """Calculate the rectangle for the next control in the vertical flow."""
        if gap is None:
            gap = self.default_gap
        
        target_width = width if width is not None else (self.width - self.padding.left - self.padding.right)
        rect = (self.base_x + self.padding.left, self.base_y + self.current_y, target_width, height)
        self.current_y += height + gap
        return rect

    def add_gap(self, gap: int) -> None:
        """Add manual vertical spacing."""
        self.current_y += gap

    def section_title(self, parent: int, hinst: int, text: str, font: int) -> int:
        """Create a static label as a section title with appropriate spacing."""
        from .controls import static
        rect = self.next_rect(24, gap=4)
        hwnd = static(parent, hinst, text, rect)
        win32gui.SendMessage(hwnd, win32con.WM_SETFONT, font, True)
        return hwnd
