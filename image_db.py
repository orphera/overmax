from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Optional

import cv2
import imagehash
import numpy as np
from PIL import Image
from skimage.feature import hog


class ImageDB:
    """Perceptual-hash + HOG + ORB 기반 경량 이미지 매칭 DB."""

    def __init__(self, db_path: str = "image_index.db", similarity_threshold: float = 0.5):
        self.db_path = Path(db_path)
        self.similarity_threshold = float(similarity_threshold)
        self.is_ready = False
        self.song_count = 0

    def initialize(self) -> bool:
        try:
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
                    """,
                    (sid, ph, dh, ah, hog_vec.tobytes(), orb_blob),
                )
                conn.commit()

            self.load()
            print(f"[ImageDB] 등록 완료: '{sid}'")
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
        pil_img = Image.fromarray(gray)
        ph = str(imagehash.phash(pil_img))
        dh = str(imagehash.dhash(pil_img))
        ah = str(imagehash.average_hash(pil_img))
        return ph, dh, ah

    @staticmethod
    def _compute_hog(gray: np.ndarray) -> np.ndarray:
        resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
        features = hog(
            resized,
            pixels_per_cell=(8, 8),
            cells_per_block=(2, 2),
            feature_vector=True,
        )
        return features.astype(np.float32)

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
