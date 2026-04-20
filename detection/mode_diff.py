"""
mode_diff_detector.py - 버튼 모드 및 선택된 난이도 감지

버튼 모드: BTN_MODE_ROI 영역 평균색 vs 대표색 거리 비교 (4B/5B/6B/8B)
난이도:    DIFF_PANEL_ROI 영역 평균 밝기 비교 (NM/HD/MX/SC)
"""

from __future__ import annotations

import math
from collections import deque
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
    """[DEPRECATED] 단일 프레임 기반 버튼 모드와 선택된 난이도 동시 감지. ModeDiffDetector 사용 권장."""
    mode = detect_button_mode(frame, roiman)
    diff, is_confident = detect_difficulty(frame, roiman)
    return mode, diff, is_confident


class ModeDiffDetector:
    """
    모드 및 난이도 인식 결과에 히스테리시스(Debounce)를 적용하여 
    단일 프레임이나 애니메이션 중 발생하는 오인식(깜빡임)을 방지합니다.
    """
    def __init__(self, history_size: int = 3):
        self.history_size = max(1, history_size)
        self._mode_history = deque(maxlen=self.history_size)
        self._diff_history = deque(maxlen=self.history_size)
        self._stable_mode: Optional[str] = None
        self._stable_diff: Optional[str] = None

    def reset(self):
        """내부 히스토리 및 안정화된 상태를 초기화합니다."""
        self._mode_history.clear()
        self._diff_history.clear()
        self._stable_mode = None
        self._stable_diff = None

    def detect(self, frame: np.ndarray, roiman: ROIManager) -> tuple[Optional[str], Optional[str], bool]:
        """
        현재 프레임을 분석하여 안정화된(stable) 모드와 난이도를 반환합니다.
        UI 애니메이션 중이거나 순간적으로 인식이 튀는 경우, 이전의 안정된 값을 유지합니다.
        
        반환: (stable_mode, stable_diff, raw_confident)
        """
        raw_mode = detect_button_mode(frame, roiman)
        raw_diff, raw_confident = detect_difficulty(frame, roiman)

        self._mode_history.append(raw_mode)
        self._diff_history.append(raw_diff if raw_confident else None)

        if len(self._mode_history) == self.history_size and len(set(self._mode_history)) == 1:
            if self._mode_history[0] is not None:
                self._stable_mode = self._mode_history[0]

        if len(self._diff_history) == self.history_size and len(set(self._diff_history)) == 1:
            if self._diff_history[0] is not None:
                self._stable_diff = self._diff_history[0]

        return self._stable_mode, self._stable_diff, raw_confident