"""Geometry helpers for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass

from overlay.utils import calculate_overlay_position

BASE_WIDTH = 360
BASE_HEIGHT = 170
BASE_MARGIN = 16
BASE_DPI = 96


@dataclass(frozen=True)
class DpiCase:
    dpi: int
    scale: float
    size: tuple[int, int]
    position: tuple[int, int]
    monitor: tuple[int, int, int, int]
    within_monitor: bool


@dataclass(frozen=True)
class PositionDiagnostics:
    calculated: tuple[int, int]
    saved: tuple[int, int]
    moved: tuple[int, int]
    callback_position: tuple[int, int]
    monitor: tuple[int, int, int, int]


def scale_for_dpi(dpi: int) -> float:
    return max(1.0, dpi / BASE_DPI)


def scaled_value(value: int, dpi: int) -> int:
    return max(1, round(value * scale_for_dpi(dpi)))


def scaled_window_size(dpi: int) -> tuple[int, int]:
    return scaled_value(BASE_WIDTH, dpi), scaled_value(BASE_HEIGHT, dpi)


def calculate_game_position(
    game_rect: tuple[int, int, int, int],
    monitor: tuple[int, int, int, int],
    dpi: int = BASE_DPI,
) -> tuple[int, int]:
    left, top, width, height = game_rect
    screen_x, screen_y, screen_right, screen_bottom = monitor
    window_width, window_height = scaled_window_size(dpi)
    margin = scaled_value(BASE_MARGIN, dpi)
    return calculate_overlay_position(
        left + width + margin,
        top + height - window_height - margin,
        window_width,
        window_height,
        screen_x,
        screen_y,
        screen_right - screen_x,
        screen_bottom - screen_y,
    )


def build_dpi_cases() -> list[DpiCase]:
    cases = [
        (96, (200, 120, 1280, 720), (0, 0, 1920, 1080)),
        (120, (260, 180, 1280, 720), (0, 0, 2560, 1440)),
        (144, (2100, 180, 1280, 720), (1920, 0, 4480, 1440)),
        (192, (-1660, 160, 1280, 720), (-1920, 0, 0, 1080)),
    ]
    return [_build_dpi_case(dpi, game_rect, monitor) for dpi, game_rect, monitor in cases]


def _build_dpi_case(
    dpi: int,
    game_rect: tuple[int, int, int, int],
    monitor: tuple[int, int, int, int],
) -> DpiCase:
    size = scaled_window_size(dpi)
    position = calculate_game_position(game_rect, monitor, dpi)
    return DpiCase(
        dpi=dpi,
        scale=scale_for_dpi(dpi),
        size=size,
        position=position,
        monitor=monitor,
        within_monitor=_within_monitor(position, size, monitor),
    )


def _within_monitor(
    position: tuple[int, int],
    size: tuple[int, int],
    monitor: tuple[int, int, int, int],
) -> bool:
    x, y = position
    width, height = size
    left, top, right, bottom = monitor
    return left <= x and top <= y and x + width <= right and y + height <= bottom
