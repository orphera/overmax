from PyQt6.QtWidgets import QFrame, QLabel, QVBoxLayout, QHBoxLayout
from PyQt6.QtCore import Qt

from data.recommend import RecommendEntry

class PatternRow(QFrame):
    def __init__(self, entry: RecommendEntry, parent=None):
        super().__init__(parent)
        self.entry = entry
        self._setup_ui()

    def _setup_ui(self):
        try:
            self.setFixedHeight(34)
            self.setStyleSheet(
                "background: rgba(44, 56, 82, 214); "
                "border-radius: 8px;"
            )

            layout = QHBoxLayout(self)
            layout.setContentsMargins(10, 4, 10, 4)
            layout.setSpacing(8)

            e = self.entry

            # 난이도 뱃지
            badge = QLabel(e.difficulty)
            badge.setFixedWidth(32)
            badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
            badge.setStyleSheet(
                f"background: {e.color}; color: #FFFFFF; font-size: 10px; "
                "font-weight: 700; border-radius: 6px; padding: 2px;"
            )
            layout.addWidget(badge)

            # floor 표시
            floor_str = e.floor_name if e.floor_name else (f"Lv.{e.level}" if e.level else "?")
            floor_label = QLabel(floor_str)
            floor_label.setFixedWidth(34)
            floor_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
            floor_label.setStyleSheet("color: #FFE2B6; font-size: 11px; font-weight: 700;")
            layout.addWidget(floor_label)

            # 곡명 + 작곡가
            name_col = QVBoxLayout()
            name_col.setSpacing(1)
            name_col.setContentsMargins(0, 0, 0, 0)

            song_label = QLabel(e.song_name)
            song_label.setStyleSheet("color: #F7FAFF; font-size: 11px; font-weight: 700;")
            song_label.setMaximumWidth(160)
            try:
                elided = song_label.fontMetrics().elidedText(
                    e.song_name, Qt.TextElideMode.ElideRight, 150
                )
                song_label.setText(elided)
            except Exception:
                song_label.setText(e.song_name[:15] + "..." if len(e.song_name) > 15 else e.song_name)
            name_col.addWidget(song_label)

            comp_label = QLabel(e.composer)
            comp_label.setStyleSheet("color: #B0BAD2; font-size: 9px;")
            comp_label.setMaximumWidth(160)
            try:
                comp_elided = comp_label.fontMetrics().elidedText(
                    e.composer, Qt.TextElideMode.ElideRight, 150
                )
                comp_label.setText(comp_elided)
            except Exception:
                comp_label.setText(e.composer[:15] + "..." if len(e.composer) > 15 else e.composer)
            name_col.addWidget(comp_label)

            layout.addLayout(name_col)
            layout.addStretch()

            # Rate
            if e.is_played:
                rate_label = QLabel(f"{e.rate:.2f}%")
                rate_label.setFixedWidth(52)
                rate_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
                rate_label.setStyleSheet(
                    f"color: {self._rate_color(e.rate)}; font-size: 11px; font-weight: 700;"
                )
                layout.addWidget(rate_label)
            else:
                dash = QLabel("—")
                dash.setFixedWidth(52)
                dash.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
                dash.setStyleSheet("color: #7A8398; font-size: 11px;")
                layout.addWidget(dash)
        except Exception as ex:
            print(f"[PatternRow] _setup_ui 오류: {ex}")

    @staticmethod
    def _rate_color(rate: float) -> str:
        if rate >= 99.0:
            return "#FFD700"
        elif rate >= 95.0:
            return "#7EC8E3"
        elif rate >= 90.0:
            return "#B5EAD7"
        else:
            return "#FF9999"
