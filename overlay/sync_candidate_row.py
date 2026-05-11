"""Candidate row widget for the V-Archive sync window."""

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtWidgets import QFrame, QHBoxLayout, QLabel, QPushButton

from data.sync_manager import SyncCandidate


_DIFF_COLORS = {
    "NM": "#4A90D9",
    "HD": "#F5A623",
    "MX": "#D0021B",
    "SC": "#9B59B6",
}

_BTN_COLORS = {
    "4B": "#2D7A8C",
    "5B": "#44A9C6",
    "6B": "#ED9430",
    "8B": "#4A2060",
}


class CandidateRow(QFrame):
    upload_requested = pyqtSignal(int)
    delete_requested = pyqtSignal(int)

    def __init__(self, index: int, candidate: SyncCandidate, parent=None):
        super().__init__(parent)
        self.index = index
        self.candidate = candidate
        self._build_ui()

    def _build_ui(self):
        self.setFixedHeight(48)
        self.setStyleSheet(
            "QFrame { background: rgb(28, 36, 56); border-radius: 6px; border: none; }"
        )

        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 0, 8, 0)
        layout.setSpacing(8)

        layout.addWidget(self._build_diff_badge())
        layout.addWidget(self._build_mode_badge())
        layout.addWidget(self._build_name_label(), 1)
        layout.addWidget(self._build_overmax_label())
        layout.addWidget(self._build_arrow_label())
        layout.addWidget(self._build_varchive_label())
        layout.addWidget(self._build_reason_label())
        layout.addLayout(self._build_action_layout())

    def _build_diff_badge(self) -> QLabel:
        diff_badge = QLabel(self.candidate.difficulty)
        diff_badge.setFixedWidth(32)
        diff_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        color = _DIFF_COLORS.get(self.candidate.difficulty, "#FFFFFF")
        diff_badge.setStyleSheet(
            f"background: {color}; color: #FFFFFF; font-size: 10px; "
            f"font-weight: 700; border-radius: 4px; padding: 2px 0;"
        )
        return diff_badge

    def _build_mode_badge(self) -> QLabel:
        mode_badge = QLabel(self.candidate.button_mode)
        mode_badge.setFixedWidth(28)
        mode_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        mode_color = _BTN_COLORS.get(self.candidate.button_mode, "#444")
        mode_badge.setStyleSheet(
            f"background: {mode_color}; color: #FFFFFF; font-size: 9px; "
            f"font-weight: 700; border-radius: 3px; padding: 2px 0;"
        )
        return mode_badge

    def _build_name_label(self) -> QLabel:
        name_label = QLabel(self.candidate.song_name)
        name_label.setStyleSheet("color: #E8EEFF; font-size: 12px; font-weight: 600;")
        try:
            fm = name_label.fontMetrics()
            elided = fm.elidedText(self.candidate.song_name, Qt.TextElideMode.ElideRight, 180)
            name_label.setText(elided)
        except Exception:
            pass
        return name_label

    def _build_overmax_label(self) -> QLabel:
        om_label = QLabel(f"{self.candidate.overmax_rate:.2f}%")
        om_label.setFixedWidth(64)
        om_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        om_label.setStyleSheet("color: #00D4FF; font-size: 12px; font-weight: 700;")
        if self.candidate.overmax_mc:
            om_label.setText(om_label.text() + " M")
        return om_label

    def _build_arrow_label(self) -> QLabel:
        arrow = QLabel("→")
        arrow.setFixedWidth(16)
        arrow.setAlignment(Qt.AlignmentFlag.AlignCenter)
        arrow.setStyleSheet("color: #505870; font-size: 11px;")
        return arrow

    def _build_varchive_label(self) -> QLabel:
        if self.candidate.varchive_rate is None:
            va_text = "——"
            va_color = "#505870"
        else:
            va_mc_mark = " M" if self.candidate.varchive_mc else ""
            va_text = f"{self.candidate.varchive_rate:.2f}%{va_mc_mark}"
            va_color = "#8891A7"

        va_label = QLabel(va_text)
        va_label.setFixedWidth(72)
        va_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        va_label.setStyleSheet(f"color: {va_color}; font-size: 12px;")
        return va_label

    def _build_reason_label(self) -> QLabel:
        reason_label = QLabel(self.candidate.reason)
        reason_label.setFixedWidth(64)
        reason_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        reason_label.setStyleSheet("color: #FFD166; font-size: 10px; font-weight: 600;")
        return reason_label

    def _build_action_layout(self) -> QHBoxLayout:
        self._upload_btn = self._build_upload_btn()
        self._delete_btn = self._build_delete_btn()
        action_layout = QHBoxLayout()
        action_layout.setSpacing(2)
        action_layout.addWidget(self._upload_btn)
        action_layout.addWidget(self._delete_btn)
        return action_layout

    def _build_upload_btn(self) -> QPushButton:
        btn = QPushButton("등록")
        btn.setFixedSize(36, 28)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setStyleSheet("""
            QPushButton {
                background: rgb(0, 180, 120);
                color: #FFFFFF;
                border: none;
                border-radius: 5px;
                font-size: 10px;
                font-weight: 700;
            }
            QPushButton:hover { background: rgb(0, 210, 140); }
            QPushButton:disabled { background: rgb(40, 50, 80); color: #505870; }
        """)
        btn.clicked.connect(lambda: self.upload_requested.emit(self.index))
        return btn

    def _build_delete_btn(self) -> QPushButton:
        btn = QPushButton("삭제")
        btn.setFixedSize(36, 28)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setStyleSheet("""
            QPushButton {
                background: rgb(180, 60, 60);
                color: #FFFFFF;
                border: none;
                border-radius: 5px;
                font-size: 10px;
                font-weight: 700;
            }
            QPushButton:hover { background: rgb(210, 80, 80); }
            QPushButton:disabled { background: rgb(40, 50, 80); color: #505870; }
        """)
        btn.clicked.connect(lambda: self.delete_requested.emit(self.index))
        return btn

    def set_status(self, status: str, message: str):
        style_map = {
            "pending": ("rgb(180, 140, 0)", "처리 중"),
            "success": ("rgb(0, 180, 120)", "완료"),
            "no_update": ("rgb(60, 80, 120)", "최신"),
            "error": ("rgb(180, 60, 60)", "실패"),
        }
        bg, text = style_map.get(status, ("rgb(60, 80, 120)", status))
        self._upload_btn.setEnabled(False)
        self._upload_btn.setStyleSheet(
            f"QPushButton {{ background: {bg}; color: #FFFFFF; "
            f"border: none; border-radius: 5px; font-size: 10px; font-weight: 700; }}"
        )
        self._upload_btn.setText(text)
        if status == "error" and message:
            self._upload_btn.setToolTip(message)

        self._delete_btn.setEnabled(True)

    def set_upload_enabled(self, enabled: bool):
        self._upload_btn.setEnabled(enabled)
