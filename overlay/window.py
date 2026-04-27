"""PyQt6 overlay window and signal bridge."""

from typing import Optional
import ctypes

WDA_EXCLUDEFROMCAPTURE = 0x00000011

try:
    from PyQt6.QtWidgets import (
        QWidget,
        QLabel,
        QVBoxLayout,
        QHBoxLayout,
        QFrame,
        QApplication,
    )
    from PyQt6.QtCore import Qt, pyqtSignal, QObject, QPoint
    from PyQt6.QtGui import QPainter, QBrush, QColor
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

import win32gui
from data.varchive import BUTTON_MODES
from data.recommend import RecommendResult
from overlay.ui.pattern_view import VerticalTabPanel
from overlay.ui.recommend_view import PatternRow
from overlay.ui.header_widget import HeaderWidget
from overlay.ui.footer_widget import FooterWidget
from overlay.utils import calculate_overlay_position
from constants import WINDOW_TITLE
from settings import SETTINGS


def _s(base: int, scale: float) -> int:
    return max(1, round(base * scale))


if PYQT_AVAILABLE:

    class OverlaySignals(QObject):
        song_changed      = pyqtSignal(str, list)
        screen_changed    = pyqtSignal(bool)
        position_changed  = pyqtSignal(int, int, int, int)
        roi_enabled_changed = pyqtSignal(bool)
        mode_diff_changed = pyqtSignal(str, str)
        recommend_ready   = pyqtSignal(RecommendResult, bool)
        visibility_toggle_requested = pyqtSignal()
        status_changed    = pyqtSignal(bool)
        confidence_changed = pyqtSignal(float)
        settings_requested = pyqtSignal()
        scale_changed     = pyqtSignal(float)


    class OverlayWindow(QWidget):
        def __init__(self, signals: OverlaySignals):
            super().__init__()
            self.signals = signals
            self._current_mode: Optional[str] = None
            self._current_diff: Optional[str] = None
            self._patterns_cache: dict[str, list] = {}
            self._tab_panel: Optional[VerticalTabPanel] = None
            
            self._header: Optional[HeaderWidget] = None
            self._footer: Optional[FooterWidget] = None
            
            self._dragging = False
            self._drag_pos = QPoint()
            self._manual_position = False
            self._user_move_cb = None
            self._last_confidence = 1.0
            self._scale = float(SETTINGS.get("overlay", {}).get("scale", 1.0))

            self._setup_window()
            self._build_ui()
            self._connect_signals()
            self._apply_opacity()

        # ------------------------------------------------------------------
        # 창 속성
        # ------------------------------------------------------------------

        def _setup_window(self):
            self.setWindowFlags(
                Qt.WindowType.FramelessWindowHint
                | Qt.WindowType.WindowStaysOnTopHint
                | Qt.WindowType.Tool
            )
            self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
            self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
            self._apply_fixed_width()

            hwnd = int(self.winId())
            try:
                ctypes.windll.user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
            except Exception as e:
                print(f"[Overlay] SetWindowDisplayAffinity 실패: {e}")

        def _apply_fixed_width(self):
            self.setFixedWidth(_s(360, self._scale))

        # ------------------------------------------------------------------
        # UI 빌드 / 리빌드
        # ------------------------------------------------------------------

        def _build_ui(self):
            sc = self._scale

            old_layout = self.layout()
            if old_layout is not None:
                while old_layout.count():
                    item = old_layout.takeAt(0)
                    if item.widget():
                        item.widget().deleteLater()
                QWidget().setLayout(old_layout)

            root = QVBoxLayout(self)
            root.setContentsMargins(0, 0, 0, 0)
            root.setSpacing(0)

            panel = QFrame()
            panel.setStyleSheet(f"""
                QFrame {{
                    background: rgb(18, 24, 38);
                    border-radius: {_s(14, sc)}px;
                }}
            """)

            panel_layout = QVBoxLayout(panel)
            panel_layout.setContentsMargins(_s(8, sc), _s(8, sc), _s(8, sc), _s(8, sc))
            panel_layout.setSpacing(_s(6, sc))
            
            self._header = HeaderWidget(scale=sc)
            self._header.settings_requested.connect(self.signals.settings_requested.emit)
            panel_layout.addWidget(self._header)
            
            panel_layout.addWidget(self._build_body())
            
            self._footer = FooterWidget(scale=sc)
            panel_layout.addWidget(self._footer)

            root.addWidget(panel)
            self.adjustSize()

        def rebuild_ui(self, scale: float):
            self._scale = scale
            self._apply_fixed_width()
            self._build_ui()
            self._apply_tab_update()
            self._apply_opacity()

        def _build_body(self) -> QFrame:
            sc = self._scale
            body = QFrame()
            body.setStyleSheet("background: transparent;")
            layout = QHBoxLayout(body)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.setSpacing(_s(6, sc))

            self._tab_panel = VerticalTabPanel(scale=sc)
            layout.addWidget(self._tab_panel)
            layout.addWidget(self._build_recommend_panel(), 1)
            return body

        def _build_recommend_panel(self) -> QWidget:
            sc = self._scale
            wrapper = QWidget()
            wrapper.setStyleSheet("background: transparent;")
            layout = QVBoxLayout(wrapper)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.setSpacing(_s(4, sc))

            self._rec_widget = QWidget()
            self._rec_widget.setStyleSheet("background: transparent;")
            self._rec_layout = QVBoxLayout(self._rec_widget)
            self._rec_layout.setContentsMargins(0, _s(8, sc), 0, _s(8, sc))
            self._rec_layout.setSpacing(_s(3, sc))
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
            self.signals.status_changed.connect(self._on_status_changed)
            self.signals.confidence_changed.connect(self._on_confidence_changed)
            self.signals.scale_changed.connect(self._on_scale_changed)

        # ------------------------------------------------------------------
        # 슬롯
        # ------------------------------------------------------------------

        def _on_song_changed(self, title: str, all_patterns: list):
            if self._header:
                self._header.update_song(title)
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

            screen = QApplication.primaryScreen().geometry()
            margin = _s(16, self._scale)

            target_x = left + width + margin
            target_y = top + height - self.height() - margin

            ox, oy = calculate_overlay_position(
                target_x, target_y, 
                self.width(), self.height(),
                screen.x(), screen.y(), screen.width(), screen.height()
            )

            self.move(ox, oy)

        def _on_mode_diff_changed(self, mode: str, diff: str):
            if self._header:
                self._header.update_mode(mode, diff)
            self._current_mode = mode or None
            self._current_diff = diff or None
            self._apply_tab_update()

        def _on_recommend_ready(self, recommendations: RecommendResult, no_selection: bool):
            while self._rec_layout.count() > 0:
                item = self._rec_layout.takeAt(0)
                if item and item.widget():
                    item.widget().deleteLater()

            if no_selection or not recommendations.entries:
                msg = "패턴을 감지하는 중..." if no_selection else "추천 결과 없음"
                empty = QLabel(msg)
                empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
                empty.setStyleSheet(
                    f"color: #505870; font-size: {_s(10, self._scale)}px; padding: {_s(16, self._scale)}px 0;"
                )
                self._rec_layout.addWidget(empty)
            else:
                for entry in recommendations.entries:
                    self._rec_layout.addWidget(PatternRow(entry, scale=self._scale))

            self._rec_layout.addStretch()
            if self._footer:
                self._footer.update_footer(recommendations.avg_rate, recommendations.has_record_count, recommendations.total_count)

        def _on_status_changed(self, is_stable: bool):
            if self._header:
                self._header.update_status(is_stable)

        def _on_confidence_changed(self, confidence: float):
            self._last_confidence = confidence
            self._apply_opacity()

        def _on_scale_changed(self, scale: float):
            was_visible = self.isVisible()
            self.hide()
            self.rebuild_ui(scale)
            if was_visible:
                self.show()

        def update_base_opacity(self, base_opacity: float):
            self._apply_opacity()

        def _apply_opacity(self):
            base_opacity = SETTINGS.get("overlay", {}).get("base_opacity", 1.0)
            MIN_SCALE = 0.3
            scale = MIN_SCALE + (1.0 - MIN_SCALE) * max(0.0, min(1.0, self._last_confidence))
            self.setWindowOpacity(base_opacity * scale)

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
                self._manual_position = True
                self._drag_pos = (
                    event.globalPosition().toPoint() - self.frameGeometry().topLeft()
                )
                self.activateWindow()
                self.raise_()

        def mouseMoveEvent(self, event):
            if self._dragging:
                self.move(event.globalPosition().toPoint() - self._drag_pos)

        def mouseReleaseEvent(self, event):
            if self._dragging:
                self._dragging = False
                self._manual_position = True
                if self._user_move_cb is not None:
                    self._user_move_cb(self.x(), self.y())
                self._restore_game_focus()

        def paintEvent(self, event):
            painter = QPainter(self)
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            painter.setBrush(QBrush(QColor(0, 0, 0)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(self.rect().adjusted(3, 4, -1, -1), 14, 14)
            
        # ------------------------------------------------------------------
        # 게임 포커스 복원
        # ------------------------------------------------------------------

        def _restore_game_focus(self):
            hwnd = win32gui.FindWindow(None, WINDOW_TITLE)
            if hwnd:
                ctypes.windll.user32.SetForegroundWindow(hwnd)

else:

    class OverlaySignals:
        pass

    class OverlayWindow:
        def __init__(self, *args, **kwargs):
            raise RuntimeError("PyQt6 is required for OverlayWindow")
