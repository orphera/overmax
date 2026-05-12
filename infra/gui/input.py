"""Win32 coordinate and hit-test helpers."""

from __future__ import annotations

import ctypes
import ctypes.wintypes


def client_point_in_rect(
    point: tuple[int, int],
    rect: tuple[int, int, int, int],
    scale: float,
) -> bool:
    x, y = point
    left, top, right, bottom = (round(v * scale) for v in rect)
    return left <= x <= right and top <= y <= bottom


def point_from_lparam(lparam: int) -> tuple[int, int]:
    return signed_word(lparam), signed_word(lparam >> 16)


def screen_point_from_lparam(lparam: int) -> tuple[int, int]:
    return point_from_lparam(lparam)


def signed_word(value: int) -> int:
    value &= 0xFFFF
    if value >= 0x8000:
        return value - 0x10000
    return value


def hit_test_from_lparam(lparam: int) -> int:
    return lparam & 0xFFFF


def rect_from_lparam(lparam: int) -> tuple[int, int, int, int]:
    rect = ctypes.cast(lparam, ctypes.POINTER(ctypes.wintypes.RECT)).contents
    return rect.left, rect.top, rect.right, rect.bottom

