from PyQt6.QtWidgets import QFrame, QLabel, QHBoxLayout, QWidget
from PyQt6.QtCore import Qt

from data.recommend import RecommendEntry
from data.varchive import DIFF_COLORS


def _s(base: int, scale: float) -> int:
    return max(1, round(base * scale))


class PatternRow(QFrame):
    """추천 패턴 한 행 — [뱃지] [곡명] [rate]"""

    def __init__(self, entry: RecommendEntry, scale: float = 1.0, parent=None):
        super().__init__(parent)
        self.entry = entry
        self._scale = scale
        self._build_ui()

    def _build_ui(self):
        sc = self._scale
        self.setFixedHeight(_s(30, sc))
        self.setStyleSheet(
            f"background: rgb(36, 46, 70); border-radius: {_s(6, sc)}px;"
        )

        layout = QHBoxLayout(self)
        layout.setContentsMargins(_s(8, sc), 0, _s(8, sc), 0)
        layout.setSpacing(_s(8, sc))

        layout.addWidget(self._create_diff_badge())

        # 곡명
        song_label = QLabel(self.entry.song_name)
        song_label.setStyleSheet(
            f"color: #E8EEFF; font-size: {_s(11, sc)}px; font-weight: 600;"
        )
        try:
            elided = song_label.fontMetrics().elidedText(
                self.entry.song_name, Qt.TextElideMode.ElideRight, _s(140, sc)
            )
            song_label.setText(elided)
        except Exception:
            pass
        layout.addWidget(song_label, 1)

        layout.addWidget(self._build_rate_widget())

    def _create_diff_badge(self) -> QLabel:
        sc = self._scale
        badge = QLabel(self.entry.difficulty)
        badge.setFixedWidth(_s(28, sc))
        badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        color = DIFF_COLORS.get(self.entry.difficulty, "#FFFFFF")
        badge.setStyleSheet(
            f"background: {color}; color: #FFFFFF; font-size: {_s(10, sc)}px; "
            f"font-weight: 700; border-radius: {_s(4, sc)}px; padding: 1px 0;"
        )
        return badge

    def _build_rate_widget(self) -> QWidget:
        sc = self._scale
        if not self.entry.is_played:
            dash = QLabel("——")
            dash.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            dash.setStyleSheet(f"color: #505870; font-size: {_s(11, sc)}px;")
            return dash

        wrapper = QWidget()
        rate_layout = QHBoxLayout(wrapper)
        rate_layout.setContentsMargins(0, 0, 0, 0)
        rate_layout.setSpacing(_s(6, sc))

        rate_label = QLabel(f"{self.entry.rate:.2f}%")
        rate_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        rate_label.setStyleSheet(
            f"color: {self._rate_color(self.entry.rate)}; "
            f"font-size: {_s(11, sc)}px; font-weight: 700;"
        )

        status_badge = self._create_status_badge()
        if status_badge:
            rate_layout.addWidget(status_badge)

        rate_layout.addWidget(rate_label)

        return wrapper

    def _create_status_badge(self) -> QLabel | None:
        sc = self._scale
        badge_text, badge_style = self._status_badge_info()
        if not badge_text:
            return None

        diameter = _s(16, sc)
        badge = QLabel(badge_text)
        badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        badge.setFixedSize(diameter, diameter)
        badge.setStyleSheet(
            "padding: 0;"
            f"border-radius: {diameter // 2}px;"
            "border: 1px solid rgba(255,255,255,0.22); "
            f"font-size: {_s(9, sc)}px; font-weight: 800; color: #FFFFFF; {badge_style}"
        )
        return badge

    def _status_badge_info(self) -> tuple[str, str]:
        if self.entry.is_perfect_play:
            return (
                "P",
                "background: qlineargradient(x1:0,y1:0,x2:1,y2:1, stop:0 #7E3CFF, stop:1 #FF2D8D);",
            )
        if self.entry.is_max_combo_play:
            return (
                "M",
                "background: qlineargradient(x1:0,y1:0,x2:1,y2:1, stop:0 #30C8FF, stop:1 #7AF56A);",
            )
        return "", ""

    @staticmethod
    def _rate_color(rate: float) -> str:
        if rate >= 100.0:
            return "#FFD700"
        if rate >= 99.0:
            return "#B8DCFF"
        if rate >= 95.0:
            return "#7EC8E3"
        if rate >= 90.0:
            return "#B5EAD7"
        return "#FF9999"
