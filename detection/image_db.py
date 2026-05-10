from __future__ import annotations

import sqlite3
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import numpy as np
from overmax_cv import hashes_gray, hog_gray, image_features


@dataclass
class _CachedEntry:
    image_id: str
    phash_int: int    # 검색 시 XOR 연산용 — 로드 시점에 변환
    dhash_int: int
    ahash_int: int
    hog: np.ndarray   # float32


class ImageDB:
    """Perceptual-hash + HOG 기반 경량 이미지 매칭 DB.

    initialize() 시점에 전체 rows를 메모리로 로드한다.
    search()는 DB I/O 없이 캐시만 사용한다.
    register() / delete_entry()는 DB 쓰기 후 캐시를 incremental 갱신한다.
    """

    def __init__(self, db_path: str = "cache/image_index.db", similarity_threshold: float = 0.7):
        self.db_path = Path(db_path)
        self.similarity_threshold = float(similarity_threshold)
        self.is_ready = False
        self.song_count = 0

        self._cache: list[_CachedEntry] = []
        self._cache_lock = threading.Lock()

        # 벡터 연산용 캐시
        self._image_ids: np.ndarray = np.array([])
        self._phash_arr: np.ndarray = np.array([], dtype=np.uint64)
        self._dhash_arr: np.ndarray = np.array([], dtype=np.uint64)
        self._ahash_arr: np.ndarray = np.array([], dtype=np.uint64)
        self._hog_arr: np.ndarray = np.empty((0, 1764), dtype=np.float32)
        self._hog_norms: np.ndarray = np.array([], dtype=np.float32)

    # ------------------------------------------------------------------
    # 초기화 / 로드
    # ------------------------------------------------------------------

    def initialize(self) -> bool:
        try:
            self.db_path.parent.mkdir(parents=True, exist_ok=True)
            with sqlite3.connect(self.db_path) as conn:
                self._ensure_schema(conn)
                conn.commit()
            self.is_ready = True
            return True
        except Exception as e:
            print(f"[ImageDB] 초기화 실패: {e}")
            self.is_ready = False
            return False

    def load(self) -> int:
        """DB 전체를 캐시로 로드한다. initialize() 후 또는 강제 리프레시 시 호출."""
        if not self.is_ready:
            return 0
        count = self._load_cache_from_db()
        print(f"[ImageDB] 로드 완료: {count}곡")
        return count

    def _load_cache_from_db(self) -> int:
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(
                    "SELECT image_id, phash, dhash, ahash, hog FROM images"
                ).fetchall()
        except Exception as e:
            print(f"[ImageDB] 캐시 로드 실패: {e}")
            return 0

        entries = [_row_to_entry(r) for r in rows]
        with self._cache_lock:
            self._cache = entries
            self.song_count = len(entries)
            self._rebuild_vectors()
        return self.song_count

    def _rebuild_vectors(self):
        """현재 _cache 데이터를 기반으로 Numpy 벡터 캐시를 재구성한다."""
        if not self._cache:
            self._image_ids = np.array([])
            self._phash_arr = np.array([], dtype=np.uint64)
            self._dhash_arr = np.array([], dtype=np.uint64)
            self._ahash_arr = np.array([], dtype=np.uint64)
            self._hog_arr = np.empty((0, 1764), dtype=np.float32)
            self._hog_norms = np.array([], dtype=np.float32)
            return

        self._image_ids = np.array([e.image_id for e in self._cache])
        self._phash_arr = np.array([e.phash_int for e in self._cache], dtype=np.uint64)
        self._dhash_arr = np.array([e.dhash_int for e in self._cache], dtype=np.uint64)
        self._ahash_arr = np.array([e.ahash_int for e in self._cache], dtype=np.uint64)
        self._hog_arr = np.vstack([e.hog for e in self._cache])
        
        norms = np.linalg.norm(self._hog_arr, axis=1)
        norms[norms == 0] = 1.0
        self._hog_norms = norms.astype(np.float32)

    def _ensure_schema(self, conn: sqlite3.Connection):
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS images (
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                image_id TEXT NOT NULL,
                phash    TEXT NOT NULL,
                dhash    TEXT NOT NULL,
                ahash    TEXT NOT NULL,
                hog      BLOB NOT NULL,
                orb      BLOB
            )
            """
        )
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_images_image_id ON images (image_id)"
        )
        conn.execute(
            """
            DELETE FROM images
            WHERE id NOT IN (
                SELECT MAX(id) FROM images GROUP BY image_id
            )
            """
        )
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS uq_images_image_id ON images (image_id)"
        )

    # ------------------------------------------------------------------
    # 검색 (캐시 전용)
    # ------------------------------------------------------------------

    def search(self, img: np.ndarray, top_k: int = 10) -> Optional[tuple[str, float]]:
        features = _compute_features(img)
        if features is None:
            return None
        q_ph, q_dh, q_ah, q_hog = features
        q_ph_uint = np.uint64(int(q_ph, 16))
        q_dh_uint = np.uint64(int(q_dh, 16))
        q_ah_uint = np.uint64(int(q_ah, 16))

        with self._cache_lock:
            n = len(self._image_ids)
            if n == 0:
                return None
            phash_arr = self._phash_arr
            dhash_arr = self._dhash_arr
            ahash_arr = self._ahash_arr
            hog_arr = self._hog_arr
            hog_norms = self._hog_norms
            image_ids = self._image_ids

        # 1단계: 벡터화된 Popcount 기반 Hash Distance 계산
        def popcount64(x):
            x = x - ((x >> 1) & np.uint64(0x5555555555555555))
            x = (x & np.uint64(0x3333333333333333)) + ((x >> 2) & np.uint64(0x3333333333333333))
            x = (x + (x >> 4)) & np.uint64(0x0f0f0f0f0f0f0f0f)
            return (x * np.uint64(0x0101010101010101)) >> 56

        dist_ph = popcount64(phash_arr ^ q_ph_uint)
        dist_dh = popcount64(dhash_arr ^ q_dh_uint)
        dist_ah = popcount64(ahash_arr ^ q_ah_uint)
        
        # Distances: lower is better
        h_scores = 0.5 * dist_ph + 0.3 * dist_dh + 0.2 * dist_ah
        
        # 2단계: hash 점수로 top_k 후보 선별
        k = min(n, top_k)
        if k == 0:
            return None
            
        # np.argpartition이 가장 빠르지만 통상 N이 2000이하이고 python C 레벨의 argsort가 <1ms이므로 단순 정렬 사용.
        idx = np.argsort(h_scores)[:k]
        
        top_k_hscores = h_scores[idx]
        top_k_hogs = hog_arr[idx]
        top_k_norms = hog_norms[idx]
        top_k_ids = image_ids[idx]

        # 3단계: HOG 코사인 유사도 연산 (벡터화)
        q_norm = np.linalg.norm(q_hog)
        if q_norm == 0:
            q_norm = 1.0
            
        # q_hog (1764,) 와 top_k_hogs (k, 1764) 의 내적 -> (k,)
        dots = np.dot(top_k_hogs, q_hog)
        hog_sims = dots / (top_k_norms * q_norm)
        
        hash_sims = np.clip(1.0 - top_k_hscores / 64.0, 0.0, 1.0)
        similarities = 0.45 * hash_sims + 0.55 * hog_sims
        
        best_idx = int(np.argmax(similarities))
        best_sim = float(similarities[best_idx])
        
        if best_sim >= self.similarity_threshold:
            return str(top_k_ids[best_idx]), best_sim
        return None

    # ------------------------------------------------------------------
    # 등록 (DB 쓰기 + 캐시 incremental upsert)
    # ------------------------------------------------------------------

    def register(self, song_id: str, img: np.ndarray) -> bool:
        if not self.is_ready:
            print("[ImageDB] 미초기화 상태 - register 불가")
            return False
        sid = str(song_id).strip()
        if not sid:
            print("[ImageDB] song_id 비어있음")
            return False

        features = _compute_features(img)
        if features is None:
            print("[ImageDB] 이미지 변환 실패")
            return False

        ph, dh, ah, hog = features

        try:
            with sqlite3.connect(self.db_path) as conn:
                conn.execute(
                    """
                    INSERT INTO images (image_id, phash, dhash, ahash, hog, orb)
                    VALUES (?, ?, ?, ?, ?, NULL)
                    ON CONFLICT(image_id) DO UPDATE SET
                        phash = excluded.phash,
                        dhash = excluded.dhash,
                        ahash = excluded.ahash,
                        hog   = excluded.hog,
                        orb   = NULL
                    """,
                    (sid, ph, dh, ah, hog.tobytes()),
                )
                conn.commit()
        except Exception as e:
            print(f"[ImageDB] 등록 실패: {e}")
            return False

        entry = _make_entry(sid, ph, dh, ah, hog)
        with self._cache_lock:
            idx = next((i for i, e in enumerate(self._cache) if e.image_id == sid), None)
            if idx is not None:
                self._cache[idx] = entry
            else:
                self._cache.append(entry)
            self.song_count = len(self._cache)
            self._rebuild_vectors()

        print(f"[ImageDB] 등록/갱신 완료: '{sid}'")
        return True

    # ------------------------------------------------------------------
    # 삭제 (DB + 캐시)
    # ------------------------------------------------------------------

    def delete_entry(self, song_id: str) -> bool:
        if not self.is_ready:
            return False
        sid = str(song_id).strip()
        if not sid:
            return False
        try:
            with sqlite3.connect(self.db_path) as conn:
                cur = conn.execute("DELETE FROM images WHERE image_id = ?", (sid,))
                conn.commit()
            if int(cur.rowcount) == 0:
                return False
        except Exception as e:
            print(f"[ImageDB] 항목 삭제 실패: {e}")
            return False

        with self._cache_lock:
            before = len(self._cache)
            self._cache = [e for e in self._cache if e.image_id != sid]
            self.song_count = len(self._cache)
            deleted = len(self._cache) < before
            if deleted:
                self._rebuild_vectors()

        return deleted

    # ------------------------------------------------------------------
    # 조회 헬퍼 (CLI / 통계용 — DB 직접 읽기)
    # ------------------------------------------------------------------

    def get_stats(self) -> Optional[dict[str, int]]:
        if not self.is_ready:
            return None
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute(
                    "SELECT COUNT(*), COUNT(DISTINCT image_id) FROM images"
                ).fetchone()
            return {"total_rows": int(row[0]), "distinct_song_ids": int(row[1])}
        except Exception as e:
            print(f"[ImageDB] 통계 조회 실패: {e}")
            return None

    def list_entries(self, limit: int = 100, offset: int = 0) -> list[dict]:
        if not self.is_ready:
            return []
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(
                    "SELECT id, image_id FROM images ORDER BY id ASC LIMIT ? OFFSET ?",
                    (max(1, int(limit)), max(0, int(offset))),
                ).fetchall()
            return [{"id": int(r[0]), "image_id": str(r[1])} for r in rows]
        except Exception as e:
            print(f"[ImageDB] 목록 조회 실패: {e}")
            return []

    def get_entry(self, song_id: str) -> Optional[dict]:
        if not self.is_ready:
            return None
        sid = str(song_id).strip()
        if not sid:
            return None
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute(
                    "SELECT id, image_id, phash, dhash, ahash, hog FROM images WHERE image_id = ?",
                    (sid,),
                ).fetchone()
            if not row:
                return None
            return {
                "id": int(row[0]),
                "image_id": str(row[1]),
                "phash": str(row[2]),
                "dhash": str(row[3]),
                "ahash": str(row[4]),
                "hog_size": len(row[5]) if row[5] is not None else 0,
            }
        except Exception as e:
            print(f"[ImageDB] 단건 조회 실패: {e}")
            return None


# ------------------------------------------------------------------
# 모듈 레벨 순수 함수
# ------------------------------------------------------------------

def _make_entry(
    image_id: str,
    ph: str, dh: str, ah: str,
    hog: np.ndarray,
) -> _CachedEntry:
    return _CachedEntry(
        image_id=image_id,
        phash_int=int(ph, 16),
        dhash_int=int(dh, 16),
        ahash_int=int(ah, 16),
        hog=hog,
    )


def _row_to_entry(row) -> _CachedEntry:
    image_id, ph, dh, ah, hog_blob = row
    hog = np.frombuffer(hog_blob, dtype=np.float32).copy()
    return _make_entry(str(image_id), ph, dh, ah, hog)


def _compute_features(img: np.ndarray) -> Optional[tuple[str, str, str, np.ndarray]]:
    prepared = _prepare_image(img)
    if prepared is None:
        return None
    data, width, height, channels = prepared
    ph, dh, ah, hog = image_features(data, width, height, channels)
    return ph, dh, ah, np.array(hog, dtype=np.float32)


def _prepare_image(img: np.ndarray) -> Optional[tuple[bytes, int, int, int]]:
    if img is None or img.size == 0:
        return None
    if img.ndim == 2:
        height, width = img.shape
        return _image_bytes(img), width, height, 1
    if img.ndim == 3 and img.shape[2] in (3, 4):
        height, width, channels = img.shape
        return _image_bytes(img), width, height, channels
    return None


def _image_bytes(img: np.ndarray) -> bytes:
    return np.ascontiguousarray(img, dtype=np.uint8).tobytes()


def _compute_hashes(gray: np.ndarray) -> tuple[str, str, str]:
    if gray is None or gray.ndim != 2:
        return "0" * 16, "0" * 16, "0" * 16
    height, width = gray.shape
    return hashes_gray(_image_bytes(gray), width, height)


def _compute_hog(gray: np.ndarray) -> np.ndarray:
    if gray is None or gray.ndim != 2:
        return np.zeros((1764,), dtype=np.float32)
    height, width = gray.shape
    return np.array(hog_gray(_image_bytes(gray), width, height), dtype=np.float32)

