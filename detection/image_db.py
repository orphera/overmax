from __future__ import annotations

import sqlite3
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import cv2
import numpy as np


@dataclass
class _CachedEntry:
    image_id: str
    phash: str
    dhash: str
    ahash: str
    hog: np.ndarray           # float32, deserialized
    orb: Optional[np.ndarray] # uint8 (N×32) | None


class ImageDB:
    """Perceptual-hash + HOG + ORB 기반 경량 이미지 매칭 DB.

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
                    "SELECT image_id, phash, dhash, ahash, hog, orb FROM images"
                ).fetchall()
        except Exception as e:
            print(f"[ImageDB] 캐시 로드 실패: {e}")
            return 0

        entries = [_row_to_entry(r) for r in rows]
        with self._cache_lock:
            self._cache = entries
            self.song_count = len(entries)
        return self.song_count

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
        gray = _to_gray(img)
        if gray is None:
            return None

        with self._cache_lock:
            if not self._cache:
                return None
            cache = list(self._cache)

        q_ph, q_dh, q_ah = _compute_hashes(gray)
        q_hog = _compute_hog(gray)

        # 1단계: hash 거리 → top_k 후보
        candidates = sorted(
            cache,
            key=lambda e: (
                0.5 * _hash_distance(q_ph, e.phash)
                + 0.3 * _hash_distance(q_dh, e.dhash)
                + 0.2 * _hash_distance(q_ah, e.ahash)
            ),
        )[:max(1, top_k)]

        # 2단계: HOG 코사인 유사도 → best
        best: Optional[tuple[str, float]] = None
        for entry in candidates:
            hash_score = (
                0.5 * _hash_distance(q_ph, entry.phash)
                + 0.3 * _hash_distance(q_dh, entry.dhash)
                + 0.2 * _hash_distance(q_ah, entry.ahash)
            )
            hash_sim = max(0.0, 1.0 - min(hash_score / 64.0, 1.0))
            hog_sim  = _cosine_sim(q_hog, entry.hog)
            similarity = 0.45 * hash_sim + 0.55 * hog_sim

            if best is None or similarity > best[1]:
                best = (entry.image_id, float(similarity))

        if best and best[1] >= self.similarity_threshold:
            return best
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

        gray = _to_gray(img)
        if gray is None:
            print("[ImageDB] 이미지 변환 실패")
            return False

        ph, dh, ah = _compute_hashes(gray)
        hog = _compute_hog(gray)
        orb_blob = None

        try:
            with sqlite3.connect(self.db_path) as conn:
                conn.execute(
                    """
                    INSERT INTO images (image_id, phash, dhash, ahash, hog, orb)
                    VALUES (?, ?, ?, ?, ?, ?)
                    ON CONFLICT(image_id) DO UPDATE SET
                        phash = excluded.phash,
                        dhash = excluded.dhash,
                        ahash = excluded.ahash,
                        hog   = excluded.hog,
                        orb   = excluded.orb
                    """,
                    (sid, ph, dh, ah, hog.tobytes(), orb_blob),
                )
                conn.commit()
        except Exception as e:
            print(f"[ImageDB] 등록 실패: {e}")
            return False

        entry = _CachedEntry(
            image_id=sid, phash=ph, dhash=dh, ahash=ah, hog=hog, orb=orb
        )
        with self._cache_lock:
            idx = next((i for i, e in enumerate(self._cache) if e.image_id == sid), None)
            if idx is not None:
                self._cache[idx] = entry
            else:
                self._cache.append(entry)
            self.song_count = len(self._cache)

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
                    "SELECT id, image_id, orb FROM images ORDER BY id ASC LIMIT ? OFFSET ?",
                    (max(1, int(limit)), max(0, int(offset))),
                ).fetchall()
            return [
                {"id": int(r[0]), "image_id": str(r[1]), "has_orb": r[2] is not None}
                for r in rows
            ]
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
                    "SELECT id, image_id, phash, dhash, ahash, hog, orb FROM images WHERE image_id = ?",
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
                "has_orb": row[6] is not None,
                "orb_size": len(row[6]) if row[6] is not None else 0,
            }
        except Exception as e:
            print(f"[ImageDB] 단건 조회 실패: {e}")
            return None


# ------------------------------------------------------------------
# 모듈 레벨 순수 함수 (클래스 외부 — 상태 없음)
# ------------------------------------------------------------------

def _row_to_entry(row) -> _CachedEntry:
    image_id, ph, dh, ah, hog_blob, orb_blob = row
    hog = np.frombuffer(hog_blob, dtype=np.float32).copy()
    orb: Optional[np.ndarray] = None
    if False:  # if orb_blob: ORB 추가 시 활성화
        flat = np.frombuffer(orb_blob, dtype=np.uint8)
        if flat.size > 0 and flat.size % 32 == 0:
            orb = flat.reshape(-1, 32).copy()
    return _CachedEntry(
        image_id=str(image_id), phash=ph, dhash=dh, ahash=ah, hog=hog, orb=orb
    )


def _to_gray(img: np.ndarray) -> Optional[np.ndarray]:
    if img is None or img.size == 0:
        return None
    if img.ndim == 2:
        return img
    if img.ndim == 3 and img.shape[2] == 4:
        return cv2.cvtColor(img, cv2.COLOR_BGRA2GRAY)
    if img.ndim == 3 and img.shape[2] == 3:
        return cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
    return None


def _compute_hashes(gray: np.ndarray) -> tuple[str, str, str]:
    return _phash(gray), _dhash(gray), _ahash(gray)


def _bits_to_hex(bits: np.ndarray) -> str:
    packed = np.packbits(bits.reshape(-1).astype(np.uint8), bitorder="big")
    return "".join(f"{b:02x}" for b in packed)


def _ahash(gray: np.ndarray) -> str:
    r = cv2.resize(gray, (8, 8), interpolation=cv2.INTER_AREA).astype(np.float32)
    return _bits_to_hex(r > float(np.mean(r)))


def _dhash(gray: np.ndarray) -> str:
    r = cv2.resize(gray, (9, 8), interpolation=cv2.INTER_AREA).astype(np.float32)
    return _bits_to_hex(r[:, 1:] > r[:, :-1])


def _phash(gray: np.ndarray) -> str:
    r = cv2.resize(gray, (32, 32), interpolation=cv2.INTER_AREA).astype(np.float32)
    dct = cv2.dct(r)
    low = dct[:8, :8]
    median = float(np.median(low.reshape(-1)[1:]))
    return _bits_to_hex(low > median)


def _compute_hog(gray: np.ndarray) -> np.ndarray:
    resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
    descriptor = cv2.HOGDescriptor(
        _winSize=(64, 64), _blockSize=(16, 16),
        _blockStride=(8, 8), _cellSize=(8, 8), _nbins=9,
    )
    features = descriptor.compute(resized)
    if features is None:
        return np.zeros((1764,), dtype=np.float32)
    return features.reshape(-1).astype(np.float32)


def _compute_orb(gray: np.ndarray) -> Optional[np.ndarray]:
    orb = cv2.ORB_create(nfeatures=50)
    _, des = orb.detectAndCompute(gray, None)
    return des.astype(np.uint8) if des is not None else None


def _hash_distance(h1: str, h2: str) -> int:
    return bin(int(h1, 16) ^ int(h2, 16)).count("1")


def _orb_match_score(des1: Optional[np.ndarray], des2: Optional[np.ndarray]) -> int:
    if des1 is None or des2 is None:
        return 0
    bf = cv2.BFMatcher(cv2.NORM_HAMMING)
    matches = bf.knnMatch(des1, des2, k=2)
    return sum(1 for m in matches if len(m) == 2 and m[0].distance < 0.75 * m[1].distance)


def _cosine_sim(a: np.ndarray, b: np.ndarray) -> float:
    denom = np.linalg.norm(a) * np.linalg.norm(b)
    if denom == 0.0:
        return 0.0
    return float(np.dot(a, b) / denom)


if __name__ == "__main__":
    from detection.image_db_cli import run_cli
    run_cli()