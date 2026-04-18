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
from overlay.ui.pattern_view import VerticalTabPanel
from overlay.ui.recommend_view import PatternRow


if PYQT_AVAILABLE:

    class OverlaySignals(QObject):
        song_changed      = pyqtSignal(str, list)
        screen_changed    = pyqtSignal(bool)
        position_changed  = pyqtSignal(int, int, int, int)
        roi_enabled_changed = pyqtSignal(bool)
        mode_diff_changed = pyqtSignal(str, str, bool)
        recommend_ready   = pyqtSignal(list, str, bool)
        visibility_toggle_requested = pyqtSignal()


    class OverlayWindow(QWidget):
        def __init__(self, signals: OverlaySignals):
            super().__init__()
            self.signals = signals
            self._current_mode: Optional[str] = None
            self._current_diff: Optional[str] = None
            self._patterns_cache: dict[str, list] = {}
            self._tab_panel: Optional[VerticalTabPanel] = None
            self._song_label: Optional[QLabel] = None
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
            self.setFixedWidth(360)

        def _setup_ui(self):
            root = QVBoxLayout(self)
            root.setContentsMargins(0, 0, 0, 0)
            root.setSpacing(0)

            panel = QFrame()
            panel.setStyleSheet("""
                QFrame {
                    background: rgba(18, 24, 38, 240);
                    border-radius: 14px;
                }
            """)

            panel_layout = QVBoxLayout(panel)
            panel_layout.setContentsMargins(8, 8, 8, 8)
            panel_layout.setSpacing(6)

            panel_layout.addWidget(self._build_header())
            panel_layout.addWidget(self._build_body())

            root.addWidget(panel)
            self.adjustSize()

        # ------------------------------------------------------------------
        # 헤더
        # ------------------------------------------------------------------

        def _build_header(self) -> QFrame:
            header = QFrame()
            header.setStyleSheet("""
                QFrame {
                    background: rgba(30, 40, 62, 220);
                    border-radius: 10px;
                }
            """)
            layout = QHBoxLayout(header)
            layout.setContentsMargins(12, 8, 12, 8)
            layout.setSpacing(8)

            self._status_lamp = QLabel()
            self._status_lamp.setFixedSize(7, 7)
            self._status_lamp.setStyleSheet(
                "background-color: #FF4B4B; border-radius: 3px;"
            )
            layout.addWidget(self._status_lamp)

            self._song_label = QLabel("곡을 선택하세요")
            self._song_label.setStyleSheet(
                "color: #F0F4FF; font-size: 14px; font-weight: 700;"
            )
            self._song_label.setAlignment(Qt.AlignmentFlag.AlignVCenter)
            layout.addWidget(self._song_label, 1)

            drag_hint = QLabel("⠿")
            drag_hint.setStyleSheet("color: #3D4D6A; font-size: 13px;")
            layout.addWidget(drag_hint)
            return header

        # ------------------------------------------------------------------
        # 바디: 세로 탭 | 추천 목록
        # ------------------------------------------------------------------

        def _build_body(self) -> QFrame:
            body = QFrame()
            body.setStyleSheet("background: transparent;")
            layout = QHBoxLayout(body)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.setSpacing(6)

            # 왼쪽: 세로 탭
            self._tab_panel = VerticalTabPanel()
            layout.addWidget(self._tab_panel)

            # 오른쪽: 추천 목록
            layout.addWidget(self._build_recommend_panel(), 1)
            return body

        def _build_recommend_panel(self) -> QWidget:
            wrapper = QWidget()
            wrapper.setStyleSheet("background: transparent;")
            layout = QVBoxLayout(wrapper)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.setSpacing(4)

            self._rec_widget = QWidget()
            self._rec_widget.setStyleSheet("background: transparent;")
            self._rec_layout = QVBoxLayout(self._rec_widget)
            self._rec_layout.setContentsMargins(0, 8, 0, 8)
            self._rec_layout.setSpacing(3)
            layout.addWidget(self._rec_widget)
            return wrapper

        # ------------------------------------------------------------------
        # 시그널 연결
        # ------------------------------------------------------------------

        def _connect_signals(self):
            self.signals.song_changed.connect(self._on_song_changed)
            self.signals.screen_changed.connect(self._on_screen_changed)
            self.signals.position_changed.connect(self._on_game_window_moved)
            self.signals.mode_diff_changed.connect(self._on_mode_diff_changed)
            self.signals.recommend_ready.connect(self._on_recommend_ready)
            self.signals.visibility_toggle_requested.connect(self.toggle_visibility)

        # ------------------------------------------------------------------
        # 슬롯
        # ------------------------------------------------------------------

        def _on_song_changed(self, title: str, all_patterns: list):
            self._song_label.setText(f"{self._current_mode} :: {title}" if self._current_mode else title)
            self._patterns_cache = {item["mode"]: item["patterns"] for item in all_patterns}
            self._apply_tab_update()

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
            color = "#00D4FF" if verified else "#FF4B4B"
            self._status_lamp.setStyleSheet(
                f"background-color: {color}; border-radius: 3px;"
            )
            if verified:
                self._current_mode = mode or None
                self._current_diff = diff or None
                self._apply_tab_update()

        def _on_recommend_ready(
            self,
            entries: list[RecommendEntry],
            pivot_str: str,
            no_selection: bool,
        ):
            # 기존 행 제거
            while self._rec_layout.count() > 0:
                item = self._rec_layout.takeAt(0)
                if item and item.widget():
                    item.widget().deleteLater()

            if no_selection or not entries:
                msg = "패턴을 감지하는 중..." if no_selection else "추천 결과 없음"
                empty = QLabel(msg)
                empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
                empty.setStyleSheet(
                    "color: #505870; font-size: 10px; padding: 16px 0;"
                )
                self._rec_layout.addWidget(empty)
            else:
                for entry in entries:
                    self._rec_layout.addWidget(PatternRow(entry))

            self._rec_layout.addStretch()

        # ------------------------------------------------------------------
        # 내부 업데이트
        # ------------------------------------------------------------------

        def _apply_tab_update(self):
            display_mode = self._current_mode or (BUTTON_MODES[0] if BUTTON_MODES else "4B")
            patterns = self._patterns_cache.get(display_mode, [])

            if self._tab_panel:
                self._tab_panel.update_patterns(patterns)
                self._tab_panel.set_active_diff(self._current_diff)

            self.adjustSize()

        # ------------------------------------------------------------------
        # 공개 API
        # ------------------------------------------------------------------

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

        # ------------------------------------------------------------------
        # 드래그 / 페인트
        # ------------------------------------------------------------------

        def mousePressEvent(self, event):
            if event.button() == Qt.MouseButton.LeftButton:
                self._dragging = True
                self._manual_position = True  # 드래그 시작 즉시 자동 위치 보정 중단
                self._drag_pos = (
                    event.globalPosition().toPoint() - self.frameGeometry().topLeft()
                )

        def mouseMoveEvent(self, event):
            if self._dragging:
                self.move(event.globalPosition().toPoint() - self._drag_pos)

        def mouseReleaseEvent(self, event):
            if self._dragging:
                self._dragging = False
                self._manual_position = True
                if self._user_move_cb is not None:
                    self._user_move_cb(self.x(), self.y())

        def paintEvent(self, event):
            painter = QPainter(self)
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            painter.setBrush(QBrush(QColor(0, 0, 0, 50)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(self.rect().adjusted(3, 4, -1, -1), 14, 14)

else:

    class OverlaySignals:
        pass

    class OverlayWindow:
        def __init__(self, *args, **kwargs):
            raise RuntimeError("PyQt6 is required for OverlayWindow")
