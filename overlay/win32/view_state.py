"""Payload-to-view adapter for the Win32 main overlay candidate."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from data.recommend import RecommendEntry
from overlay.ui_payload import OverlayUpdatePayload


@dataclass(frozen=True)
class Win32PatternTab:
    difficulty: str
    label: str
    exists: bool


@dataclass(frozen=True)
class Win32Recommendation:
    difficulty: str
    level: str
    song_name: str
    rate: Optional[float]
    is_max_combo: bool


@dataclass(frozen=True)
class Win32OverlayViewState:
    title: str
    mode_diff: str
    is_stable: bool
    recommendations: list[Win32Recommendation | str]
    footer: str
    tabs: list[Win32PatternTab] = None
    active_diff: str = ""


def default_view_state() -> Win32OverlayViewState:
    return Win32OverlayViewState(
        title="RESPECT V",
        mode_diff="",
        is_stable=False,
        recommendations=["패턴을 감지하는 중..."],
        footer="",
        tabs=[],
        active_diff="",
    )


def apply_payload_to_view_state(
    state: Win32OverlayViewState,
    payload: OverlayUpdatePayload,
) -> Win32OverlayViewState:
    title = payload.song.title if payload.song is not None else state.title
    mode_diff = _format_mode_diff(payload, state.mode_diff)
    recommendations = _format_recommendations(payload, state.recommendations)
    footer = _format_footer(payload, state.footer)
    tabs = _format_tabs(payload, state.tabs or [])
    active_diff = payload.mode_diff.diff if payload.mode_diff else state.active_diff
    return Win32OverlayViewState(
        title, mode_diff, _next_stable(payload, state), recommendations,
        footer, tabs, active_diff
    )


def _format_mode_diff(payload: OverlayUpdatePayload, fallback: str) -> str:
    if payload.mode_diff is None:
        return fallback
    return f"{payload.mode_diff.mode} {payload.mode_diff.diff}".strip()


def _format_recommendations(
    payload: OverlayUpdatePayload,
    fallback: list[Win32Recommendation | str],
) -> list[Win32Recommendation | str]:
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
        return f"—— | {result.has_record_count}/{result.total_count}개 패턴"
    return f"{result.avg_rate:.2f}% | {result.has_record_count}/{result.total_count}개 패턴"


def _format_entry(entry: RecommendEntry) -> Win32Recommendation:
    level = "" if entry.level is None else str(entry.level)
    return Win32Recommendation(
        entry.difficulty, level, entry.song_name, entry.rate, entry.is_max_combo
    )


def _format_tabs(
    payload: OverlayUpdatePayload,
    fallback: list[Win32PatternTab],
) -> list[Win32PatternTab]:
    if payload.song is None or payload.mode_diff is None:
        return fallback
    patterns = _patterns_for_mode(payload.song.all_patterns, payload.mode_diff.mode)
    return [_format_tab(diff, patterns.get(diff)) for diff in ("NM", "HD", "MX", "SC")]


def _patterns_for_mode(all_patterns: list[dict], mode: str) -> dict[str, dict]:
    for group in all_patterns:
        if group.get("mode") == mode:
            return {p.get("diff", ""): p for p in group.get("patterns", [])}
    return {}


def _format_tab(diff: str, pattern: Optional[dict]) -> Win32PatternTab:
    if not pattern:
        return Win32PatternTab(diff, "—", False)
    label = pattern.get("floorName") or _level_label(pattern.get("level"))
    return Win32PatternTab(diff, label, True)


def _level_label(level: Optional[int]) -> str:
    if level is None:
        return "—"
    return f"Lv{level}"


def _next_stable(
    payload: OverlayUpdatePayload,
    state: Win32OverlayViewState,
) -> bool:
    if payload.status_changed is None:
        return state.is_stable
    return payload.status_changed
