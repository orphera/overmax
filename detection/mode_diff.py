"""
mode_diff_detector.py - 버튼 모드 및 선택된 난이도 감지

1920x1080 기준 픽셀 좌표를 비율로 변환하여 사용.

버튼 모드 감지:
    좌상단 (80, 130) 기준 5x5 픽셀 영역 색상 분류
    4B: 녹색계열  #2D4F55 / #0C475A
    5B: 연파랑    #44A9C6
    6B: 노랑      #ED9430
    8B: 진파랑    #1D1431

난이도 감지 (NM 기준, HD/MX/SC는 x 오프셋):
    위치1 (97, 487), 위치2 (100, 492)
    선택 안 됨: 위치1 !≈ 위치2 (색 차이 큼)
    선택 됨:   위치1 ≈ 위치2 (색 차이 작음)

    각 난이도 x 오프셋 (1920 기준):
        NM=0, HD=120, MX=240, SC=360
"""

from __future__ import annotations

import math
from typing import Optional

import numpy as np

# ------------------------------------------------------------------
# 1920x1080 기준 원본 좌표 → 비율
# ------------------------------------------------------------------

_W = 1920.0
_H = 1080.0


def _r(x: float, y: float) -> tuple[float, float]:
    return x / _W, y / _H


# ── 버튼 모드 감지 ──────────────────────────────────────────────
# 중심 픽셀 + 반경 2px 사각형 (5x5)
_BTN_CX, _BTN_CY = _r(82.0, 132.0)   # 중심 (80~84, 130~134)
_BTN_HALF_W = 2 / _W
_BTN_HALF_H = 2 / _H

# 버튼별 대표 색 (BGR)
_BTN_COLORS: dict[str, list[tuple[int, int, int]]] = {
    "4B": [(0x55, 0x4F, 0x2D), (0x5A, 0x47, 0x0C)],   # #2D4F55 / #0C475A
    "5B": [(0xC6, 0xA9, 0x44)],                         # #44A9C6
    "6B": [(0x30, 0x94, 0xED)],                         # #ED9430
    "8B": [(0x31, 0x14, 0x1D)],                         # #1D1431
}

# ── 난이도 감지 ──────────────────────────────────────────────────
# NM 기준 패널 영역 (x1, y1, x2, y2)
_DIFF_ROI = (102.0, 492.0, 204.0, 510.0)

# 각 난이도의 x 오프셋 (픽셀)
_DIFF_X_OFFSETS: dict[str, float] = {
    "NM": 0.0,
    "HD": 120.0,
    "MX": 240.0,
    "SC": 360.0,
}

# 난이도 없음 기준색 (BGR)
_NOT_EXISTS_DIFF_COLOR   = (0x30, 0x30, 0x30)   # #303030

_COLOR_TOLERANCE   = 20   # 기준색과의 최대 유클리드 거리


# ------------------------------------------------------------------
# 내부 유틸
# ------------------------------------------------------------------

def _pixel_at(frame_bgra: np.ndarray, rx: float, ry: float) -> tuple[int, int, int]:
    """비율 좌표 → BGR 픽셀 값 반환 (BGRA 또는 BGR 모두 지원)"""
    h, w = frame_bgra.shape[:2]
    px = min(int(rx * w), w - 1)
    py = min(int(ry * h), h - 1)
    pixel = frame_bgra[py, px]
    return int(pixel[2]), int(pixel[1]), int(pixel[0])  # R,G,B → return B,G,R tuple


def _color_dist(c1: tuple[int, int, int], c2: tuple[int, int, int]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(c1, c2)))


def _region_mean_bgr(
    frame_bgra: np.ndarray,
    rx1: float, ry1: float,
    rx2: float, ry2: float,
) -> tuple[int, int, int]:
    h, w = frame_bgra.shape[:2]
    x1 = max(0, int(rx1 * w))
    y1 = max(0, int(ry1 * h))
    x2 = min(w, int(rx2 * w) + 1)
    y2 = min(h, int(ry2 * h) + 1)
    if x2 <= x1 or y2 <= y1:
        return (0, 0, 0)
    roi = frame_bgra[y1:y2, x1:x2]
    mean = roi.mean(axis=(0, 1))  # shape (3 or 4,)
    b, g, r = int(mean[0]), int(mean[1]), int(mean[2])
    return (b, g, r)


def get_difficulty_roi(diff: str) -> tuple[float, float, float, float]:
    """해당 난이도 패널의 1920x1080 기준 비율 좌표 (rx1, ry1, rx2, ry2) 반환"""
    x_offset = _DIFF_X_OFFSETS.get(diff, 0.0)
    dx = x_offset / _W
    rx1 = (_DIFF_ROI[0] + x_offset) / _W
    ry1 = _DIFF_ROI[1] / _H
    rx2 = (_DIFF_ROI[2] + x_offset) / _W
    ry2 = _DIFF_ROI[3] / _H
    return rx1, ry1, rx2, ry2


# ------------------------------------------------------------------

def detect_button_mode(frame_bgra: np.ndarray) -> Optional[str]:
    """
    버튼 모드 감지 (4B / 5B / 6B / 8B).
    감지 실패 시 None 반환.
    """
    mean_bgr = _region_mean_bgr(
        frame_bgra,
        _BTN_CX - _BTN_HALF_W, _BTN_CY - _BTN_HALF_H,
        _BTN_CX + _BTN_HALF_W, _BTN_CY + _BTN_HALF_H,
    )

    best_mode: Optional[str] = None
    best_dist = float("inf")

    for mode, colors in _BTN_COLORS.items():
        for ref_bgr in colors:
            dist = _color_dist(mean_bgr, ref_bgr)
            if dist < best_dist:
                best_dist = dist
                best_mode = mode

    # 거리가 너무 크면 인식 실패
    if best_dist > 60:
        return None
    return best_mode


def detect_difficulty(
    frame_bgra: np.ndarray,
    min_confident_margin: float = 15.0,
) -> tuple[Optional[str], bool]:
    """
    현재 선택된 난이도 감지 (NM / HD / MX / SC).
    각 패널 영역의 평균 밝기를 계산하여 가장 밝은 패널을 선택된 것으로 판정.

    반환: (diff, is_confident)
      diff          : 감지된 난이도, 실패 시 None
      is_confident  : 1위 패널이 2위보다 min_confident_margin 이상 밝으면 True
                      (OCR 검증 없이 신뢰할 수 있는 수준)
    """
    rx1 = _DIFF_ROI[0] / _W
    ry1 = _DIFF_ROI[1] / _H
    rx2 = _DIFF_ROI[2] / _W
    ry2 = _DIFF_ROI[3] / _H

    brightnesses: dict[str, float] = {}
    for diff, x_offset_px in _DIFF_X_OFFSETS.items():
        dx = x_offset_px / _W
        mean_bgr = _region_mean_bgr(frame_bgra, rx1 + dx, ry1, rx2 + dx, ry2)
        brightnesses[diff] = sum(mean_bgr) / 3.0

    sorted_diffs = sorted(brightnesses, key=lambda d: brightnesses[d], reverse=True)
    best_diff = sorted_diffs[0]
    max_brightness = brightnesses[best_diff]

    # 모든 패널이 너무 어두우면 (ex. 곡 전환 중) 인식 실패
    if max_brightness < 45:
        return None, False

    second_brightness = brightnesses[sorted_diffs[1]] if len(sorted_diffs) > 1 else 0.0
    gap = max_brightness - second_brightness
    is_confident = gap >= min_confident_margin

    return best_diff, is_confident


def detect_mode_and_difficulty(
    frame_bgra: np.ndarray,
) -> tuple[Optional[str], Optional[str], bool]:
    """
    버튼 모드와 선택된 난이도를 동시에 감지.
    반환: (button_mode, difficulty, is_confident)
      is_confident: 난이도 감지의 신뢰도 (밝기 마진 기반)
    """
    mode = detect_button_mode(frame_bgra)
    diff, is_confident = detect_difficulty(frame_bgra)
    return mode, diff, is_confident
