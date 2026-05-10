"""Helper functions for screen_capture runtime pipeline."""

from typing import Optional

import numpy as np
from overmax_cv import thumbnail_bgra_32


def crop_roi(frame: np.ndarray, roi: tuple[int, int, int, int]) -> np.ndarray:
    """ROI (x1, y1, x2, y2) 영역을 프레임에서 잘라낸다."""
    h, w = frame.shape[:2]
    x1, y1, x2, y2 = roi
    return frame[max(0, y1):min(h, y2), max(0, x1):min(w, x2)]


def make_thumbnail(image_bgra: np.ndarray) -> np.ndarray:
    h, w = image_bgra.shape[:2]
    data = np.ascontiguousarray(image_bgra, dtype=np.uint8).tobytes()
    thumb = thumbnail_bgra_32(data, w, h)
    return np.frombuffer(thumb, dtype=np.uint8).reshape((32, 32))


def has_thumbnail_changed(current: np.ndarray, prev: Optional[np.ndarray], threshold: float) -> bool:
    if prev is None:
        return True
    diff = np.abs(current.astype(np.float32) - prev.astype(np.float32))
    return float(np.mean(diff)) >= threshold
