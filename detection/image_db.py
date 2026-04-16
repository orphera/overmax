from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Optional

import cv2
import numpy as np


class ImageDB:
    """Perceptual-hash + HOG + ORB 기반 경량 이미지 매칭 DB."""

    def __init__(self, db_path: str = "cache/image_index.db", similarity_threshold: float = 0.7):
        self.db_path = Path(db_path)
        self.similarity_threshold = float(similarity_threshold)
        self.is_ready = False
        self.song_count = 0

    def initialize(self) -> bool:
        try:
            self.db_path.parent.mkdir(parents=True, exist_ok=True)
            with sqlite3.connect(self.db_path) as conn:
                conn.execute(
                    """
                    CREATE TABLE IF NOT EXISTS images (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        image_id TEXT NOT NULL,
                        phash TEXT NOT NULL,
                        dhash TEXT NOT NULL,
                        ahash TEXT NOT NULL,
                        hog BLOB NOT NULL,
                        orb BLOB
                    )
                    """
                )
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_images_image_id ON images (image_id)"
                )
                # image_id(song_id) 중복을 제거한 뒤 unique 인덱스를 보장한다.
                conn.execute(
                    """
                    DELETE FROM images
                    WHERE id NOT IN (
                        SELECT MAX(id)
                        FROM images
                        GROUP BY image_id
                    )
                    """
                )
                conn.execute(
                    "CREATE UNIQUE INDEX IF NOT EXISTS uq_images_image_id ON images (image_id)"
                )
                conn.commit()
            self.is_ready = True
            return True
        except Exception as e:
            print(f"[ImageDB] 초기화 실패: {e}")
            self.is_ready = False
            return False

    def load(self) -> int:
        if not self.is_ready:
            return 0
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute(
                    "SELECT COUNT(DISTINCT image_id) FROM images"
                ).fetchone()
            self.song_count = int(row[0] if row else 0)
            print(f"[ImageDB] 로드 완료: {self.song_count}곡")
            return self.song_count
        except Exception as e:
            print(f"[ImageDB] 로드 실패: {e}")
            return 0

    def get_stats(self) -> Optional[dict[str, int]]:
        if not self.is_ready:
            return None
        try:
            with sqlite3.connect(self.db_path) as conn:
                row = conn.execute(
                    """
                    SELECT
                        COUNT(*) AS total_rows,
                        COUNT(DISTINCT image_id) AS distinct_song_ids
                    FROM images
                    """
                ).fetchone()
            if not row:
                return {"total_rows": 0, "distinct_song_ids": 0}
            return {
                "total_rows": int(row[0]),
                "distinct_song_ids": int(row[1]),
            }
        except Exception as e:
            print(f"[ImageDB] 통계 조회 실패: {e}")
            return None

    def list_entries(self, limit: int = 100, offset: int = 0) -> list[dict]:
        if not self.is_ready:
            return []
        safe_limit = max(1, int(limit))
        safe_offset = max(0, int(offset))
        try:
            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(
                    """
                    SELECT id, image_id, orb
                    FROM images
                    ORDER BY id ASC
                    LIMIT ? OFFSET ?
                    """,
                    (safe_limit, safe_offset),
                ).fetchall()
            return [
                {
                    "id": int(row[0]),
                    "image_id": str(row[1]),
                    "has_orb": row[2] is not None,
                }
                for row in rows
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
                    """
                    SELECT id, image_id, phash, dhash, ahash, hog, orb
                    FROM images
                    WHERE image_id = ?
                    """,
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

    def delete_entry(self, song_id: str) -> bool:
        if not self.is_ready:
            return False
        sid = str(song_id).strip()
        if not sid:
            return False
        try:
            with sqlite3.connect(self.db_path) as conn:
                cur = conn.execute(
                    "DELETE FROM images WHERE image_id = ?",
                    (sid,),
                )
                conn.commit()
            deleted = int(cur.rowcount) > 0
            if deleted:
                self.load()
            return deleted
        except Exception as e:
            print(f"[ImageDB] 항목 삭제 실패: {e}")
            return False

    def register(self, song_id: str, img: np.ndarray) -> bool:
        if not self.is_ready:
            print("[ImageDB] 미초기화 상태 - register 불가")
            return False
        sid = str(song_id).strip()
        if not sid:
            print("[ImageDB] song_id 비어있음")
            return False

        gray = self._to_gray(img)
        if gray is None:
            print("[ImageDB] 이미지 변환 실패")
            return False

        try:
            ph, dh, ah = self._compute_hashes(gray)
            hog_vec = self._compute_hog(gray)
            orb_desc = self._compute_orb(gray)
            orb_blob = orb_desc.tobytes() if orb_desc is not None else None

            with sqlite3.connect(self.db_path) as conn:
                conn.execute(
                    """
                    INSERT INTO images (image_id, phash, dhash, ahash, hog, orb)
                    VALUES (?, ?, ?, ?, ?, ?)
                    ON CONFLICT(image_id) DO UPDATE SET
                        phash = excluded.phash,
                        dhash = excluded.dhash,
                        ahash = excluded.ahash,
                        hog = excluded.hog,
                        orb = excluded.orb
                    """,
                    (sid, ph, dh, ah, hog_vec.tobytes(), orb_blob),
                )
                conn.commit()

            self.load()
            print(f"[ImageDB] 등록/갱신 완료: '{sid}'")
            return True
        except Exception as e:
            print(f"[ImageDB] 등록 실패: {e}")
            return False

    def search(self, img: np.ndarray, top_k: int = 10) -> Optional[tuple[str, float]]:
        if not self.is_ready or self.song_count <= 0:
            return None

        gray = self._to_gray(img)
        if gray is None:
            return None

        try:
            q_ph, q_dh, q_ah = self._compute_hashes(gray)
            q_hog = self._compute_hog(gray)
            q_orb = self._compute_orb(gray)

            with sqlite3.connect(self.db_path) as conn:
                rows = conn.execute(
                    "SELECT image_id, phash, dhash, ahash, hog, orb FROM images"
                ).fetchall()

            if not rows:
                return None

            candidates = []
            for image_id, ph, dh, ah, hog_blob, orb_blob in rows:
                hash_score = (
                    0.5 * self._hash_distance(q_ph, ph)
                    + 0.3 * self._hash_distance(q_dh, dh)
                    + 0.2 * self._hash_distance(q_ah, ah)
                )
                candidates.append((hash_score, image_id, hog_blob, orb_blob))

            candidates.sort(key=lambda x: x[0])
            candidates = candidates[: max(1, top_k)]

            refined = []
            for hash_score, image_id, hog_blob, orb_blob in candidates:
                db_hog = np.frombuffer(hog_blob, dtype=np.float32)
                h_score = float(np.linalg.norm(q_hog - db_hog))
                refined.append((h_score, hash_score, image_id, orb_blob))

            refined.sort(key=lambda x: x[0])
            refined = refined[:3]

            best: Optional[tuple[str, float]] = None
            for h_score, hash_score, image_id, orb_blob in refined:
                db_orb = self._decode_orb(orb_blob)
                orb_matches = self._orb_match_score(q_orb, db_orb)

                hog_sim = max(0.0, 1.0 - min(h_score / 30.0, 1.0))
                orb_sim = min(orb_matches / 20.0, 1.0)
                hash_sim = max(0.0, 1.0 - min(hash_score / 64.0, 1.0))
                similarity = (0.45 * hash_sim) + (0.35 * hog_sim) + (0.20 * orb_sim)

                if best is None or similarity > best[1]:
                    best = (str(image_id), float(similarity))

            if best and best[1] >= self.similarity_threshold:
                return best
            return None
        except Exception as e:
            print(f"[ImageDB] 검색 실패: {e}")
            return None

    @staticmethod
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

    @staticmethod
    def _compute_hashes(gray: np.ndarray) -> tuple[str, str, str]:
        ph = ImageDB._phash(gray)
        dh = ImageDB._dhash(gray)
        ah = ImageDB._ahash(gray)
        return ph, dh, ah

    @staticmethod
    def _bits_to_hex(bits: np.ndarray) -> str:
        flat = bits.reshape(-1).astype(np.uint8)
        packed = np.packbits(flat, bitorder="big")
        return "".join(f"{b:02x}" for b in packed)

    @staticmethod
    def _ahash(gray: np.ndarray) -> str:
        resized = cv2.resize(gray, (8, 8), interpolation=cv2.INTER_AREA).astype(np.float32)
        avg = float(np.mean(resized))
        bits = resized > avg
        return ImageDB._bits_to_hex(bits)

    @staticmethod
    def _dhash(gray: np.ndarray) -> str:
        resized = cv2.resize(gray, (9, 8), interpolation=cv2.INTER_AREA).astype(np.float32)
        bits = resized[:, 1:] > resized[:, :-1]
        return ImageDB._bits_to_hex(bits)

    @staticmethod
    def _phash(gray: np.ndarray) -> str:
        resized = cv2.resize(gray, (32, 32), interpolation=cv2.INTER_AREA).astype(np.float32)
        dct = cv2.dct(resized)
        low = dct[:8, :8]
        median = float(np.median(low.reshape(-1)[1:]))  # DC 성분 제외
        bits = low > median
        return ImageDB._bits_to_hex(bits)

    @staticmethod
    def _compute_hog(gray: np.ndarray) -> np.ndarray:
        resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
        descriptor = cv2.HOGDescriptor(
            _winSize=(64, 64),
            _blockSize=(16, 16),
            _blockStride=(8, 8),
            _cellSize=(8, 8),
            _nbins=9,
        )
        features = descriptor.compute(resized)
        if features is None:
            # 64x64, 8x8 cell, 16x16 block, 8 stride, 9 bins -> 1764 dims
            return np.zeros((1764,), dtype=np.float32)
        return features.reshape(-1).astype(np.float32)

    @staticmethod
    def _compute_orb(gray: np.ndarray) -> Optional[np.ndarray]:
        orb = cv2.ORB_create(nfeatures=50)
        _, des = orb.detectAndCompute(gray, None)
        if des is None:
            return None
        return des.astype(np.uint8)

    @staticmethod
    def _hash_distance(h1: str, h2: str) -> int:
        return bin(int(h1, 16) ^ int(h2, 16)).count("1")

    @staticmethod
    def _decode_orb(orb_blob: Optional[bytes]) -> Optional[np.ndarray]:
        if not orb_blob:
            return None
        flat = np.frombuffer(orb_blob, dtype=np.uint8)
        if flat.size == 0 or (flat.size % 32) != 0:
            return None
        return flat.reshape(-1, 32)

    @staticmethod
    def _orb_match_score(des1: Optional[np.ndarray], des2: Optional[np.ndarray]) -> int:
        if des1 is None or des2 is None:
            return 0
        bf = cv2.BFMatcher(cv2.NORM_HAMMING)
        matches = bf.knnMatch(des1, des2, k=2)
        good = 0
        for pair in matches:
            if len(pair) < 2:
                continue
            m, n = pair
            if m.distance < 0.75 * n.distance:
                good += 1
        return good


if __name__ == "__main__":
    from detection.image_db_cli import run_cli

    run_cli()
