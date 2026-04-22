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
        QScrollArea,
        QApplication,
        QPushButton,
    )
    from PyQt6.QtCore import Qt, pyqtSignal, QObject, QPoint
    from PyQt6.QtGui import QPainter, QBrush, QColor
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

import win32gui
from data.varchive import BUTTON_MODES
from data.recommend import RecommendEntry
from overlay.ui.pattern_view import VerticalTabPanel
from overlay.ui.recommend_view import PatternRow
from constants import BTN_COLORS, WINDOW_TITLE
from settings import SETTINGS


if PYQT_AVAILABLE:

    class OverlaySignals(QObject):
        song_changed      = pyqtSignal(str, list)
        screen_changed    = pyqtSignal(bool)
        position_changed  = pyqtSignal(int, int, int, int)
        roi_enabled_changed = pyqtSignal(bool)
        mode_diff_changed = pyqtSignal(str, str)
        recommend_ready   = pyqtSignal(list, bool)
        visibility_toggle_requested = pyqtSignal()
        status_changed    = pyqtSignal(bool)
        confidence_changed = pyqtSignal(float)
        settings_requested = pyqtSignal()


    class OverlayWindow(QWidget):
        def __init__(self, signals: OverlaySignals):
            super().__init__()
            self.signals = signals
            self._current_mode: Optional[str] = None
            self._current_diff: Optional[str] = None
            self._patterns_cache: dict[str, list] = {}
            self._tab_panel: Optional[VerticalTabPanel] = None
            self._mode_label: Optional[QLabel] = None
            self._song_label: Optional[QLabel] = None
            self._dragging = False
            self._drag_pos = QPoint()
            self._manual_position = False
            self._user_move_cb = None
            self._last_confidence = 1.0  # 기본값

            self._setup_window()
            self._setup_ui()
            self._connect_signals()
            self._apply_opacity()

        def _setup_window(self):
            self.setWindowFlags(
                Qt.WindowType.FramelessWindowHint
                | Qt.WindowType.WindowStaysOnTopHint
                | Qt.WindowType.Tool
            )
            self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
            self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
            self.setFixedWidth(360)

            # 캡처 프로그램(mss 등)에서 오버레이를 캡처하지 않도록 설정
            hwnd = int(self.winId())
            try:
                ctypes.windll.user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
            except Exception as e:
                print(f"[Overlay] SetWindowDisplayAffinity 실패: {e}")

        def _setup_ui(self):
            root = QVBoxLayout(self)
            root.setContentsMargins(0, 0, 0, 0)
            root.setSpacing(0)

            panel = QFrame()
            panel.setStyleSheet("""
                QFrame {
                    background: rgb(18, 24, 38);
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
                    background: rgb(30, 40, 62);
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

            self._mode_label = QLabel("—")
            self._mode_label.setFixedSize(28, 22)
            self._mode_label.setStyleSheet(
                "color: #F0F4FF; background-color: #3D4D6A; font-size: 12px; font-weight: 900; border-radius: 3px;"
            )
            self._mode_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
            layout.addWidget(self._mode_label)

            self._song_label = QLabel("곡을 선택하세요")
            self._song_label.setStyleSheet(
                "color: #F0F4FF; font-size: 14px; font-weight: 700;"
            )
            self._song_label.setAlignment(Qt.AlignmentFlag.AlignVCenter)
            layout.addWidget(self._song_label, 1)

            self._settings_btn = QPushButton("⚙")
            self._settings_btn.setFixedSize(24, 24)
            self._settings_btn.setCursor(Qt.CursorShape.PointingHandCursor)
            self._settings_btn.setStyleSheet("""
                QPushButton {
                    color: #505870;
                    background: transparent;
                    border: none;
                    font-size: 16px;
                    font-weight: bold;
                }
                QPushButton:hover {
                    color: #F0F4FF;
                }
            """)
            self._settings_btn.clicked.connect(self.signals.settings_requested.emit)
            layout.addWidget(self._settings_btn)
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
            self.signals.status_changed.connect(self._on_status_changed)
            self.signals.confidence_changed.connect(self._on_confidence_changed)

        # ------------------------------------------------------------------
        # 슬롯
        # ------------------------------------------------------------------

        def _on_song_changed(self, title: str, all_patterns: list):
            self._song_label.setText(title)
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
            margin = 16
            
            # 1. 오른쪽 외부 시도 (하단 정렬)
            ox = left + width + margin
            oy = top + height - self.height() - margin
            
            # 오른쪽이 화면 밖이면 왼쪽 외부 시도
            if ox + self.width() > screen.width():
                ox = left - self.width() - margin
            
            # 왼쪽도 화면 밖이면 (창이 최대화되었거나 화면을 꽉 채운 경우) 내부 우측 하단에 배치
            if ox < 0 or ox + self.width() > screen.width() or ox < screen.x():
                ox = left + width - self.width() - margin
                oy = top + height - self.height() - margin
            
            # 최종 좌표 화면 범위 내로 보정 (최소한의 가시성 확보)
            ox = max(screen.x(), min(ox, screen.x() + screen.width() - self.width()))
            oy = max(screen.y(), min(oy, screen.y() + screen.height() - self.height()))
            
            self.move(ox, oy)

        def _on_mode_diff_changed(self, mode: str, diff: str):
            self._mode_label.setText(mode if mode else "—")
            mode_color = BTN_COLORS.get(mode, [(0x6A, 0x4D, 0x3D)])[0]
            mode_color = f"rgb({mode_color[2]}, {mode_color[1]}, {mode_color[0]})"
            self._mode_label.setStyleSheet(
                f"color: #F0F4FF; background-color: {mode_color}; font-size: 12px; font-weight: 900; border-radius: 3px;"
            )

            self._current_mode = mode or None
            self._current_diff = diff or None
            self._apply_tab_update()

        def _on_recommend_ready(
            self,
            entries: list[RecommendEntry],
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

        def _on_status_changed(self, is_stable: bool):
            if is_stable:
                self._status_lamp.setStyleSheet(
                    "background-color: #00D4FF; border-radius: 3px;"
                )
            else:
                self._status_lamp.setStyleSheet(
                    "background-color: #FF4B4B; border-radius: 3px;"
                )

        def _on_confidence_changed(self, confidence: float):
            """신뢰도(0.0~1.0)를 오버레이 불투명도로 매핑."""
            self._last_confidence = confidence
            self._apply_opacity()

        def update_base_opacity(self, base_opacity: float):
            """설정에서 기본 투명도가 변경되었을 때 즉시 반영."""
            self._apply_opacity()

        def _apply_opacity(self):
            """현재 신뢰도와 기본 투명도를 조합하여 최종 투명도 적용."""
            base_opacity = SETTINGS.get("overlay", {}).get("base_opacity", 1.0)
            
            # 신뢰도에 따른 감쇄 효과 (최소 0.3배 ~ 1.0배)
            # 신뢰도가 낮아도 완전히 사라지지는 않게 함
            MIN_SCALE = 0.3
            scale = MIN_SCALE + (1.0 - MIN_SCALE) * max(0.0, min(1.0, self._last_confidence))
            
            final_opacity = base_opacity * scale
            self.setWindowOpacity(final_opacity)

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
