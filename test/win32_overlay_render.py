"""Compatibility imports for older Win32 overlay smoke commands."""

from overlay.win32.render import (  # noqa: F401
    BADGE_BG,
    PANEL_BG,
    TEXT_FLAGS,
    RenderDiagnostics,
    TextLayoutCase,
    TextLayoutDiagnostics,
    build_text_layout_diagnostics,
    render_diagnostics_ok,
    text_layout_diagnostics_ok,
)
