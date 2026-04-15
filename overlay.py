"""Compatibility entrypoint for overlay classes.

`main.py` imports `OverlayController` from here, so we keep a stable import path
while implementation lives in dedicated modules.
"""

from overlay_controller import OverlayController
from overlay_window import OverlaySignals, OverlayWindow, PYQT_AVAILABLE

__all__ = ["OverlayController", "OverlaySignals", "OverlayWindow", "PYQT_AVAILABLE"]
