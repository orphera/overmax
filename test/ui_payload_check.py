"""Smoke checks for the Qt-independent overlay payload boundary."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from core.game_state import GameSessionState
from data.recommend import RecommendResult
from data.varchive import BUTTON_MODES
from overlay.ui_payload import OverlayPayloadBuilder


class FakeDB:
    def __init__(self):
        self.song = {"title": "42", "name": "Payload Test", "patterns": {}}

    def search_by_id(self, song_id: int):
        return self.song if song_id == 42 else None

    def format_pattern_info(self, song: dict, button_mode: str) -> list[dict]:
        return [{"diff": "MX", "level": 12, "mode": button_mode}]


class FakeRecommender:
    def __init__(self):
        self.calls = []

    def recommend(self, song_id: int, button_mode: str, difficulty: str):
        self.calls.append((song_id, button_mode, difficulty))
        return RecommendResult.empty()


def main():
    recommender = FakeRecommender()
    builder = OverlayPayloadBuilder(FakeDB(), recommender)

    initial = builder.build_initial()
    assert initial.song.title == "곡을 선택하세요"
    assert [item["mode"] for item in initial.song.all_patterns] == BUTTON_MODES
    assert initial.recommendations.no_selection is True

    detecting = builder.build_state_update(GameSessionState(42, "4B", "MX"))
    assert detecting.status_changed is False
    assert detecting.song is None

    stable = builder.build_state_update(GameSessionState(42, "4B", "MX", is_stable=True))
    assert stable.status_changed is True
    assert stable.song.title == "Payload Test"
    assert stable.mode_diff.mode == "4B"
    assert stable.mode_diff.diff == "MX"
    assert stable.recommendations.no_selection is False
    assert recommender.calls[-1] == (42, "4B", "MX")

    repeated = builder.build_state_update(GameSessionState(42, "4B", "MX", is_stable=True))
    assert repeated.status_changed is None
    assert repeated.song is None
    assert repeated.recommendations is None

    refreshed = builder.build_recommendation_refresh()
    assert refreshed.no_selection is False
    assert recommender.calls[-1] == (42, "4B", "MX")

    source = Path("overlay/ui_payload.py").read_text(encoding="utf-8")
    assert "PyQt6" not in source


if __name__ == "__main__":
    main()
    print("ui_payload_check_ok")
