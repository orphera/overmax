from typing import Optional
from PyQt6.QtWidgets import QFrame, QLabel, QVBoxLayout, QHBoxLayout
from PyQt6.QtCore import Qt, QRect
from PyQt6.QtGui import QColor, QPainter, QFont, QBrush, QPen

from data.varchive import DIFFICULTIES, DIFF_COLORS

class DiffCard(QFrame):
    def __init__(self, diff: str, parent=None):
        super().__init__(parent)
        self.diff = diff
        self.color = QColor(DIFF_COLORS.get(diff, "#FFFFFF"))
        self._level = None
        self._floor_name = None
        self._selected = False   # 현재 선택된 난이도 여부

        self.setFixedSize(72, 64)
        self.setStyleSheet("background: transparent;")

    def set_info(self, level: Optional[int], floor_name: Optional[str]):
        self._level = level
        self._floor_name = floor_name
        self.update()

    def set_selected(self, selected: bool):
        if self._selected != selected:
            self._selected = selected
            self.update()

    def clear(self):
        self._level = None
        self._floor_name = None
        self._selected = False
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        if self._level is None:
            # 비활성 상태
            painter.setBrush(QBrush(QColor(66, 78, 103, 182)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(0, 0, self.width(), self.height(), 6, 6)
            return

        # 배경
        bg = QColor(self.color)
        bg.setAlpha(218)
        if self._selected:
            bg = bg.lighter(110)
        painter.setBrush(QBrush(bg))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(0, 0, self.width(), self.height(), 6, 6)

        # 선택 상태: 보더 + 얇은 상태 레이어
        if self._selected:
            painter.setPen(QPen(QColor(227, 238, 255, 232), 2))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawRoundedRect(1, 1, self.width() - 2, self.height() - 2, 6, 6)

            painter.setBrush(QBrush(QColor(255, 255, 255, 24)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(0, 0, self.width(), self.height(), 6, 6)

        # 난이도 라벨 (NM/HD/MX/SC)
        painter.setPen(QPen(QColor(255, 255, 255, 200)))
        font = QFont("Arial", 9, QFont.Weight.Bold)
        painter.setFont(font)
        painter.drawText(QRect(0, 6, self.width(), 16), Qt.AlignmentFlag.AlignHCenter, self.diff)

        # 공식 레벨
        painter.setPen(QPen(QColor(255, 255, 255)))
        font = QFont("Arial", 18, QFont.Weight.Bold)
        painter.setFont(font)
        painter.drawText(QRect(0, 18, self.width(), 26), Qt.AlignmentFlag.AlignHCenter, str(self._level))

        # 비공식 난이도 (floorName)
        if self._floor_name:
            painter.setPen(QPen(QColor(255, 255, 180)))
            font = QFont("Arial", 10, QFont.Weight.Bold)
            painter.setFont(font)
            painter.drawText(QRect(0, 44, self.width(), 16), Qt.AlignmentFlag.AlignHCenter, self._floor_name)
        else:
            painter.setPen(QPen(QColor(200, 200, 200, 120)))
            font = QFont("Arial", 9)
            painter.setFont(font)
            painter.drawText(QRect(0, 44, self.width(), 16), Qt.AlignmentFlag.AlignHCenter, "-")


class ButtonModePanel(QFrame):
    def __init__(self, parent=None):
        super().__init__(parent)
        self._cards: dict[str, DiffCard] = {}

        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 6, 6, 6)
        layout.setSpacing(4)

        # 난이도 카드 (가로 배열)
        cards_layout = QHBoxLayout()
        cards_layout.setSpacing(3)
        for diff in DIFFICULTIES:
            card = DiffCard(diff)
            self._cards[diff] = card
            cards_layout.addWidget(card)
        layout.addLayout(cards_layout)

        self.setStyleSheet("""
            ButtonModePanel {
                background: rgba(34, 44, 66, 216);
                border-radius: 10px;
            }
        """)

    def set_selected_diff(self, diff: Optional[str]):
        """특정 난이도 카드를 선택 상태로, 나머지는 해제."""
        for d, card in self._cards.items():
            card.set_selected(d == diff)

    def update_patterns(self, patterns: list[dict]):
        """패턴 정보로 카드 업데이트"""
        pattern_map = {p["diff"]: p for p in patterns}
        for diff, card in self._cards.items():
            if diff in pattern_map:
                p = pattern_map[diff]
                card.set_info(p["level"], p.get("floorName"))
            else:
                card.clear()

    def clear(self):
        for card in self._cards.values():
            card.clear()
        self.set_selected_diff(None)
