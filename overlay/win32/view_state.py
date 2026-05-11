"""Payload-to-view adapter for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass

from data.recommend import RecommendEntry
from overlay.ui_payload import OverlayUpdatePayload


@dataclass(frozen=True)
class Win32OverlayViewState:
    title: str
    mode_diff: str
    is_stable: bool
    recommendations: list[str]
    footer: str


def default_view_state() -> Win32OverlayViewState:
    return Win32OverlayViewState(
        title="RESPECT V",
        mode_diff="",
        is_stable=False,
        recommendations=["패턴을 감지하는 중..."],
        footer="",
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
    return f"{payload.mode_diff.mode} {payload.mode_diff.diff}".strip()


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


def _next_stable(
    payload: OverlayUpdatePayload,
    state: Win32OverlayViewState,
) -> bool:
    if payload.status_changed is None:
        return state.is_stable
    return payload.status_changed
