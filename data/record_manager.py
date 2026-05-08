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
        self._varchive_cache: dict[tuple[int, str, str], tuple[float, bool]] = {}
        self._current_steam_id: Optional[str] = None
        self._data_revision: int = 0
        self._dirty_record_keys: set[tuple[int, str, str]] = set()
        self._full_dirty: bool = True

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
                    is_max_combo = bool(rec.get("maxCombo", False))
                    if song_id is not None and diff:
                        self._varchive_cache[(song_id, button_mode, diff)] = (rate, is_max_combo)
                except (ValueError, TypeError):
                    continue
        
        print(f"[RecordManager] V-Archive 캐시 로드 완료: {len(self._varchive_cache)} 건 (steam_id={mask_steam_id(steam_id)})")
        self._data_revision += 1
        self._full_dirty = True
        self._dirty_record_keys.clear()

    def upsert(
        self, 
        song_id: int, 
        button_mode: str, 
        difficulty: str, 
        rate: float,
        is_max_combo: bool = False
    ) -> bool:
        """로컬 DB에 저장 (V-Archive 캐시는 읽기 전용)"""
        updated = self.rdb.upsert(song_id, button_mode, difficulty, rate, is_max_combo)
        if updated:
            self._data_revision += 1
            self._dirty_record_keys.add((song_id, button_mode, difficulty))
        return updated

    def delete(
        self, 
        song_id: int, 
        button_mode: str, 
        difficulty: str
    ) -> bool:
        """로컬 DB에서 기록 삭제"""
        deleted = self.rdb.delete(song_id, button_mode, difficulty)
        if deleted:
            self._data_revision += 1
            self._dirty_record_keys.add((song_id, button_mode, difficulty))
        return deleted

    def get(self, song_id: int, button_mode: str, difficulty: str) -> Optional[dict]:
        local_data = self.rdb.get(song_id, button_mode, difficulty)
        v_rate, is_max_combo = self._varchive_cache.get((song_id, button_mode, difficulty), (None, False))

        if local_data is None and v_rate is None:
            return None
            
        rate = v_rate if v_rate is not None else 0.0

        if local_data:
            rate = max(rate, local_data["rate"])
            is_max_combo = local_data["is_max_combo"] or is_max_combo
            
        return {
            "rate": rate,
            "is_max_combo": is_max_combo
        }

    def get_bulk(self, song_ids: list[int], button_mode: str, difficulty: str) -> dict[int, dict]:
        local_map = self.rdb.get_bulk(song_ids, button_mode, difficulty)
        result = local_map.copy()

        for sid in song_ids:
            v_rate, is_max_combo = self._varchive_cache.get((sid, button_mode, difficulty))
            if v_rate is not None:
                if sid not in result:
                    result[sid] = {
                        "rate": v_rate,
                        "is_max_combo": is_max_combo
                    }
                else:
                    entry = result[sid]
                    entry["rate"] = max(entry["rate"], v_rate)
                    entry["is_max_combo"] |= is_max_combo
        
        return result

    def get_rate_map(self, song_ids: list[int]) -> dict[tuple[int, str, str], dict]:
        local_map = self.rdb.get_rate_map(song_ids)
        result = local_map.copy()

        for (sid, mode, diff), (v_rate, is_max_combo) in self._varchive_cache.items():
            if sid in song_ids:
                key = (sid, mode, diff)
                if key not in result:
                    result[key] = {
                        "rate": v_rate,
                        "is_max_combo": is_max_combo
                    }
                else:
                    entry = result[key]
                    entry["rate"] = max(entry["rate"], v_rate)
                    entry["is_max_combo"] |= is_max_combo

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

    @property
    def data_revision(self) -> int:
        return self._data_revision

    def consume_dirty_info(self) -> tuple[bool, set[tuple[int, str, str]]]:
        """(full_dirty, dirty_keys)를 반환하고 dirty 상태를 소모한다."""
        full_dirty = self._full_dirty
        dirty_keys = set(self._dirty_record_keys)
        self._full_dirty = False
        self._dirty_record_keys.clear()
        return full_dirty, dirty_keys
