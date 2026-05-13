import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from core.game_state import GameSessionState


FIXTURE_PATH = ROOT / "test" / "fixtures" / "game_state_cases.json"


def should_store_rate(state: GameSessionState) -> bool:
    return state.rate is not None and state.rate > 0.0


def load_cases() -> list[dict]:
    return json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))


def build_state(data: dict) -> GameSessionState:
    return GameSessionState(
        song_id=data["song_id"],
        mode=data["mode"],
        diff=data["diff"],
        is_stable=data["is_stable"],
        is_max_combo=data["is_max_combo"],
        rate=data["rate"],
    )


def main() -> int:
    for case in load_cases():
        state = build_state(case["state"])
        expected = case["expected"]

        assert state.is_valid == expected["is_valid"], case["name"]
        assert should_store_rate(state) == expected["should_store_rate"], case["name"]
        assert str(state) == expected["display"], case["name"]

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
