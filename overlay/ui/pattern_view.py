from typing import Optional
from PyQt6.QtWidgets import QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QColor, QPainter, QFont, QBrush, QPen

from data.varchive import DIFFICULTIES, DIFF_COLORS


class DiffTab(QFrame):
    """세로 탭 하나 — 난이도 레이블 + floor 힌트."""

    _ACTIVE_BG   = "rgb(63, 80, 117)"
    _INACTIVE_BG = "rgb(28, 36, 54)"
    _DIM_BG      = "rgb(20, 26, 40)"

    def __init__(self, diff: str, parent=None):
        super().__init__(parent)
        self.diff = diff
        self._floor_name: Optional[str] = None
        self._active = False
        self._exists = False

        self.setFixedSize(52, 46)
        self._build_ui()

    def _build_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 6, 0, 6)
        layout.setSpacing(2)

        color = DIFF_COLORS.get(self.diff, "#FFFFFF")
        self._diff_label = QLabel(self.diff)
        self._diff_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._diff_label.setStyleSheet(
            f"color: {color}; font-size: 11px; font-weight: 700; background: transparent;"
        )
        layout.addWidget(self._diff_label)

        self._floor_label = QLabel("—")
        self._floor_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._floor_label.setStyleSheet(
            "color: #8891A7; font-size: 10px; font-weight: 600; background: transparent;"
        )
        layout.addWidget(self._floor_label)

    def set_info(self, floor_name: Optional[str], level: Optional[int]):
        self._exists = True
        self._floor_name = floor_name
        display = floor_name if floor_name else (f"Lv{level}" if level else "—")
        self._floor_label.setText(display)
        self._floor_label.setStyleSheet(
            "color: #B4CBFF; font-size: 10px; font-weight: 600; background: transparent;"
            if self._active else
            "color: #8891A7; font-size: 10px; font-weight: 600; background: transparent;"
        )
        self._update_style()

    def clear(self):
        self._exists = False
        self._floor_name = None
        self._floor_label.setText("—")
        self._update_style()

    def set_active(self, active: bool):
        self._active = active
        color = "#B4CBFF" if active else ("#8891A7" if self._exists else "#505870")
        self._floor_label.setStyleSheet(
            f"color: {color}; font-size: 10px; font-weight: 600; background: transparent;"
        )
        self._update_style()

    def _update_style(self):
        if not self._exists:
            bg = self._DIM_BG
        elif self._active:
            bg = self._ACTIVE_BG
        else:
            bg = self._INACTIVE_BG

        radius_side = "border-radius: 6px;" if self._active else "border-radius: 6px;"
        self.setStyleSheet(
            f"DiffTab {{ background: {bg}; {radius_side} }}"
        )


class VerticalTabPanel(QWidget):
    """NM/HD/MX/SC 세로 탭 패널."""

    tab_clicked = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._tabs: dict[str, DiffTab] = {}
        self._build_ui()

    def _build_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 6, 0, 6)
        layout.setSpacing(4)
        layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        for diff in DIFFICULTIES:
            tab = DiffTab(diff)
            self._tabs[diff] = tab
            layout.addWidget(tab)

        layout.addStretch()
        self.setFixedWidth(52)
        self.setStyleSheet("background: transparent;")

    def update_patterns(self, patterns: list[dict]):
        pattern_map = {p["diff"]: p for p in patterns}
        for diff, tab in self._tabs.items():
            if diff in pattern_map:
                p = pattern_map[diff]
                tab.set_info(p.get("floorName"), p.get("level"))
            else:
                tab.clear()

    def set_active_diff(self, diff: Optional[str]):
        for d, tab in self._tabs.items():
            tab.set_active(d == diff)

    def clear(self):
        for tab in self._tabs.values():
            tab.clear()
        self.set_active_diff(None)
