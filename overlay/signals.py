"""Overlay signals implementation using native signals."""

from __future__ import annotations

from typing import Any
from infra.gui.signals import Signal
from data.recommend import RecommendResult


class OverlaySignals:
    def __init__(self) -> None:
        self.song_changed = Signal()                # (title: str, all_patterns: list)
        self.screen_changed = Signal()              # (is_song_select: bool)
        self.position_changed = Signal()            # (left, top, width, height)
        self.roi_enabled_changed = Signal()         # (enabled: bool)
        self.mode_diff_changed = Signal()           # (mode: str, diff: str)
        self.recommend_ready = Signal()             # (recommendations: RecommendResult, no_selection: bool)
        self.visibility_toggle_requested = Signal()
        self.status_changed = Signal()              # (is_stable: bool)
        self.confidence_changed = Signal()          # (confidence: float)
        self.settings_requested = Signal()
        self.scale_changed = Signal()               # (scale: float)
