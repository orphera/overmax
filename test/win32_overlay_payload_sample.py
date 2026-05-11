"""Sample payload helpers for the Win32 overlay smoke."""

from __future__ import annotations

from typing import Optional

from core.game_state import GameSessionState
from data.recommend import RecommendEntry, RecommendResult
from overlay.ui_payload import OverlayPayloadBuilder
from overlay.win32.view_state import (
    Win32OverlayViewState,
    apply_payload_to_view_state,
    default_view_state as production_default_view_state,
)


class FakeDB:
    def __init__(self) -> None:
        self.song = {
            "title": "42",
            "name": "Payload Test",
            "patterns": {
                "6B": {
                    "MX": {"diff": "MX", "level": 12, "floorName": "12.4"},
                    "SC": {"diff": "SC", "level": 13, "floorName": "13.1"},
                }
            },
        }

    def search_by_id(self, song_id: int) -> Optional[dict]:
        return self.song if song_id == 42 else None

    def format_pattern_info(self, song: dict, button_mode: str) -> list[dict]:
        return list(song.get("patterns", {}).get(button_mode, {}).values())


class FakeRecommender:
    def recommend(
        self,
        song_id: int,
        button_mode: str,
        difficulty: str,
    ) -> RecommendResult:
        return RecommendResult(
            [
                RecommendEntry(
                    101, "Rising Payload", "sample", button_mode,
                    difficulty, 12, 12.4, "12.4", 98.76,
                ),
                RecommendEntry(
                    202, "No Record Sample", "sample", button_mode,
                    difficulty, 12, 12.5, "12.5", None,
                ),
            ],
            97.42,
            8,
            12,
        )


def default_view_state() -> Win32OverlayViewState:
    return production_default_view_state()


def sample_payload_view_state() -> Win32OverlayViewState:
    builder = OverlayPayloadBuilder(FakeDB(), FakeRecommender())
    state = default_view_state()
    state = apply_payload_to_view_state(state, builder.build_initial())
    payload = builder.build_state_update(
        GameSessionState(42, "6B", "MX", is_stable=True)
    )
    return apply_payload_to_view_state(state, payload)


def long_payload_view_state() -> Win32OverlayViewState:
    return Win32OverlayViewState(
        title="Very Long Payload Title For Win32 Overlay Density Check",
        mode_diff="8B SC",
        is_stable=True,
        recommendations=[
            "SC 15  Extremely Long Recommendation Song Name For Elide Check  99.99%",
            "MX 14  Another Long Candidate With No Record Marker  --",
        ],
        footer="avg 100.00% / records 123/456 / long footer density sample",
    )


def mixed_unstable_payload_view_state() -> Win32OverlayViewState:
    return Win32OverlayViewState(
        title="한글 English Mixed Title For Style Check",
        mode_diff="5B HD",
        is_stable=False,
        recommendations=[
            "HD 11  한글 Recommendation Mixed Candidate  97.31%",
            "MX 13  English Candidate Without Local Record  --",
        ],
        footer="avg 97.31% / records 1/2 / opacity scale visual sample",
    )
