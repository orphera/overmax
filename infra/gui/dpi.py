"""DPI helpers shared by Win32 display windows."""

from __future__ import annotations

import ctypes

BASE_DPI = 96


def set_process_dpi_awareness() -> None:
    try:
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except Exception:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            pass


def get_system_dpi() -> int:
    try:
        return int(ctypes.windll.user32.GetDpiForSystem())
    except Exception:
        return BASE_DPI


def get_window_dpi(hwnd: int) -> int:
    """Get the current DPI for a specific window."""
    try:
        return ctypes.windll.user32.GetDpiForWindow(hwnd)
    except (AttributeError, OSError):
        # Fallback for older Windows or errors
        return 96


def scale_for_dpi(value: int, dpi: int) -> int:
    """Scale a logical pixel value to the target DPI."""
    return int(value * dpi / 96)


def scaled_value(value: int, dpi: int) -> int:
    """Alias for scale_for_dpi."""
    return scale_for_dpi(value, dpi)


def scaled_rect(rect: tuple[int, int, int, int], dpi: int) -> tuple[int, int, int, int]:
    """Scale a (x, y, w, h) rectangle to the target DPI."""
    return tuple(scaled_value(v, dpi) for v in rect)
