"""
sync_manager.py - overmax 수집 기록 vs V-Archive 기록 비교

등록 후보 조건:
  1. overmax rate > v아카이브 rate
  2. overmax에만 있는 패턴 (v아카이브 기록 없음)
  3. rate는 같지만 overmax is_max_combo=True, v아카이브=False
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from data.varchive import VArchiveDB
from data.record_manager import RecordManager


@dataclass
class SyncCandidate:
    song_id:       int
    song_name:     str
    composer:      str
    dlc:           str        # songs.json의 dlcNo 또는 pack 필드
    button_mode:   str
    difficulty:    str
    overmax_rate:  float
    overmax_mc:    bool
    varchive_rate: Optional[float]   # None = v아카이브에 없음
    varchive_mc:   Optional[bool]

    @property
    def rate_diff(self) -> Optional[float]:
        if self.varchive_rate is None:
            return None
        return self.overmax_rate - self.varchive_rate

    @property
    def reason(self) -> str:
        """후보 이유 요약 (표시용)"""
        parts = []
        if self.varchive_rate is None:
            parts.append("미등록")
        elif self.overmax_rate > self.varchive_rate:
            parts.append(f"+{self.overmax_rate - self.varchive_rate:.2f}%")
        if self.overmax_mc and not self.varchive_mc:
            parts.append("MC")
        return " · ".join(parts)

    # 등록 API 호출 후 상태 업데이트용
    upload_status: str = ""   # "" | "success" | "no_update" | "error"
    upload_message: str = ""


def build_candidates(
    varchive_db: VArchiveDB,
    record_manager: RecordManager,
    steam_id: str,
) -> list[SyncCandidate]:
    """
    지정된 steam_id 기준으로 RecordManager의 로컬 기록과 V-Archive 캐시를 비교하여
    등록 후보 목록을 반환한다.
    """
    if not steam_id or steam_id == "__unknown__":
        return []

    # 로컬 DB 전체 조회 — 전달된 steam_id 기반
    local_map = _load_all_local(record_manager, steam_id)
    if not local_map:
        return []

    # V-Archive 캐시 참조 (record_manager 내부 캐시 직접 접근)
    varchive_cache = record_manager._varchive_cache   # {(song_id, mode, diff): (rate, mc)}

    candidates: list[SyncCandidate] = []

    for (song_id, mode, diff), (local_rate, local_mc) in local_map.items():
        if local_rate <= 0.0:
            continue

        v_entry = varchive_cache.get((song_id, mode, diff))
        v_rate: Optional[float] = v_entry[0] if v_entry else None
        v_mc: Optional[bool]   = v_entry[1] if v_entry else None

        # 후보 조건 검사
        is_candidate = (
            v_rate is None                          # 미등록
            or local_rate > v_rate                  # 더 좋은 rate
            or (local_mc and not v_mc)              # MC 신규
        )
        if not is_candidate:
            continue

        song = varchive_db.search_by_id(song_id)
        song_name = song["name"] if song else str(song_id)
        composer  = song.get("composer", "") if song else ""
        dlc       = song.get("dlcCode", "") if song else ""

        candidates.append(SyncCandidate(
            song_id=song_id,
            song_name=song_name,
            composer=composer,
            dlc=dlc,
            button_mode=mode,
            difficulty=diff,
            overmax_rate=local_rate,
            overmax_mc=local_mc,
            varchive_rate=v_rate,
            varchive_mc=v_mc,
        ))

    # 정렬: rate 차이 큰 순 → 미등록 → MC만 다른 순
    candidates.sort(key=_sort_key)
    return candidates


def _sort_key(c: SyncCandidate) -> tuple:
    if c.varchive_rate is None:
        return (1, -(c.overmax_rate))
    diff = c.overmax_rate - c.varchive_rate
    if diff > 0:
        return (0, -diff)
    return (2, 0.0)


def _load_all_local(
    record_manager: RecordManager,
    steam_id: str,
) -> dict[tuple[int, str, str], tuple[float, bool]]:
    """RecordDB에서 지정된 steam_id의 전체 기록을 가져온다."""
    rdb = record_manager.rdb
    if not rdb.is_ready:
        return {}

    if not steam_id or steam_id == "__unknown__":
        return {}

    import sqlite3
    try:
        with sqlite3.connect(rdb.db_path) as conn:
            rows = conn.execute(
                """
                SELECT song_id, button_mode, difficulty, rate, is_max_combo
                FROM records
                WHERE steam_id = ? AND rate > 0
                """,
                (steam_id,),
            ).fetchall()
        return {
            (int(r[0]), r[1], r[2]): (float(r[3]), bool(r[4]))
            for r in rows
        }
    except Exception as e:
        print(f"[SyncManager] 로컬 기록 조회 실패: {e}")
        return {}
