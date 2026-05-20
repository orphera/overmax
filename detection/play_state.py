"""
play_state.py - 게임 플레이 상태(모드, 난이도, 맥스콤보, 레이트) 통합 감지 모듈
"""

from __future__ import annotations
import math
from collections import deque
from typing import Optional, TYPE_CHECKING

import numpy as np

from constants import (
    BTN_COLORS,
    BTN_MODE_MAX_DIST,
    DIFF_MIN_BRIGHTNESS,
    DIFF_CONFIDENT_MARGIN,
)
from core.game_state import GameSessionState

if TYPE_CHECKING:
    from capture.roi_manager import ROIManager
    from detection.ocr import OcrDetector


# ------------------------------------------------------------------
# 내부 유틸 및 원시 감지 함수
# ------------------------------------------------------------------

def _color_dist(c1: tuple[int, int, int], c2: tuple[int, int, int]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(c1, c2)))


def _region_mean_bgr(
    frame: np.ndarray,
    roi: tuple[int, int, int, int],
) -> tuple[int, int, int]:
    h, w = frame.shape[:2]
    x1, y1, x2, y2 = roi
    x1, y1, x2, y2 = max(0, x1), max(0, y1), min(w, x2), min(h, y2)
    if x2 <= x1 or y2 <= y1:
        return (0, 0, 0)
    roi_img = frame[y1:y2, x1:x2]
    mean = roi_img.mean(axis=(0, 1))
    return int(mean[0]), int(mean[1]), int(mean[2])


def detect_button_mode(frame: np.ndarray, roiman: ROIManager) -> Optional[str]:
    roi = roiman.get_roi("btn_mode")
    mean_bgr = _region_mean_bgr(frame, roi)
    best_mode, best_dist = None, float("inf")
    for mode, colors in BTN_COLORS.items():
        for ref_bgr in colors:
            dist = _color_dist(mean_bgr, ref_bgr)
            if dist < best_dist:
                best_dist, best_mode = dist, mode
    return best_mode if best_dist <= BTN_MODE_MAX_DIST else None


def detect_difficulty(frame: np.ndarray, roiman: ROIManager) -> tuple[Optional[str], bool]:
    brightnesses = {
        diff: sum(_region_mean_bgr(frame, roiman.get_diff_panel_roi(diff))) / 3.0
        for diff in ["NM", "HD", "MX", "SC"]
    }
    sorted_diffs = sorted(brightnesses, key=lambda d: brightnesses[d], reverse=True)
    best_diff = sorted_diffs[0]
    max_bright = brightnesses[best_diff]
    if max_bright < DIFF_MIN_BRIGHTNESS:
        return None, False
    second_bright = brightnesses[sorted_diffs[1]] if len(sorted_diffs) > 1 else 0.0
    is_confident = (max_bright - second_bright) >= DIFF_CONFIDENT_MARGIN
    return best_diff, is_confident


def detect_max_combo(frame: np.ndarray, roiman: ROIManager) -> bool:
    roi = roiman.get_roi("max_combo_badge")
    b, g, r = _region_mean_bgr(frame, roi)
    return (r + g + b) / 3.0 >= 160


class PlayStateDetector:
    """
    모드, 난이도, 맥스 콤보, 그리고 달성률(Rate)을 통합적으로 감지합니다.
    히스테리시스를 통해 안정화된 상태를 판정하며, 상태가 확정되는 시점에 1회 Rate OCR을 수행합니다.
    """
    def __init__(self, ocr_detector: OcrDetector, history_size: int = 3):
        self.ocr_detector = ocr_detector
        self.history_size = max(1, history_size)
        
        self._history = deque(maxlen=self.history_size)
        self._last_stable_state: Optional[GameSessionState] = None
        self._ocr_done_for: Optional[tuple] = None # (song_id, mode, diff)

    def reset(self):
        """내부 히스토리 및 상태를 초기화합니다."""
        self._history.clear()
        self._last_stable_state = None
        self._ocr_done_for = None

    async def detect(
        self, 
        frame: np.ndarray, 
        roiman: ROIManager, 
        song_id: Optional[int],
        screen_mode: Optional[str] = None,
    ) -> GameSessionState:
        """
        현재 프레임을 분석하여 안정화된 게임 상태를 반환합니다.
        새로운 상태가 안정화되면 내부적으로 Rate OCR을 수행합니다.
        """
        raw_mode = detect_button_mode(frame, roiman)
        raw_diff, raw_confident = detect_difficulty(frame, roiman)
        raw_max_combo = detect_max_combo(frame, roiman)
        
        # 유효한 데이터인 경우만 히스토리에 추가
        current_raw = (song_id, raw_mode, raw_diff, raw_max_combo)
        is_valid_raw = all(v is not None for v in [song_id, raw_mode, raw_diff]) and raw_confident
        
        self._history.append(current_raw if is_valid_raw else None)
        
        # 안정화 여부 확인
        is_stable = False
        stable_res = None
        if len(self._history) == self.history_size and len(set(self._history)) == 1:
            if self._history[0] is not None:
                is_stable = True
                stable_res = self._history[0]
        
        if not is_stable:
            return GameSessionState(
                song_id=song_id,
                mode=raw_mode,
                diff=raw_diff,
                is_stable=False,
                is_max_combo=raw_max_combo,
                rate=None
            )

        # 안정화된 상태 처리
        s_sid, s_mode, s_diff, s_mc = stable_res
        
        # 새로운 상태인 경우 Rate OCR 시도
        current_state_key = (s_sid, s_mode, s_diff)
        rate = None
        
        if self._ocr_done_for == current_state_key:
            # 이미 이 상태에서 OCR을 했으면 기존 rate 유지 (있다면)
            if self._last_stable_state and self._last_stable_state.is_stable:
                rate = self._last_stable_state.rate
        else:
            # 새로운 상태 진입 -> OCR 수행
            rate_roi_name = "online_rate" if screen_mode == "ONLINE" else "rate"
            rate_roi = roiman.get_roi(rate_roi_name)
            # ROI 영역 크롭 (helpers 의 기능을 직접 사용하거나 roiman 확장 고려)
            h, w = frame.shape[:2]
            x1, y1, x2, y2 = rate_roi
            rate_img = frame[max(0, y1):min(h, y2), max(0, x1):min(w, x2)]
            
            rate, _ = await self.ocr_detector.detect_rate(rate_img)
            self._ocr_done_for = current_state_key

        new_state = GameSessionState(
            song_id=s_sid,
            mode=s_mode,
            diff=s_diff,
            is_stable=True,
            is_max_combo=s_mc,
            rate=rate
        )
        self._last_stable_state = new_state
        return new_state
