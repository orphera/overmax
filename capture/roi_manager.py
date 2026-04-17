"""
roi_manager.py - ROI(Region of Interest) 좌표 관리 및 해상도 변환 모듈

1920x1080 해상도를 기준으로 정의된 ROI 좌표를 현재 창 크기에 맞게 변환한다.
DJMAX RESPECT V의 특성상 비-16:9 해상도에서는 Letterbox/Pillarbox가 발생하는 것으로 가정한다.
"""

from typing import Dict, Tuple

from constants import (REF_WIDTH, REF_HEIGHT)


class ROIManager:
    # 기준 해상도 (16:9)
    REF_ASPECT = REF_WIDTH / REF_HEIGHT

    # 기준 ROI 정의 (x1, y1, x2, y2) - 1920x1080 픽셀 기준
    ROIS = {
        "logo": (167, 23, 303, 49),
        "jacket": (710, 535, 770, 593),
        "rate": (176, 583, 270, 605),
        "btn_mode": (80, 130, 85, 135),
        "diff_panel": (98, 488, 208, 516),
    }

    # 난이도별 X 오프셋 (1920x1080 기준)
    DIFF_OFFSETS = {
        "NM": 0,
        "HD": 120,
        "MX": 240,
        "SC": 360,
    }

    def __init__(self, w: int = 1920, h: int = 1080):
        self.width = w
        self.height = h
        self.scale = 1.0
        self.offset_x = 0
        self.offset_y = 0
        self._calculate_transformation()

    def update_window_size(self, w: int, h: int):
        """창 크기 변경 시 호출하여 변환 계수를 재계산한다."""
        if self.width == w and self.height == h:
            return
        self.width = w
        self.height = h
        self._calculate_transformation()

    def _calculate_transformation(self):
        """Letterbox/Pillarbox를 고려한 스케일 및 오프셋 계산."""
        if self.width <= 0 or self.height <= 0:
            return

        current_aspect = self.width / self.height

        if current_aspect > self.REF_ASPECT:
            # Pillarbox (좌우 검은 막대) - 창이 16:9보다 더 넓음 (예: 17:9, 21:9)
            self.scale = self.height / REF_HEIGHT
            active_w = REF_WIDTH * self.scale
            self.offset_x = int((self.width - active_w) / 2)
            self.offset_y = 0
        elif current_aspect < self.REF_ASPECT:
            # Letterbox (상하 검은 막대) - 창이 16:9보다 더 좁음 (예: 16:10, 4:3)
            self.scale = self.width / REF_WIDTH
            active_h = REF_HEIGHT * self.scale
            self.offset_x = 0
            self.offset_y = int((self.height - active_h) / 2)
        else:
            # 완전한 16:9
            self.scale = self.width / REF_WIDTH
            self.offset_x = 0
            self.offset_y = 0

    def get_roi(self, name: str) -> Tuple[int, int, int, int]:
        """지정한 이름의 ROI를 현재 해상도에 맞춰 변환하여 반환 (x1, y1, x2, y2)."""
        if name not in self.ROIS:
            raise KeyError(f"Unknown ROI name: {name}")
        return self.transform_roi(self.ROIS[name])

    def transform_roi(self, roi: Tuple[int, int, int, int]) -> Tuple[int, int, int, int]:
        """임의의 ROI (x1, y1, x2, y2)를 현재 해상도에 맞춰 변환."""
        x1, y1, x2, y2 = roi
        tx1, ty1 = self.transform_point(x1, y1)
        tx2, ty2 = self.transform_point(x2, y2)
        return tx1, ty1, tx2, ty2

    def transform_point(self, x: int, y: int) -> Tuple[int, int]:
        """1920x1080 기준 좌표 (x, y)를 현재 해상도에 맞춰 변환."""
        tx = int(self.offset_x + (x * self.scale))
        ty = int(self.offset_y + (y * self.scale))
        return tx, ty

    def get_scaled_value(self, val: float) -> int:
        """단순히 현재 스케일에 맞춰 크기만 조정한 값을 반환 (오프셋 제외)."""
        return int(val * self.scale)

    def get_diff_panel_roi(self, diff: str) -> Tuple[int, int, int, int]:
        """난이도별 패널 ROI를 현재 해상도에 맞춰 변환하여 반환."""
        base_x1, y1, base_x2, y2 = self.ROIS["diff_panel"]
        offset = self.DIFF_OFFSETS.get(diff, 0)
        return self.transform_roi((base_x1 + offset, y1, base_x2 + offset, y2))
