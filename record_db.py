"""
record_db.py - 플레이 기록 로컬 캐시

(song_id, button_mode, difficulty) → rate(float) 를 SQLite에 저장한다.
rate = 0.0  : 미플레이 또는 미클리어
rate > 0.0  : 플레이 기록 있음
rate = 100.0: Perfect Play
"""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Optional


class RecordDB:
    def __init__(self, db_path: str = "cache/record.db"):
        self.db_path = Path(db_path)
        self.is_ready = False

    def initialize(self) -> bool:
        try:
            self.db_path.parent.mkdir(parents=True, exist_ok=True)
            with sqlite3.connect(self.db_path) as conn:
                conn.execute("""
                    CREATE TABLE IF NOT EXISTS records (
                        song_id     TEXT NOT NULL,
                        button_mode TEXT NOT NULL,
                        difficulty  TEXT NOT NULL,
                        rate        REAL NOT NULL,
                        updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                        PRIMARY KEY (song_id, button_mode, difficulty)
                    )
                """)
                conn.commit()
            self.is_ready = True
            print(f"[RecordDB] 초기화 완료: {self.db_path}")
            return True
        except Exception as e:
            print(f"[RecordDB] 초기화 실패: {e}")
            return False

    def upsert(self, song_id: int, button_mode: str, difficulty: str, rate: float) -> bool:
        """기록 저장/갱신. 기존 값보다 rate가 높을 때만 덮어씀. rate=0.0은 호출자가 걸러야 함."""
        if not self.is_ready:
            return False
        sid = str(song_id)
        try:
            with sqlite3.connect(self.db_path) as conn:
                conn.execute("""
                    INSERT INTO records (song_id, button_mode, difficulty, rate)
                    VALUES (?, ?, ?, ?)
                    ON CONFLICT(song_id, button_mode, difficulty) DO UPDATE SET
                        rate       = MAX(rate, excluded.rate),
                        updated_at = CAST(strftime('%s', 'now') AS INTEGER)
                """, (sid, button_mode, difficulty, float(rate)))
                conn.commit()
            return True
        except Exception as e:
            print(f"[RecordDB] upsert 실패: {e}")
            return False

    def get(self, song_id: int, button_mode: str, difficulty: str) -> Optional[float]:
        """단건 조회. 없으면 None."""
        if not self.is_ready:
            return None
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute("""
                    SELECT rate FROM records
                    WHERE song_id=? AND button_mode=? AND difficulty=?
                """, (str(song_id), button_mode, difficulty)).fetchone()
            return float(row[0]) if row else None
        except Exception as e:
            print(f"[RecordDB] get 실패: {e}")
            return None

    def get_all_for_song(self, song_id: int) -> dict[tuple[str, str], float]:
        """한 곡의 모든 (button_mode, difficulty) → rate 반환."""
        if not self.is_ready:
            return {}
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute("""
                    SELECT button_mode, difficulty, rate FROM records
                    WHERE song_id=?
                """, (str(song_id),)).fetchall()
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
        placeholders = ",".join("?" * len(song_ids))
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(f"""
                    SELECT song_id, rate FROM records
                    WHERE song_id IN ({placeholders})
                      AND button_mode=? AND difficulty=?
                """, [str(s) for s in song_ids] + [button_mode, difficulty]).fetchall()
            return {int(r[0]): float(r[1]) for r in rows}
        except Exception as e:
            print(f"[RecordDB] get_bulk 실패: {e}")
            return {}

    def stats(self) -> dict:
        """간단한 통계 (디버그용)."""
        if not self.is_ready:
            return {}
        try:
            with sqlite3.connect(self.db_path) as conn:
                total = conn.execute("SELECT COUNT(*) FROM records").fetchone()[0]
                played = conn.execute(
                    "SELECT COUNT(*) FROM records WHERE rate > 0"
                ).fetchone()[0]
                perfect = conn.execute(
                    "SELECT COUNT(*) FROM records WHERE rate >= 100.0"
                ).fetchone()[0]
            return {"total": total, "played": played, "perfect": perfect}
        except Exception as e:
            print(f"[RecordDB] stats 실패: {e}")
            return {}
