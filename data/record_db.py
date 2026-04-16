"""
record_db.py - 플레이 기록 로컬 캐시

(steam_id, song_id, button_mode, difficulty) → rate(float) 를 SQLite에 저장한다.
rate = 0.0  : 미플레이 또는 미클리어
rate > 0.0  : 플레이 기록 있음
rate = 100.0: Perfect Play
"""

from __future__ import annotations

import sqlite3
import threading
from pathlib import Path
from typing import Optional


class RecordDB:
    _UNKNOWN_STEAM_ID = "__unknown__"

    def __init__(self, db_path: str = "cache/record.db", steam_id: Optional[str] = None):
        self.db_path = Path(db_path)
        self.steam_id = self._normalize_steam_id(steam_id)
        self._steam_id_lock = threading.Lock()
        self.is_ready = False

    def initialize(self) -> bool:
        try:
            self.db_path.parent.mkdir(parents=True, exist_ok=True)
            with sqlite3.connect(self.db_path) as conn:
                self._create_records_table(conn)
                self._ensure_schema(conn)
                conn.commit()
            self.is_ready = True
            print(f"[RecordDB] 초기화 완료: {self.db_path} (steam_id={self.masked_steam_id})")
            return True
        except Exception as e:
            print(f"[RecordDB] 초기화 실패: {e}")
            return False

    @property
    def masked_steam_id(self) -> str:
        sid = self.get_steam_id()
        if sid == self._UNKNOWN_STEAM_ID:
            return sid
        if len(sid) <= 8:
            return "***"
        return f"{sid[:4]}...{sid[-4:]}"

    def _normalize_steam_id(self, steam_id: Optional[str]) -> str:
        if steam_id is None:
            return self._UNKNOWN_STEAM_ID
        value = str(steam_id).strip()
        if not value:
            return self._UNKNOWN_STEAM_ID
        return value

    def _create_records_table(self, conn: sqlite3.Connection):
        conn.execute("""
            CREATE TABLE IF NOT EXISTS records (
                steam_id    TEXT NOT NULL,
                song_id     TEXT NOT NULL,
                button_mode TEXT NOT NULL,
                difficulty  TEXT NOT NULL,
                rate        REAL NOT NULL,
                updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
            )
        """)

    def _table_has_column(self, conn: sqlite3.Connection, table_name: str, column_name: str) -> bool:
        rows = conn.execute(f"PRAGMA table_info({table_name})").fetchall()
        return any(r[1] == column_name for r in rows)

    def _ensure_schema(self, conn: sqlite3.Connection):
        if self._table_has_column(conn, "records", "steam_id"):
            return
        conn.execute("DROP TABLE records")
        self._create_records_table(conn)
        print("[RecordDB] 구버전 records 스키마 감지 - 테이블을 새 스키마로 초기화했습니다.")

    def set_steam_id(self, steam_id: Optional[str]) -> tuple[bool, str, str]:
        """현재 세션 steam_id를 갱신한다. (changed, before_masked, after_masked) 반환"""
        new_sid = self._normalize_steam_id(steam_id)
        with self._steam_id_lock:
            old_sid = self.steam_id
            changed = old_sid != new_sid
            self.steam_id = new_sid
        return changed, self._mask_id(old_sid), self._mask_id(new_sid)

    def get_steam_id(self) -> str:
        with self._steam_id_lock:
            return self.steam_id

    def _mask_id(self, steam_id: str) -> str:
        if steam_id == self._UNKNOWN_STEAM_ID:
            return steam_id
        if len(steam_id) <= 8:
            return "***"
        return f"{steam_id[:4]}...{steam_id[-4:]}"

    def upsert(self, song_id: int, button_mode: str, difficulty: str, rate: float) -> bool:
        """기록 저장/갱신. 기존 값보다 rate가 높을 때만 덮어씀. rate=0.0은 호출자가 걸러야 함."""
        if not self.is_ready:
            return False
        sid = str(song_id)
        steam_id = self.get_steam_id()
        try:
            with sqlite3.connect(self.db_path) as conn:
                conn.execute("""
                    INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate)
                    VALUES (?, ?, ?, ?, ?)
                    ON CONFLICT(steam_id, song_id, button_mode, difficulty) DO UPDATE SET
                        rate       = MAX(rate, excluded.rate),
                        updated_at = CAST(strftime('%s', 'now') AS INTEGER)
                """, (steam_id, sid, button_mode, difficulty, float(rate)))
                conn.commit()
            return True
        except Exception as e:
            print(f"[RecordDB] upsert 실패: {e}")
            return False

    def get(self, song_id: int, button_mode: str, difficulty: str) -> Optional[float]:
        """단건 조회. 없으면 None."""
        if not self.is_ready:
            return None
        steam_id = self.get_steam_id()
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute("""
                    SELECT rate FROM records
                    WHERE steam_id=? AND song_id=? AND button_mode=? AND difficulty=?
                """, (steam_id, str(song_id), button_mode, difficulty)).fetchone()
            return float(row[0]) if row else None
        except Exception as e:
            print(f"[RecordDB] get 실패: {e}")
            return None

    def get_all_for_song(self, song_id: int) -> dict[tuple[str, str], float]:
        """한 곡의 모든 (button_mode, difficulty) → rate 반환."""
        if not self.is_ready:
            return {}
        steam_id = self.get_steam_id()
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute("""
                    SELECT button_mode, difficulty, rate FROM records
                    WHERE steam_id=? AND song_id=?
                """, (steam_id, str(song_id))).fetchall()
            return {(r[0], r[1]): float(r[2]) for r in rows}
        except Exception as e:
            print(f"[RecordDB] get_all_for_song 실패: {e}")
            return {}

    def get_bulk(
        self,
        song_ids: list[int],
        button_mode: str,
        difficulty: str,
    ) -> dict[int, float]:
        """여러 song_id에 대해 특정 (button_mode, difficulty) rate를 한 번에 조회."""
        if not self.is_ready or not song_ids:
            return {}
        steam_id = self.get_steam_id()
        placeholders = ",".join("?" * len(song_ids))
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(f"""
                    SELECT song_id, rate FROM records
                    WHERE steam_id=?
                      AND song_id IN ({placeholders})
                      AND button_mode=? AND difficulty=?
                """, [steam_id] + [str(s) for s in song_ids] + [button_mode, difficulty]).fetchall()
            return {int(r[0]): float(r[1]) for r in rows}
        except Exception as e:
            print(f"[RecordDB] get_bulk 실패: {e}")
            return {}

    def get_rate_map(self, song_ids: list[int]) -> dict[tuple[int, str, str], float]:
        """여러 song_id에 대한 (song_id, button_mode, difficulty)→rate 맵 조회."""
        if not self.is_ready or not song_ids:
            return {}
        steam_id = self.get_steam_id()
        placeholders = ",".join("?" * len(song_ids))
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(f"""
                    SELECT song_id, button_mode, difficulty, rate
                    FROM records
                    WHERE steam_id=?
                      AND song_id IN ({placeholders})
                """, [steam_id] + [str(s) for s in song_ids]).fetchall()
            return {(int(r[0]), r[1], r[2]): float(r[3]) for r in rows}
        except Exception as e:
            print(f"[RecordDB] get_rate_map 실패: {e}")
            return {}

    def stats(self) -> dict:
        """간단한 통계 (디버그용)."""
        if not self.is_ready:
            return {}
        steam_id = self.get_steam_id()
        try:
            with sqlite3.connect(self.db_path) as conn:
                total = conn.execute(
                    "SELECT COUNT(*) FROM records WHERE steam_id=?",
                    (steam_id,),
                ).fetchone()[0]
                played = conn.execute(
                    "SELECT COUNT(*) FROM records WHERE steam_id=? AND rate > 0",
                    (steam_id,),
                ).fetchone()[0]
                perfect = conn.execute(
                    "SELECT COUNT(*) FROM records WHERE steam_id=? AND rate >= 100.0",
                    (steam_id,),
                ).fetchone()[0]
            return {
                "steam_id": self._mask_id(steam_id),
                "total": total,
                "played": played,
                "perfect": perfect,
            }
        except Exception as e:
            print(f"[RecordDB] stats 실패: {e}")
            return {}
