"""Helper functions for screen_capture runtime pipeline."""

import difflib
import re
from typing import Optional

import cv2
import numpy as np


def crop_ratio_region(frame: np.ndarray, x_start: float, x_end: float, y_start: float, y_end: float) -> np.ndarray:
    h, w = frame.shape[:2]
    return frame[int(h * y_start):int(h * y_end), int(w * x_start):int(w * x_end)]


def make_thumbnail(image_bgra: np.ndarray) -> np.ndarray:
    gray = cv2.cvtColor(image_bgra, cv2.COLOR_BGRA2GRAY)
    return cv2.resize(gray, (32, 32), interpolation=cv2.INTER_AREA)


def has_thumbnail_changed(current: np.ndarray, prev: Optional[np.ndarray], threshold: float) -> bool:
    if prev is None:
        return True
    diff = np.abs(current.astype(np.float32) - prev.astype(np.float32))
    return float(np.mean(diff)) >= threshold


def parse_rate_text(text: str) -> Optional[float]:
    if not text:
        return None

    cleaned = re.sub(r"[^0-9.]", "", text)
    try:
        if cleaned.count(".") > 1:
            parts = cleaned.split(".")
            cleaned = parts[0] + "." + "".join(parts[1:])

        value = float(cleaned)
        if 0.0 <= value <= 100.0:
            return value
    except ValueError:
        return None
    return None


def build_ratio_region(rect, x_start, x_end, y_start, y_end) -> dict:
    return {
        "top": rect.top + int(rect.height * y_start),
        "left": rect.left + int(rect.width * x_start),
        "width": max(1, int(rect.width * (x_end - x_start))),
        "height": max(1, int(rect.height * (y_end - y_start))),
    }


def normalize_alnum(text: str) -> str:
    return re.sub(r"[^A-Z0-9]", "", text.upper())


def is_logo_keyword_match(keyword: str, normalized_ocr: str) -> bool:
    if not keyword or not normalized_ocr:
        return False
    if keyword in normalized_ocr:
        return True

    min_partial_len = min(6, len(keyword))
    for i in range(0, len(keyword) - min_partial_len + 1):
        part = keyword[i:i + min_partial_len]
        if part and part in normalized_ocr:
            return True

    ratio = difflib.SequenceMatcher(None, keyword, normalized_ocr).ratio()
    return ratio >= 0.72


def make_rate_roi(frame: np.ndarray, x1: int, y1: int, x2: int, y2: int) -> np.ndarray:
    h, w = frame.shape[:2]
    sx, sy = w / 1920.0, h / 1080.0
    rx1 = int(x1 * sx)
    ry1 = int(y1 * sy)
    rx2 = int(x2 * sx)
    ry2 = int(y2 * sy)
    return frame[ry1:ry2, rx1:rx2]


def preprocess_for_ocr(img_bgra: np.ndarray, force_invert: bool = False) -> Optional[np.ndarray]:
    h, w = img_bgra.shape[:2]
    if w == 0 or h == 0:
        return None

    upscaled = cv2.resize(img_bgra, (w * 3, h * 3), interpolation=cv2.INTER_CUBIC)
    gray = cv2.cvtColor(upscaled, cv2.COLOR_BGRA2GRAY)

    bg_mean = float(gray.mean())
    normal_is_dark = bg_mean < 128
    use_invert = normal_is_dark if force_invert else not normal_is_dark

    if use_invert:
        _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY_INV | cv2.THRESH_OTSU)
    else:
        _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY | cv2.THRESH_OTSU)

    return cv2.copyMakeBorder(thresh, 10, 10, 10, 10, cv2.BORDER_CONSTANT, value=0)
