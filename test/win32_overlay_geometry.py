"""Compatibility imports for older Win32 overlay smoke commands."""

from overlay.win32.geometry import (  # noqa: F401
    BASE_DPI,
    BASE_HEIGHT,
    BASE_MARGIN,
    BASE_WIDTH,
    DpiCase,
    PositionDiagnostics,
    build_dpi_cases,
    calculate_game_position,
    scale_for_dpi,
    scaled_value,
    scaled_window_size,
)
