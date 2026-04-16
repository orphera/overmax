"""
debug_window.py - Overmax 디버그 로그 창

별도 PyQt6 창으로 실시간 로그를 표시한다.
ScreenCapture / OverlayController 가 emit 하는 로그 문자열을
on_debug_log 콜백 → DebugSignals.log_received → DebugWindow._append_log 경로로 수신.
"""

from typing import Optional, Callable
from settings import SETTINGS

try:
    from PyQt6.QtWidgets import (
        QWidget, QVBoxLayout, QHBoxLayout,
        QTextEdit, QPushButton, QLabel, QCheckBox,
    )
    from PyQt6.QtCore import Qt, pyqtSignal, QObject
    from PyQt6.QtGui import QFont, QColor, QTextCharFormat, QTextCursor
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False


# ------------------------------------------------------------------
# 시그널 브릿지
# ------------------------------------------------------------------

class DebugSignals(QObject):
    log_received = pyqtSignal(str)   # 다른 스레드 → Qt 메인 스레드


# ------------------------------------------------------------------
# 디버그 창
# ------------------------------------------------------------------

class DebugWindow(QWidget):
    MAX_LINES = int(SETTINGS["debug_window"]["max_lines"])   # 이 이상이면 오래된 줄 삭제

    # 모듈별 색상
    TAG_COLORS = {
        "[ScreenCapture]": "#7EC8E3",   # 하늘
        "[Overlay]":       "#B5EAD7",   # 민트
        "[VArchive]":      "#FFD6A5",   # 살구
        "[WindowTracker]": "#C9B1FF",   # 보라
        "[Main]":          "#FFFFB5",   # 노랑
    }
    DEFAULT_COLOR = "#CCCCCC"

    def __init__(self, signals: DebugSignals, parent=None):
        super().__init__(parent)
        self.signals = signals
        self._paused = False
        self._line_count = 0
        self._roi_toggle_cb: Optional[Callable[[bool], None]] = None

        self._setup_window()
        self._setup_ui()
        self.signals.log_received.connect(self._append_log)

    def _setup_window(self):
        self.setWindowTitle(str(SETTINGS["debug_window"]["title"]))
        self.setWindowFlags(
            Qt.WindowType.Window
            | Qt.WindowType.WindowStaysOnTopHint
        )
        self.resize(700, 400)
        self.setStyleSheet("background: #1A1A2E; color: #CCCCCC;")

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(6)

        # 헤더
        header = QHBoxLayout()
        title = QLabel("🔍 Debug Log")
        title.setStyleSheet("color: #7EC8E3; font-weight: bold; font-size: 13px;")
        header.addWidget(title)
        header.addStretch()

        self._pause_btn = QPushButton("⏸ 일시정지")
        self._pause_btn.setCheckable(True)
        self._pause_btn.setStyleSheet("""
            QPushButton { background: #2A2A4A; color: #CCCCCC; border: 1px solid #444; border-radius: 4px; padding: 3px 10px; }
            QPushButton:checked { background: #5A3A1A; color: #FFD6A5; }
        """)
        self._pause_btn.toggled.connect(self._on_pause_toggled)
        header.addWidget(self._pause_btn)

        clear_btn = QPushButton("🗑 지우기")
        clear_btn.setStyleSheet("""
            QPushButton { background: #2A2A4A; color: #CCCCCC; border: 1px solid #444; border-radius: 4px; padding: 3px 10px; }
            QPushButton:hover { background: #3A2A3A; }
        """)
        clear_btn.clicked.connect(self._clear)
        header.addWidget(clear_btn)

        self._roi_btn = QPushButton("ROI 표시 OFF")
        self._roi_btn.setCheckable(True)
        self._roi_btn.setStyleSheet("""
            QPushButton { background: #2A2A4A; color: #CCCCCC; border: 1px solid #444; border-radius: 4px; padding: 3px 10px; }
            QPushButton:checked { background: #1F5A3A; color: #B5EAD7; }
        """)
        self._roi_btn.setEnabled(False)
        self._roi_btn.toggled.connect(self._on_roi_toggled)
        header.addWidget(self._roi_btn)

        layout.addLayout(header)

        # 필터 체크박스
        filter_row = QHBoxLayout()
        self._filters: dict[str, QCheckBox] = {}
        filter_label = QLabel("필터:")
        filter_label.setStyleSheet("color: #888; font-size: 10px;")
        filter_row.addWidget(filter_label)
        for tag, color in self.TAG_COLORS.items():
            short = tag.strip("[]")
            cb = QCheckBox(short)
            cb.setChecked(True)
            cb.setStyleSheet(f"color: {color}; font-size: 10px;")
            self._filters[tag] = cb
            filter_row.addWidget(cb)
        filter_row.addStretch()
        layout.addLayout(filter_row)

        # 로그 텍스트
        self._log_text = QTextEdit()
        self._log_text.setReadOnly(True)
        self._log_text.setFont(QFont("Consolas", 9))
        self._log_text.setStyleSheet("""
            QTextEdit {
                background: #0D0D1A;
                color: #CCCCCC;
                border: 1px solid #333;
                border-radius: 4px;
            }
        """)
        layout.addWidget(self._log_text)

        # 상태바
        self._status = QLabel("대기 중...")
        self._status.setStyleSheet("color: #666; font-size: 9px;")
        layout.addWidget(self._status)

    def _on_pause_toggled(self, checked: bool):
        self._paused = checked
        self._pause_btn.setText("▶ 재개" if checked else "⏸ 일시정지")

    def _clear(self):
        self._log_text.clear()
        self._line_count = 0
        self._status.setText("로그 지워짐")

    def set_roi_toggle_callback(self, callback: Optional[Callable[[bool], None]]):
        self._roi_toggle_cb = callback
        self._roi_btn.setEnabled(callback is not None)
        if callback is None:
            self._roi_btn.setChecked(False)
            self._roi_btn.setText("ROI 표시 OFF")

    def _on_roi_toggled(self, checked: bool):
        self._roi_btn.setText("ROI 표시 ON" if checked else "ROI 표시 OFF")
        if self._roi_toggle_cb:
            self._roi_toggle_cb(checked)

    def _append_log(self, msg: str):
        if self._paused:
            return

        # 필터 체크
        visible_tag = None
        for tag, cb in self._filters.items():
            if tag in msg:
                if not cb.isChecked():
                    return
                visible_tag = tag
                break

        # 최대 라인 초과 시 앞부분 삭제
        if self._line_count >= self.MAX_LINES:
            cursor = self._log_text.textCursor()
            cursor.movePosition(QTextCursor.MoveOperation.Start)
            cursor.movePosition(
                QTextCursor.MoveOperation.Down,
                QTextCursor.MoveMode.KeepAnchor,
                50,   # 50줄 삭제
            )
            cursor.removeSelectedText()
            self._line_count -= 50

        # 색상 결정
        color_str = self.TAG_COLORS.get(visible_tag, self.DEFAULT_COLOR) if visible_tag else self.DEFAULT_COLOR

        # 텍스트 추가
        cursor = self._log_text.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        fmt = QTextCharFormat()
        fmt.setForeground(QColor(color_str))
        cursor.setCharFormat(fmt)
        cursor.insertText(msg + "\n")

        self._log_text.setTextCursor(cursor)
        self._log_text.ensureCursorVisible()
        self._line_count += 1
        self._status.setText(f"총 {self._line_count}줄")


# ------------------------------------------------------------------
# 컨트롤러 (다른 모듈에서 사용)
# ------------------------------------------------------------------

class DebugController:
    """ScreenCapture / OverlayController 와 연결하는 진입점"""

    def __init__(self):
        self.signals = DebugSignals()
        self._window: Optional[DebugWindow] = None
        self._roi_toggle_cb: Optional[Callable[[bool], None]] = None

    def create_window(self) -> Optional["DebugWindow"]:
        """Qt App 생성 후에 호출해야 함"""
        if not PYQT_AVAILABLE:
            return None
        if self._window is None:
            self._window = DebugWindow(self.signals)
            self._window.set_roi_toggle_callback(self._roi_toggle_cb)
        return self._window

    def show_window(self):
        window = self.create_window()
        if window is not None:
            window.show()
            window.raise_()
            window.activateWindow()

    def hide_window(self):
        if self._window is not None:
            self._window.hide()

    def toggle_window(self):
        if not PYQT_AVAILABLE:
            return
        window = self.create_window()
        if window is None:
            return
        if window.isVisible():
            window.hide()
        else:
            window.show()
            window.raise_()
            window.activateWindow()

    def log(self, msg: str):
        """스레드-안전 로그 emit"""
        self.signals.log_received.emit(msg)

    def set_roi_toggle_callback(self, callback: Optional[Callable[[bool], None]]):
        self._roi_toggle_cb = callback
        if self._window is not None:
            self._window.set_roi_toggle_callback(callback)
