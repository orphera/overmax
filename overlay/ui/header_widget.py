import sys

try:
    from PyQt6.QtWidgets import QFrame, QHBoxLayout, QLabel, QPushButton
    from PyQt6.QtCore import Qt, pyqtSignal
except ImportError:
    pass

from constants import BTN_COLORS

def _s(base: int, scale: float) -> int:
    return max(1, round(base * scale))

class HeaderWidget(QFrame):
    settings_requested = pyqtSignal()

    def __init__(self, scale: float = 1.0, parent=None):
        super().__init__(parent)
        self._scale = scale
        self._build_ui()

    def _build_ui(self):
        sc = self._scale
        self.setStyleSheet(f"""
            QFrame {{
                background: rgb(30, 40, 62);
                border-radius: {_s(10, sc)}px;
            }}
        """)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(_s(12, sc), _s(8, sc), _s(12, sc), _s(8, sc))
        layout.setSpacing(_s(8, sc))

        self._status_lamp = QLabel()
        self._status_lamp.setFixedSize(_s(7, sc), _s(7, sc))
        self.update_status(False)
        layout.addWidget(self._status_lamp)

        self._mode_label = QLabel("—")
        self._mode_label.setFixedSize(_s(28, sc), _s(22, sc))
        self._mode_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.update_mode("", "")
        layout.addWidget(self._mode_label)

        self._song_label = QLabel("곡을 선택하세요")
        self._song_label.setStyleSheet(
            f"color: #F0F4FF; font-size: {_s(14, sc)}px; font-weight: 700;"
        )
        self._song_label.setAlignment(Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self._song_label, 1)

        self._settings_btn = QPushButton("⚙")
        self._settings_btn.setFixedSize(_s(24, sc), _s(24, sc))
        self._settings_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._settings_btn.setStyleSheet(f"""
            QPushButton {{
                color: #505870;
                background: transparent;
                border: none;
                font-size: {_s(16, sc)}px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                color: #F0F4FF;
            }}
        """)
        self._settings_btn.clicked.connect(self.settings_requested.emit)
        layout.addWidget(self._settings_btn)

    def update_status(self, is_stable: bool):
        sc = self._scale
        color = "#00D4FF" if is_stable else "#FF4B4B"
        self._status_lamp.setStyleSheet(
            f"background-color: {color}; border-radius: {_s(3, sc)}px;"
        )

    def update_mode(self, mode: str, diff: str):
        sc = self._scale
        self._mode_label.setText(mode if mode else "—")
        mode_color = BTN_COLORS.get(mode, [(0x6A, 0x4D, 0x3D)])[0]
        mode_color_str = f"rgb({mode_color[2]}, {mode_color[1]}, {mode_color[0]})"
        self._mode_label.setStyleSheet(
            f"color: #F0F4FF; background-color: {mode_color_str}; "
            f"font-size: {_s(12, sc)}px; font-weight: 900; border-radius: {_s(3, sc)}px;"
        )

    def update_song(self, title: str):
        self._song_label.setText(title)
