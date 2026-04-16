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

from data.varchive import VArchiveDB, BUTTON_MODES, DIFFICULTIES, DIFF_COLORS
from data.record_db import RecordDB
from constants import SC_GROUP, NHM_GROUP


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
    floor:       Optional[float]   # 비공식 난이도 수치 (floorName 파싱)
    floor_name:  Optional[str]     # 표시용 문자열 ex) "15.2"
    rate:        Optional[float]   # None = 미탐색
    color:       str

    @property
    def has_record(self) -> bool:
        return self.rate is not None

    @property
    def is_played(self) -> bool:
        return self.rate is not None

    @property
    def is_perfect(self) -> bool:
        return self.rate is not None and self.rate >= 100.0


class Recommender:
    def __init__(self, varchive_db: VArchiveDB, record_db: RecordDB):
        self.vdb = varchive_db
        self.rdb = record_db

    def recommend(
        self,
        song_id: int,
        button_mode: str,
        difficulty: str,
        floor_range: float = 0.0,
        max_results: int = 5,
        same_mode_only: bool = True,
    ) -> list[RecommendEntry]:
        """
        현재 패턴과 floor가 유사한 패턴 목록 반환.

        Args:
            song_id:        현재 곡 ID
            button_mode:    현재 버튼 모드 ex) "4B"
            difficulty:     현재 난이도   ex) "SC"
            floor_range:    ±이 범위 안의 floor만 포함 (기본 ±0.0)
            max_results:    최대 반환 수
            same_mode_only: True면 같은 button_mode 패턴만
        """
        # 1. 현재 패턴의 floor 파악
        current_song = self.vdb.search_by_id(song_id)
        if not current_song:
            return []

        current_pattern = (
            current_song.get("patterns", {})
            .get(button_mode, {})
            .get(difficulty)
        )
        if not current_pattern:
            return []

        floor_name_ref = current_pattern.get("floorName")
        ref_floor      = _parse_floor_value(floor_name_ref)
        use_official   = ref_floor is None   # floorName 없으면 공식 난이도 체계

        if use_official:
            ref_floor    = float(current_pattern.get("level", 0))
            ref_diff_grp = _diff_group(difficulty)

        # 2. 후보 수집
        modes_to_check = [button_mode] if same_mode_only else BUTTON_MODES
        candidates: list[RecommendEntry] = []

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

                    # ── 난이도 체계 분기 ──────────────────────────
                    cand_floor_name = p.get("floorName")
                    cand_floor      = _parse_floor_value(cand_floor_name)

                    if use_official:
                        # 공식 체계: floorName 있는 후보는 제외(척도 불일치),
                        # 같은 diff 그룹(NHM vs SC)만 비교
                        if cand_floor is not None:
                            continue
                        if _diff_group(diff) != ref_diff_grp:
                            continue
                        cand_floor = float(p.get("level", 0))
                    else:
                        # 비공식 체계: floorName 없는 후보는 제외
                        if cand_floor is None:
                            continue

                    # floor 범위 필터
                    if abs(cand_floor - ref_floor) > floor_range:
                        continue

                    # 현재 패턴 자신은 제외
                    if sid == song_id and mode == button_mode and diff == difficulty:
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
                        color=DIFF_COLORS.get(diff, "#FFFFFF"),
                    ))

        if not candidates:
            return []

        # 3. RecordDB bulk 조회
        all_ids  = list({c.song_id for c in candidates})
        rate_map: dict[tuple[int, str, str], float] = {}
        if self.rdb.is_ready:
            rate_map = self.rdb.get_rate_map(all_ids)

        for entry in candidates:
            key = (entry.song_id, entry.button_mode, entry.difficulty)
            if key in rate_map:
                entry.rate = rate_map[key]

        # 4. 정렬
        #   1. 기록 있음: rate 낮은 순 (연습이 필요한 약한 패턴 우선)
        #   2. 기록 없음: floor/level 낮은 순 (신규 도전)
        def sort_key(e: RecommendEntry) -> tuple:
            if e.is_played:
                return (0, e.rate, e.floor or 0.0)
            else:
                return (1, e.floor or 0.0, 0.0)

        candidates.sort(key=sort_key)
        return candidates[:max_results]
