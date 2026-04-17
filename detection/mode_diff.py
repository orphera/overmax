"""
mode_diff_detector.py - 버튼 모드 및 선택된 난이도 감지

버튼 모드: BTN_MODE_ROI 영역 평균색 vs 대표색 거리 비교 (4B/5B/6B/8B)
난이도:    DIFF_PANEL_ROI 영역 평균 밝기 비교 (NM/HD/MX/SC)
"""

from __future__ import annotations

import math
from typing import Optional

import numpy as np

from constants import (
    BTN_COLORS,
    BTN_MODE_MAX_DIST,
    DIFF_MIN_BRIGHTNESS,
    DIFF_CONFIDENT_MARGIN,
)
from capture.roi_manager import ROIManager


# ------------------------------------------------------------------
# 내부 유틸
# ------------------------------------------------------------------

def _color_dist(c1: tuple[int, int, int], c2: tuple[int, int, int]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(c1, c2)))


def _region_mean_bgr(
    frame: np.ndarray,
    roi: tuple[int, int, int, int],
) -> tuple[int, int, int]:
    h, w = frame.shape[:2]
    x1, y1, x2, y2 = roi
    
    # 영역 클리핑
    x1 = max(0, min(w, x1))
    y1 = max(0, min(h, y1))
    x2 = max(0, min(w, x2))
    y2 = max(0, min(h, y2))
    
    if x2 <= x1 or y2 <= y1:
        return (0, 0, 0)
    roi_img = frame[y1:y2, x1:x2]
    mean = roi_img.mean(axis=(0, 1))
    return int(mean[0]), int(mean[1]), int(mean[2])


# ------------------------------------------------------------------
# 퍼블릭 API
# ------------------------------------------------------------------

def detect_button_mode(frame: np.ndarray, roiman: ROIManager) -> Optional[str]:
    """버튼 모드 감지 (4B/5B/6B/8B). 실패 시 None."""
    roi = roiman.get_roi("btn_mode")
    mean_bgr = _region_mean_bgr(frame, roi)

    best_mode: Optional[str] = None
    best_dist = float("inf")
    for mode, colors in BTN_COLORS.items():
        for ref_bgr in colors:
            dist = _color_dist(mean_bgr, ref_bgr)
            if dist < best_dist:
                best_dist = dist
                best_mode = mode

    return best_mode if best_dist <= BTN_MODE_MAX_DIST else None


def detect_difficulty(frame: np.ndarray, roiman: ROIManager) -> tuple[Optional[str], bool]:
    """
    현재 선택된 난이도 감지 (NM/HD/MX/SC).
    반환: (diff, is_confident)
    """
    brightnesses = {
        diff: sum(_region_mean_bgr(frame, roiman.get_diff_panel_roi(diff))) / 3.0
        for diff in ["NM", "HD", "MX", "SC"]
    }

    sorted_diffs = sorted(brightnesses, key=lambda d: brightnesses[d], reverse=True)
    best_diff    = sorted_diffs[0]
    max_bright   = brightnesses[best_diff]

    if max_bright < DIFF_MIN_BRIGHTNESS:
        return None, False

    second_bright = brightnesses[sorted_diffs[1]] if len(sorted_diffs) > 1 else 0.0
    is_confident  = (max_bright - second_bright) >= DIFF_CONFIDENT_MARGIN
    return best_diff, is_confident


def detect_mode_and_difficulty(frame: np.ndarray, roiman: ROIManager) -> tuple[Optional[str], Optional[str], bool]:
    """버튼 모드와 선택된 난이도를 동시에 감지. 반환: (mode, diff, is_confident)"""
    mode = detect_button_mode(frame, roiman)
    diff, is_confident = detect_difficulty(frame, roiman)
    return mode, diff, is_confident