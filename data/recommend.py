"""
recommend.py - 유사 난이도 패턴 추천

현재 선택된 패턴의 floor 값을 기준으로
±floor_range 범위 내 패턴을 찾고,
RecordDB의 rate를 붙여 정렬해 반환한다.

난이도 체계:
  비공식(floorName 있음): NM/HD/MX/SC 공통 척도 → 전 난이도 대상
  공식(floorName 없음):   NM/HD/MX vs SC 별도 체계
                          → SC 선택 시 SC끼리, NM/HD/MX 선택 시 NM/HD/MX끼리만

정렬 우선순위:
  1. 기록 있음(rate > 0) → rate 낮은 순  (약한 패턴 우선)
  2. 미탐색(None)        → floor 오름차순
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from data.varchive import VArchiveDB, DIFFICULTIES
from data.record_manager import RecordManager
from constants import SC_GROUP, NHM_GROUP, DIFF_COLORS


def _parse_floor_value(floor_name: Optional[str]) -> Optional[float]:
    """'15.2' → 15.2, None → None"""
    if not floor_name:
        return None
    try:
        return float(floor_name)
    except ValueError:
        return None


def _diff_group(difficulty: str) -> str:
    """공식 난이도 체계에서 같은 그룹인지 판별하기 위한 그룹 키."""
    return "SC" if difficulty in SC_GROUP else "NHM"


@dataclass
class RecommendEntry:
    song_id:     int
    song_name:   str
    composer:    str
    button_mode: str
    difficulty:  str
    level:       Optional[int]
    floor:       Optional[float]
    floor_name:  Optional[str]
    rate:        Optional[float]
    is_max_combo: bool = False

    @property
    def has_record(self) -> bool:
        return self.rate is not None

    @property
    def is_played(self) -> bool:
        return self.rate is not None

    @property
    def is_perfect_play(self) -> bool:
        return self.rate is not None and self.rate >= 100.0
        
    @property
    def is_max_combo_play(self) -> bool:
        return self.is_max_combo


@dataclass
class RecommendResult:
    entries:              list[RecommendEntry]
    avg_rate:             float
    has_record_count:     int
    total_count:          int

    @staticmethod
    def empty() -> RecommendResult:
        return RecommendResult([], -1.0, 0, 0)


class Recommender:
    def __init__(self, varchive_db: VArchiveDB, record_db: RecordManager):
        self.vdb = varchive_db
        self.rdb = record_db

    def recommend(
        self,
        song_id: int,
        button_mode: str,
        difficulty: str,
        floor_range: float = 0.0,
        max_results: int = 6,
        same_mode_only: bool = True,
    ) -> RecommendResult:
        """현재 패턴과 floor가 유사한 패턴 목록 반환."""
        current_song = self.vdb.search_by_id(song_id)
        if not current_song:
            return RecommendResult.empty()

        current_pattern = (
            current_song.get("patterns", {})
            .get(button_mode, {})
            .get(difficulty)
        )
        if not current_pattern:
            return RecommendResult.empty()

        floor_name_ref = current_pattern.get("floorName")
        ref_floor      = _parse_floor_value(floor_name_ref)
        use_official   = ref_floor is None

        if use_official:
            ref_floor    = float(current_pattern.get("level", 0))
            ref_diff_grp = _diff_group(difficulty)
        else:
            ref_diff_grp = ""

        # 1. 후보 수집
        candidates = self._get_candidates(
            song_id, button_mode, difficulty,
            ref_floor, use_official, ref_diff_grp,
            floor_range, same_mode_only
        )

        if not candidates:
            return RecommendResult.empty()

        # 2. RecordDB 레이트 병합
        self._merge_record_rates(candidates)

        # 3. 정렬 및 결과 반환
        def sort_key(e: RecommendEntry) -> tuple:
            if e.is_played:
                return (0, e.rate, e.floor or 0.0)
            else:
                return (1, e.floor or 0.0, 0.0)

        candidates.sort(key=sort_key)
        avg_rate, count = _calc_avg_rate(candidates)
        return RecommendResult(candidates[:max_results], avg_rate, count, len(candidates))

    def _get_candidates(
        self,
        target_song_id: int,
        target_mode: str,
        target_diff: str,
        ref_floor: float,
        use_official: bool,
        ref_diff_grp: str,
        floor_range: float,
        same_mode_only: bool
    ) -> list[RecommendEntry]:
        """주어진 조건에 맞는 추천 후보를 수집한다."""
        from data.varchive import BUTTON_MODES
        modes_to_check = [target_mode] if same_mode_only else BUTTON_MODES
        candidates = []

        for song in self.vdb.songs:
            try:
                sid = int(song.get("title", 0))
            except (ValueError, TypeError):
                continue
            patterns = song.get("patterns", {})

            for mode in modes_to_check:
                mode_patterns = patterns.get(mode, {})
                for diff in DIFFICULTIES:
                    p = mode_patterns.get(diff)
                    if not p:
                        continue

                    cand_floor_name = p.get("floorName")
                    cand_floor = _parse_floor_value(cand_floor_name)

                    if use_official:
                        if cand_floor is not None:
                            continue
                        if _diff_group(diff) != ref_diff_grp:
                            continue
                        cand_floor = float(p.get("level", 0))
                    else:
                        if cand_floor is None:
                            continue

                    if abs(cand_floor - ref_floor) > floor_range:
                        continue

                    if sid == target_song_id and mode == target_mode and diff == target_diff:
                        continue

                    candidates.append(RecommendEntry(
                        song_id=sid,
                        song_name=song.get("name", ""),
                        composer=song.get("composer", ""),
                        button_mode=mode,
                        difficulty=diff,
                        level=p.get("level"),
                        floor=cand_floor,
                        floor_name=cand_floor_name,
                        rate=None,
                    ))
        return candidates

    def _merge_record_rates(self, candidates: list[RecommendEntry]):
        """후보 목록에 RecordDB의 레이트 정보를 병합한다."""
        if not self.rdb.is_ready:
            return

        all_ids = list({c.song_id for c in candidates})
        rate_map = self.rdb.get_rate_map(all_ids)

        for entry in candidates:
            key = (entry.song_id, entry.button_mode, entry.difficulty)
            if key in rate_map:
                rec = rate_map[key]
                entry.rate = rec["rate"]
                entry.is_max_combo = rec["is_max_combo"]


def _calc_avg_rate(candidates: list[RecommendEntry]) -> tuple[float, int]:
    """floor 범위 내 후보 전체 중 기록 있는 패턴의 rate 평균. 없으면 -1.0."""
    rates = [e.rate for e in candidates if e.rate is not None]
    count = len(rates)
    return sum(rates) / count if rates else -1.0, count