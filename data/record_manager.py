"""
RecordManager - Local RecordDB와 V-Archive 캐시를 머지하는 레이어
"""

from __future__ import annotations
from typing import Optional

from data.record_db import RecordDB
from data.steam_session import mask_steam_id
from data.varchive_client import VArchiveRecordClient


class RecordManager:
    def __init__(self, record_db: RecordDB, varchive_client: VArchiveRecordClient):
        self.rdb = record_db
        self.vclient = varchive_client
        self._varchive_cache: dict[tuple[int, str, str], float] = {}
        self._current_steam_id: Optional[str] = None

    def initialize(self) -> bool:
        success = self.rdb.initialize()
        self.refresh()
        return success

    def set_steam_id(self, steam_id: Optional[str]) -> tuple[bool, str, str]:
        changed, old_masked, new_masked = self.rdb.set_steam_id(steam_id)
        if changed:
            self.refresh()
        return changed, old_masked, new_masked

    def refresh(self):
        """현재 Steam ID에 해당하는 V-Archive 캐시를 메모리로 로드한다."""
        steam_id = self.rdb.get_steam_id()
        self._current_steam_id = steam_id
        self._varchive_cache = {}

        if steam_id == "__unknown__":
            return

        for button in [4, 5, 6, 8]:
            button_mode = f"{button}B"
            records = self.vclient.load_cached_records(steam_id, button)
            for rec in records:
                # rec: { "title": song_id, "pattern": "SC", "score": 99.99, ... }
                try:
                    song_id = int(rec.get("title", 0))
                    diff = rec.get("pattern")
                    rate = float(rec.get("score", 0.0))
                    if song_id is not None and diff:
                        self._varchive_cache[(song_id, button_mode, diff)] = rate
                except (ValueError, TypeError):
                    continue
        
        print(f"[RecordManager] V-Archive 캐시 로드 완료: {len(self._varchive_cache)} 건 (steam_id={mask_steam_id(steam_id)})")

    def upsert(self, song_id: int, button_mode: str, difficulty: str, rate: float) -> bool:
        """로컬 DB에 저장 (V-Archive 캐시는 읽기 전용)"""
        return self.rdb.upsert(song_id, button_mode, difficulty, rate)

    def get(self, song_id: int, button_mode: str, difficulty: str) -> Optional[float]:
        local_rate = self.rdb.get(song_id, button_mode, difficulty)
        v_rate = self._varchive_cache.get((song_id, button_mode, difficulty))

        if local_rate is None: return v_rate
        if v_rate is None: return local_rate
        return max(local_rate, v_rate)

    def get_bulk(self, song_ids: list[int], button_mode: str, difficulty: str) -> dict[int, float]:
        local_map = self.rdb.get_bulk(song_ids, button_mode, difficulty)
        result = local_map.copy()

        for sid in song_ids:
            v_rate = self._varchive_cache.get((sid, button_mode, difficulty))
            if v_rate is not None:
                if sid not in result or v_rate > result[sid]:
                    result[sid] = v_rate
        
        return result

    def get_rate_map(self, song_ids: list[int]) -> dict[tuple[int, str, str], float]:
        local_map = self.rdb.get_rate_map(song_ids)
        result = local_map.copy()

        # 모든 캐시를 순회하는 것보다 song_ids에 해당하는 것만 찾는게 빠를 수 있음
        # 하지만 _varchive_cache가 이미 필터링된 상태라면 (steam_id별) 그냥 순회해도 됨
        # 여기서는 song_ids를 기준으로 _varchive_cache에서 매칭되는 것만 병합
        for (sid, mode, diff), v_rate in self._varchive_cache.items():
            if sid in song_ids:
                key = (sid, mode, diff)
                if key not in result or v_rate > result[key]:
                    result[key] = v_rate

        return result

    def stats(self) -> dict:
        s = self.rdb.stats()
        s["varchive_cached_count"] = len(self._varchive_cache)
        return s
    
    # 델리게이션 메서드들
    def get_steam_id(self) -> str:
        return self.rdb.get_steam_id()
    
    @property
    def is_ready(self) -> bool:
        return self.rdb.is_ready
    
    @property
    def masked_steam_id(self) -> str:
        return self.rdb.masked_steam_id
