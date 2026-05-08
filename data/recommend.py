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


@dataclass(frozen=True)
class FloorCacheKey:
    button_mode: str
    scale_type: str
    floor: float


@dataclass
class FloorRateSummary:
    total_count: int = 0
    has_record_count: int = 0
    rate_sum: float = 0.0

    @property
    def avg_rate(self) -> float:
        if self.has_record_count <= 0:
            return -1.0
        return self.rate_sum / self.has_record_count


class Recommender:
    def __init__(self, varchive_db: VArchiveDB, record_db: RecordManager):
        self.vdb = varchive_db
        self.rdb = record_db
        self._floor_rate_cache: dict[FloorCacheKey, FloorRateSummary] = {}
        self._floor_rate_dirty: dict[FloorCacheKey, bool] = {}
        self._floor_patterns: dict[FloorCacheKey, list[tuple[int, str, str]]] = {}
        self._record_to_floor_key: dict[tuple[int, str, str], FloorCacheKey] = {}
        self._cache_index_ready: bool = False

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
        summary = self._get_summary_from_cache(
            song_id=song_id,
            button_mode=button_mode,
            difficulty=difficulty,
            ref_floor=ref_floor,
            use_official=use_official,
            floor_range=floor_range,
            same_mode_only=same_mode_only,
        )
        return RecommendResult(
            candidates[:max_results],
            summary.avg_rate,
            summary.has_record_count,
            summary.total_count,
        )

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

    def _ensure_floor_rate_cache(self):
        if not self._cache_index_ready:
            self._build_floor_cache_index()

        full_dirty, dirty_keys = self.rdb.consume_dirty_info()
        if full_dirty:
            for key in self._floor_patterns:
                self._floor_rate_dirty[key] = True
        else:
            for record_key in dirty_keys:
                floor_key = self._record_to_floor_key.get(record_key)
                if floor_key is not None:
                    self._floor_rate_dirty[floor_key] = True

        dirty_floor_keys = [k for k, is_dirty in self._floor_rate_dirty.items() if is_dirty]
        if not dirty_floor_keys:
            return

        all_song_ids = []
        for song in self.vdb.songs:
            try:
                song_id = int(song.get("title", 0))
            except (ValueError, TypeError):
                continue
            all_song_ids.append(song_id)
        rate_map = self.rdb.get_rate_map(list(set(all_song_ids))) if all_song_ids else {}
        for key in dirty_floor_keys:
            entries = self._floor_patterns.get(key, [])
            summary = FloorRateSummary(total_count=len(entries))
            for song_id, mode, diff in entries:
                rec = rate_map.get((song_id, mode, diff))
                if not rec:
                    continue
                rate = rec.get("rate", 0.0)
                if rate <= 0.0:
                    continue
                summary.has_record_count += 1
                summary.rate_sum += rate
            self._floor_rate_cache[key] = summary
            self._floor_rate_dirty[key] = False

    def _build_floor_cache_index(self):
        floor_patterns: dict[FloorCacheKey, list[tuple[int, str, str]]] = {}
        record_to_floor_key: dict[tuple[int, str, str], FloorCacheKey] = {}

        for song in self.vdb.songs:
            try:
                song_id = int(song.get("title", 0))
            except (ValueError, TypeError):
                continue
            patterns = song.get("patterns", {})
            for mode, mode_patterns in patterns.items():
                for diff in DIFFICULTIES:
                    p = mode_patterns.get(diff)
                    if not p:
                        continue
                    floor_name = p.get("floorName")
                    floor_val = _parse_floor_value(floor_name)
                    if floor_val is None:
                        level = p.get("level")
                        if level is None:
                            continue
                        floor_val = float(level)
                        scale_type = "OFFICIAL_SC" if diff in SC_GROUP else "OFFICIAL_NHM"
                    else:
                        scale_type = "UNOFFICIAL"
                    key = FloorCacheKey(mode, scale_type, floor_val)
                    record_key = (song_id, mode, diff)
                    floor_patterns.setdefault(key, []).append(record_key)
                    record_to_floor_key[record_key] = key

        self._floor_patterns = floor_patterns
        self._record_to_floor_key = record_to_floor_key
        self._floor_rate_cache = {
            key: FloorRateSummary(total_count=len(entries))
            for key, entries in self._floor_patterns.items()
        }
        self._floor_rate_dirty = {key: True for key in self._floor_patterns}
        self._cache_index_ready = True

    def _get_summary_from_cache(
        self,
        song_id: int,
        button_mode: str,
        difficulty: str,
        ref_floor: float,
        use_official: bool,
        floor_range: float,
        same_mode_only: bool,
    ) -> FloorRateSummary:
        self._ensure_floor_rate_cache()

        if use_official:
            scale_type = "OFFICIAL_SC" if difficulty in SC_GROUP else "OFFICIAL_NHM"
        else:
            scale_type = "UNOFFICIAL"

        from data.varchive import BUTTON_MODES
        modes = [button_mode] if same_mode_only else BUTTON_MODES

        total = 0
        has_record = 0
        rate_sum = 0.0
        for key, summary in self._floor_rate_cache.items():
            if key.button_mode not in modes:
                continue
            if key.scale_type != scale_type:
                continue
            if abs(key.floor - ref_floor) > floor_range:
                continue
            total += summary.total_count
            has_record += summary.has_record_count
            rate_sum += summary.rate_sum

        return FloorRateSummary(
            total_count=max(0, total),
            has_record_count=max(0, has_record),
            rate_sum=max(0.0, rate_sum),
        )
