"""PyQt6 overlay window and signal bridge."""

from typing import Optional

try:
    from PyQt6.QtWidgets import (
        QWidget,
        QLabel,
        QVBoxLayout,
        QHBoxLayout,
        QFrame,
        QScrollArea,
        QApplication,
    )
    from PyQt6.QtCore import Qt, pyqtSignal, QObject, QPoint
    from PyQt6.QtGui import QPainter, QBrush, QColor
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

from data.varchive import BUTTON_MODES
from data.recommend import RecommendEntry
from overlay.ui.pattern_view import ButtonModePanel
from overlay.ui.recommend_view import PatternRow


if PYQT_AVAILABLE:

    class OverlaySignals(QObject):
        song_changed = pyqtSignal(str, list)
        screen_changed = pyqtSignal(bool)
        position_changed = pyqtSignal(int, int, int, int)
        roi_enabled_changed = pyqtSignal(bool)
        mode_diff_changed = pyqtSignal(str, str, bool)
        recommend_ready = pyqtSignal(list, str, bool)


    class OverlayWindow(QWidget):
        def __init__(self, signals: OverlaySignals):
            super().__init__()
            self.signals = signals
            self._current_mode: Optional[str] = None
            self._current_diff: Optional[str] = None
            self._patterns_cache: dict[str, list] = {}
            self._pattern_panel: Optional[ButtonModePanel] = None
            self._song_label: Optional[QLabel] = None
            self._mode_indicator: Optional[QLabel] = None
            self._dragging = False
            self._drag_pos = QPoint()
            self._manual_position = False
            self._user_move_cb = None

            self._setup_window()
            self._setup_ui()
            self._connect_signals()

        def _setup_window(self):
            self.setWindowFlags(
                Qt.WindowType.FramelessWindowHint
                | Qt.WindowType.WindowStaysOnTopHint
                | Qt.WindowType.Tool
            )
            self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
            self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
            self.setFixedWidth(330)

        def _setup_ui(self):
            main_layout = QVBoxLayout(self)
            main_layout.setContentsMargins(8, 8, 8, 8)
            main_layout.setSpacing(0)

            panel = QFrame()
            panel.setStyleSheet(
                """
                QFrame {
                    background: rgba(20, 27, 42, 236);
                    border-radius: 14px;
                }
                """
            )

            panel_layout = QVBoxLayout(panel)
            panel_layout.setContentsMargins(8, 8, 8, 8)
            panel_layout.setSpacing(6)

            panel_layout.addWidget(self._build_header())
            panel_layout.addWidget(self._build_mode_indicator())

            self._pattern_panel = ButtonModePanel()
            panel_layout.addWidget(self._pattern_panel)

            panel_layout.addLayout(self._build_recommend_header())
            panel_layout.addWidget(self._build_recommend_scroll())
            main_layout.addWidget(panel)
            self.adjustSize()

        def _build_header(self) -> QFrame:
            header = QFrame()
            header.setStyleSheet(
                """
                QFrame {
                    background: rgba(36, 46, 70, 230);
                    border-radius: 12px;
                }
                """
            )
            header_layout = QHBoxLayout(header)
            header_layout.setContentsMargins(12, 8, 12, 8)

            badge = QLabel("Overmax")
            badge.setStyleSheet("color: #B4CBFF; font-size: 10px; font-weight: 700;")
            header_layout.addWidget(badge)

            self._status_lamp = QLabel()
            self._status_lamp.setFixedSize(8, 8)
            self._status_lamp.setStyleSheet("background-color: #FF4B4B; border-radius: 4px;")
            self._status_lamp.setToolTip("인식 검증 중...")
            header_layout.addWidget(self._status_lamp)

            self._song_label = QLabel("곡을 선택하세요")
            self._song_label.setStyleSheet("color: #F4F7FF; font-size: 13px; font-weight: 700;")
            self._song_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
            header_layout.addWidget(self._song_label, 1)

            hint = QLabel("드래그")
            hint.setStyleSheet("color: #8891A7; font-size: 9px;")
            header_layout.addWidget(hint)
            return header

        def _build_mode_indicator(self) -> QLabel:
            self._mode_indicator = QLabel("— / —")
            self._mode_indicator.setAlignment(Qt.AlignmentFlag.AlignCenter)
            self._mode_indicator.setStyleSheet(
                """
                color: #DDE7FF;
                background: rgba(63, 80, 117, 214);
                border-radius: 10px;
                font-size: 10px;
                font-weight: 700;
                padding: 5px 8px;
                """
            )
            return self._mode_indicator

        def _build_recommend_header(self) -> QHBoxLayout:
            rec_header = QHBoxLayout()
            rec_title = QLabel("유사 난이도 추천")
            rec_title.setStyleSheet("color: #B4CBFF; font-size: 10px; font-weight: 700;")
            rec_header.addWidget(rec_title)
            rec_header.addStretch()
            self._rec_count_label = QLabel("")
            self._rec_count_label.setStyleSheet("color: #A5B1CD; font-size: 8px;")
            rec_header.addWidget(self._rec_count_label)
            return rec_header

        def _build_recommend_scroll(self) -> QScrollArea:
            self._rec_scroll = QScrollArea()
            self._rec_scroll.setWidgetResizable(True)
            self._rec_scroll.setFixedHeight(186)
            self._rec_scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
            self._rec_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
            self._rec_scroll.setFrameShape(QFrame.Shape.NoFrame)
            self._rec_scroll.setStyleSheet(
                """
                QScrollArea { background: transparent; }
                QScrollBar:vertical {
                    background: transparent;
                    width: 4px;
                }
                QScrollBar::handle:vertical {
                    background: rgba(187, 209, 255, 120);
                    border-radius: 2px;
                }
                """
            )

            self._rec_widget = QWidget()
            self._rec_widget.setStyleSheet("background: transparent;")
            self._rec_layout = QVBoxLayout(self._rec_widget)
            self._rec_layout.setContentsMargins(0, 0, 4, 0)
            self._rec_layout.setSpacing(4)
            self._rec_scroll.setWidget(self._rec_widget)
            return self._rec_scroll

        def _connect_signals(self):
            self.signals.song_changed.connect(self._on_song_changed)
            self.signals.screen_changed.connect(self._on_screen_changed)
            self.signals.position_changed.connect(self._on_game_window_moved)
            self.signals.mode_diff_changed.connect(self._on_mode_diff_changed)
            self.signals.recommend_ready.connect(self._on_recommend_ready)

        def _on_recommend_ready(
            self,
            entries: list[RecommendEntry],
            pivot_str: str,
            no_selection: bool,
        ):
            try:
                while self._rec_layout.count() > 0:
                    item = self._rec_layout.takeAt(0)
                    if item and item.widget():
                        item.widget().deleteLater()

                if no_selection or not entries:
                    message = "패턴을 감지하는 중..." if no_selection else "추천 결과 없음"
                    empty = QLabel(message)
                    empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
                    empty.setStyleSheet("color: #9DA8C4; font-size: 10px; padding: 20px;")
                    self._rec_layout.addWidget(empty)
                    self._rec_layout.addStretch()
                    self._rec_count_label.setText("")
                    return

                for entry in entries:
                    self._rec_layout.addWidget(PatternRow(entry))
                self._rec_layout.addStretch()

                played = sum(1 for e in entries if e.is_played)
                self._rec_count_label.setText(f"{len(entries)}개 결과 (기록 {played})")
            except Exception as exc:
                print(f"[Overlay] _on_recommend_ready 오류: {exc}")

        def _on_song_changed(self, title: str, all_patterns: list):
            self._song_label.setText(title)
            self._patterns_cache = {item["mode"]: item["patterns"] for item in all_patterns}
            self._apply_mode_diff_highlight()

        def _on_screen_changed(self, is_song_select: bool):
            if is_song_select:
                self.show()
            else:
                self.hide()

        def _on_game_window_moved(self, left, top, width, height):
            if self._manual_position:
                return
            ox = left + width + 10
            oy = top + height - self.height() - 40
            screen = QApplication.primaryScreen().geometry()
            if ox + self.width() > screen.width():
                ox = left - self.width() - 10
            self.move(ox, max(oy, top))

        def _on_mode_diff_changed(self, mode: str, diff: str, verified: bool):
            if verified:
                self._status_lamp.setStyleSheet("background-color: #00D4FF; border-radius: 4px;")
                self._status_lamp.setToolTip("인식 완료")
                self._current_mode = mode if mode else None
                self._current_diff = diff if diff else None
                self._apply_mode_diff_highlight()
                return

            self._status_lamp.setStyleSheet("background-color: #FF4B4B; border-radius: 4px;")
            self._status_lamp.setToolTip("인식 검증 중...")

        def _apply_mode_diff_highlight(self):
            display_mode = self._current_mode or BUTTON_MODES[0]
            patterns = self._patterns_cache.get(display_mode, [])

            if self._pattern_panel:
                self._pattern_panel.update_patterns(patterns)
                self._pattern_panel.set_selected_diff(self._current_diff)

            mode_str = self._current_mode or "—"
            diff_str = self._current_diff or "—"
            self._mode_indicator.setText(f"현재: {mode_str}  /  {diff_str}")
            self.adjustSize()

        def set_user_move_callback(self, callback):
            self._user_move_cb = callback

        def apply_saved_position(self, x: int, y: int):
            self._manual_position = True
            self.move(x, y)

        def toggle_visibility(self):
            if self.isVisible():
                self.hide()
            else:
                self.show()

        def mousePressEvent(self, event):
            if event.button() == Qt.MouseButton.LeftButton:
                self._dragging = True
                self._drag_pos = event.globalPosition().toPoint() - self.frameGeometry().topLeft()

        def mouseMoveEvent(self, event):
            if self._dragging:
                self.move(event.globalPosition().toPoint() - self._drag_pos)

        def mouseReleaseEvent(self, event):
            if self._dragging:
                self._dragging = False
                self._manual_position = True
                if self._user_move_cb is not None:
                    self._user_move_cb(self.x(), self.y())
                return
            self._dragging = False

        def paintEvent(self, event):
            painter = QPainter(self)
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            painter.setBrush(QBrush(QColor(0, 0, 0, 40)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(self.rect().adjusted(4, 5, -1, -1), 14, 14)

            painter.setBrush(QBrush(QColor(0, 0, 0, 68)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(self.rect().adjusted(2, 3, -2, -1), 14, 14)

else:

    class OverlaySignals:
        pass


    class OverlayWindow:
        def __init__(self, *args, **kwargs):
            raise RuntimeError("PyQt6 is required for OverlayWindow")
