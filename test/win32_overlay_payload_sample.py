"""Sample payload helpers for the Win32 overlay smoke."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from core.game_state import GameSessionState
from data.recommend import RecommendEntry, RecommendResult
from overlay.ui_payload import OverlayPayloadBuilder, OverlayUpdatePayload


@dataclass(frozen=True)
class Win32OverlayViewState:
    title: str
    mode_diff: str
    is_stable: bool
    recommendations: list[str]
    footer: str


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
    return Win32OverlayViewState(
        title="RESPECT V",
        mode_diff="6B MX",
        is_stable=False,
        recommendations=["sample recommendation", "drag / noactivate / topmost"],
        footer="capture excluded / move by dragging",
    )


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


def apply_payload_to_view_state(
    state: Win32OverlayViewState,
    payload: OverlayUpdatePayload,
) -> Win32OverlayViewState:
    title = payload.song.title if payload.song is not None else state.title
    mode_diff = _format_mode_diff(payload, state.mode_diff)
    recommendations = _format_recommendations(payload, state.recommendations)
    footer = _format_footer(payload, state.footer)
    return Win32OverlayViewState(
        title, mode_diff, _next_stable(payload, state), recommendations, footer
    )


def _format_mode_diff(payload: OverlayUpdatePayload, fallback: str) -> str:
    if payload.mode_diff is None:
        return fallback
    return f"{payload.mode_diff.mode} {payload.mode_diff.diff}".strip() or "-"


def _format_recommendations(
    payload: OverlayUpdatePayload,
    fallback: list[str],
) -> list[str]:
    if payload.recommendations is None:
        return fallback
    if payload.recommendations.no_selection:
        return ["패턴을 감지하는 중..."]
    entries = payload.recommendations.result.entries
    return [_format_entry(entry) for entry in entries] or ["추천 결과 없음"]


def _format_footer(payload: OverlayUpdatePayload, fallback: str) -> str:
    if payload.recommendations is None:
        return fallback
    result = payload.recommendations.result
    if result.avg_rate < 0:
        return f"records {result.has_record_count}/{result.total_count}"
    return f"avg {result.avg_rate:.2f}% / records {result.has_record_count}/{result.total_count}"


def _format_entry(entry: RecommendEntry) -> str:
    rate = "--" if entry.rate is None else f"{entry.rate:.2f}%"
    return f"{entry.difficulty} {entry.level}  {entry.song_name}  {rate}"


def _next_stable(payload: OverlayUpdatePayload, state: Win32OverlayViewState) -> bool:
    if payload.status_changed is None:
        return state.is_stable
    return payload.status_changed
