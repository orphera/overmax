"""Qt-independent payload builder for overlay updates."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Optional

from core.game_state import GameSessionState
from data.recommend import RecommendResult, Recommender
from data.varchive import BUTTON_MODES, VArchiveDB


@dataclass(frozen=True)
class SongPayload:
    title: str
    all_patterns: list[dict]


@dataclass(frozen=True)
class ModeDiffPayload:
    mode: str
    diff: str


@dataclass(frozen=True)
class RecommendationPayload:
    result: RecommendResult
    no_selection: bool


@dataclass(frozen=True)
class OverlayUpdatePayload:
    status_changed: Optional[bool] = None
    song: Optional[SongPayload] = None
    mode_diff: Optional[ModeDiffPayload] = None
    recommendations: Optional[RecommendationPayload] = None


@dataclass(frozen=True)
class _Selection:
    song_id: Optional[int] = None
    mode: Optional[str] = None
    diff: Optional[str] = None


class OverlayPayloadBuilder:
    def __init__(
        self,
        db: VArchiveDB,
        recommender: Recommender,
        log: Optional[Callable[[str], None]] = None,
    ):
        self.db = db
        self.recommender = recommender
        self.log = log
        self._selection = _Selection()
        self._last_verified: Optional[bool] = None

    @property
    def selection(self) -> _Selection:
        return self._selection

    def build_initial(self) -> OverlayUpdatePayload:
        return OverlayUpdatePayload(
            song=SongPayload("곡을 선택하세요", self._empty_patterns()),
            mode_diff=ModeDiffPayload("", ""),
            recommendations=RecommendationPayload(RecommendResult.empty(), True),
        )

    def build_state_update(self, state: GameSessionState) -> OverlayUpdatePayload:
        status_changed = self._get_status_change(state.is_stable)
        if not state.is_stable:
            return OverlayUpdatePayload(status_changed=status_changed)

        next_selection = _Selection(state.song_id, state.mode, state.diff)
        song_changed = self._selection.song_id != next_selection.song_id
        mode_diff_changed = (
            self._selection.mode != next_selection.mode
            or self._selection.diff != next_selection.diff
        )
        if not (song_changed or mode_diff_changed):
            return OverlayUpdatePayload(status_changed=status_changed)

        self._selection = next_selection
        if next_selection.song_id is None:
            return self._build_initial_with_status(status_changed)

        song, recommendations = self._build_song_and_recommendations(next_selection)
        return OverlayUpdatePayload(
            status_changed=status_changed,
            song=song if song_changed else None,
            mode_diff=self._build_mode_diff(next_selection) if mode_diff_changed else None,
            recommendations=RecommendationPayload(recommendations, False),
        )

    def _build_initial_with_status(
        self,
        status_changed: Optional[bool],
    ) -> OverlayUpdatePayload:
        initial = self.build_initial()
        return OverlayUpdatePayload(
            status_changed=status_changed,
            song=initial.song,
            mode_diff=initial.mode_diff,
            recommendations=initial.recommendations,
        )

    def build_recommendation_refresh(self) -> RecommendationPayload:
        if not self._selection_complete():
            return RecommendationPayload(RecommendResult.empty(), True)

        return RecommendationPayload(
            self.recommender.recommend(
                song_id=self._selection.song_id,
                button_mode=self._selection.mode,
                difficulty=self._selection.diff,
            ),
            False,
        )

    def _get_status_change(self, is_stable: bool) -> Optional[bool]:
        if self._last_verified == is_stable:
            return None
        self._last_verified = is_stable
        return is_stable

    def _build_song_and_recommendations(
        self,
        selection: _Selection,
    ) -> tuple[SongPayload, RecommendResult]:
        song = self._find_song(selection.song_id)
        if not song:
            return SongPayload("곡을 선택하세요", []), RecommendResult.empty()

        song_payload = SongPayload(song["name"], self._build_patterns(song))
        if not self._selection_complete():
            return song_payload, RecommendResult.empty()

        recommendations = self.recommender.recommend(
            song_id=selection.song_id,
            button_mode=selection.mode,
            difficulty=selection.diff,
        )
        return song_payload, recommendations

    def _find_song(self, song_id: Optional[int]) -> Optional[dict]:
        if song_id is None:
            return None

        song = self.db.search_by_id(song_id)
        if song is None and self.log is not None:
            self.log(f"ID={song_id}를 DB에서 찾을 수 없음")
        return song

    def _build_patterns(self, song: dict) -> list[dict]:
        return [
            {"mode": mode, "patterns": self.db.format_pattern_info(song, mode)}
            for mode in BUTTON_MODES
        ]

    def _build_mode_diff(self, selection: _Selection) -> ModeDiffPayload:
        return ModeDiffPayload(selection.mode or "", selection.diff or "")

    def _selection_complete(self) -> bool:
        return (
            self._selection.song_id is not None
            and self._selection.mode is not None
            and self._selection.diff is not None
        )

    def _empty_patterns(self) -> list[dict]:
        return [{"mode": mode, "patterns": []} for mode in BUTTON_MODES]
