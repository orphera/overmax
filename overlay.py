"""
PyQt6 투명 오버레이 창
- Always-on-top, 클릭 투과
- 선곡화면에서만 표시
- 현재 선택 곡의 버튼 모드별 난이도 표시
"""

import sys
import threading
import json
from typing import Optional
from settings import SETTINGS
import runtime_patch

try:
    from PyQt6.QtWidgets import (
        QApplication, QWidget, QLabel, QVBoxLayout, QHBoxLayout,
        QFrame, QGraphicsOpacityEffect, QSystemTrayIcon, QMenu, QStyle
    )
    from PyQt6.QtCore import (
        Qt, QTimer, pyqtSignal, QObject, QPoint, QRect
    )
    from PyQt6.QtGui import (
        QColor, QPainter, QFont, QFontMetrics, QPen, QBrush,
        QLinearGradient, QKeySequence, QShortcut, QIcon, QAction
    )
    PYQT_AVAILABLE = True
except ImportError:
    print("[Overlay] PyQt6 없음")
    PYQT_AVAILABLE = False

from varchive import VArchiveDB, BUTTON_MODES, DIFFICULTIES, DIFF_COLORS

OVERLAY_SETTINGS = SETTINGS.get("overlay", {})
TOGGLE_HOTKEY = str(OVERLAY_SETTINGS.get("toggle_hotkey", "F9"))
TRAY_TOOLTIP = str(OVERLAY_SETTINGS.get("tray_tooltip", "Overmax - DJMAX Respect V 난이도 오버레이"))
HINT_LABEL = str(OVERLAY_SETTINGS.get("hint_label", f"{TOGGLE_HOTKEY}: 표시/숨김  |  드래그로 위치 이동"))
OVERLAY_POSITION_FILE = str(OVERLAY_SETTINGS.get("position_file", "overlay_position.json"))
SCREEN_CAPTURE_SETTINGS = SETTINGS.get("screen_capture", {})

LIST_X_START = float(SCREEN_CAPTURE_SETTINGS.get("list_x_start", 0.031))
LIST_X_END = float(SCREEN_CAPTURE_SETTINGS.get("list_x_end", 0.167))
LOGO_X_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_start", 0.028))
LOGO_X_END = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_end", 0.210))
LOGO_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_start", 0.015))
LOGO_Y_END = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_end", 0.090))
LEFT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_start", 0.028))
LEFT_TITLE_X_END = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_end", 0.265))
LEFT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_start", 0.180))
LEFT_TITLE_Y_END = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_end", 0.305))
RIGHT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_start", 0.325))
RIGHT_TITLE_X_END = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_end", 0.660))
RIGHT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_start", 0.535))
RIGHT_TITLE_Y_END = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_end", 0.602))
LEFT_COMPOSER_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_start", 0.028))
LEFT_COMPOSER_X_END = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_end", 0.300))
LEFT_COMPOSER_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_start", 0.245))
LEFT_COMPOSER_Y_END = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_end", 0.285))


# ------------------------------------------------------------------
# 시그널 브릿지 (다른 스레드 → Qt 메인스레드)
# ------------------------------------------------------------------

class OverlaySignals(QObject):
    song_changed = pyqtSignal(str, list)   # (곡명, 패턴 정보 리스트)
    screen_changed = pyqtSignal(bool)      # 선곡화면 여부
    position_changed = pyqtSignal(int, int, int, int)  # 창 위치


# ------------------------------------------------------------------
# 난이도 카드 위젯
# ------------------------------------------------------------------

class DiffCard(QFrame):
    def __init__(self, diff: str, parent=None):
        super().__init__(parent)
        self.diff = diff
        self.color = QColor(DIFF_COLORS.get(diff, "#FFFFFF"))
        self._level = None
        self._floor_name = None

        self.setFixedSize(72, 64)
        self.setStyleSheet("background: transparent;")

    def set_info(self, level: Optional[int], floor_name: Optional[str]):
        self._level = level
        self._floor_name = floor_name
        self.update()

    def clear(self):
        self._level = None
        self._floor_name = None
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        if self._level is None:
            # 비활성 상태
            painter.setBrush(QBrush(QColor(60, 60, 60, 120)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(0, 0, self.width(), self.height(), 6, 6)
            return

        # 배경
        bg = QColor(self.color)
        bg.setAlpha(200)
        painter.setBrush(QBrush(bg))
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
            # 비공식 없으면 "-" 표시
            painter.setPen(QPen(QColor(200, 200, 200, 120)))
            font = QFont("Arial", 9)
            painter.setFont(font)
            painter.drawText(QRect(0, 44, self.width(), 16), Qt.AlignmentFlag.AlignHCenter, "-")


# ------------------------------------------------------------------
# 버튼 모드 패널
# ------------------------------------------------------------------

class ButtonModePanel(QFrame):
    def __init__(self, mode: str, parent=None):
        super().__init__(parent)
        self.mode = mode
        self._cards: dict[str, DiffCard] = {}

        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 6, 6, 6)
        layout.setSpacing(4)

        # 모드 라벨
        mode_label = QLabel(mode)
        mode_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        mode_label.setStyleSheet("color: #CCCCCC; font-size: 11px; font-weight: bold;")
        layout.addWidget(mode_label)

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
                background: rgba(20, 20, 30, 160);
                border: 1px solid rgba(255,255,255,30);
                border-radius: 8px;
            }
        """)

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


class RoiOverlayWindow(QWidget):
    """게임 화면 위에 OCR/검출 ROI를 선으로 표시하는 디버그 오버레이"""
    def __init__(self):
        super().__init__()
        self._enabled = False
        self._has_rect = False
        self._setup_window()

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    def set_game_rect(self, left: int, top: int, width: int, height: int):
        self._has_rect = width > 0 and height > 0
        if not self._has_rect:
            self.hide()
            return
        self.setGeometry(left, top, width, height)
        if self._enabled:
            self.show()
        self.update()

    def set_enabled(self, enabled: bool):
        self._enabled = enabled
        if enabled and self._has_rect:
            self.show()
            self.raise_()
        else:
            self.hide()
        self.update()

    def is_enabled(self) -> bool:
        return self._enabled

    def _ratio_rect(self, rx1: float, ry1: float, rx2: float, ry2: float) -> QRect:
        x = int(self.width() * rx1)
        y = int(self.height() * ry1)
        w = max(1, int(self.width() * (rx2 - rx1)))
        h = max(1, int(self.height() * (ry2 - ry1)))
        return QRect(x, y, w, h)

    def _draw_box(self, painter: QPainter, rect: QRect, color: QColor, label: str):
        painter.setPen(QPen(color, 2))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRect(rect)
        painter.setPen(QPen(color, 1))
        painter.setFont(QFont("Consolas", 9, QFont.Weight.Bold))
        painter.drawText(rect.left() + 4, max(12, rect.top() - 4), label)

    def paintEvent(self, event):
        if not self._enabled or not self._has_rect:
            return
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        self._draw_box(
            painter,
            self._ratio_rect(LOGO_X_START, LOGO_Y_START, LOGO_X_END, LOGO_Y_END),
            QColor("#CC66FF"),
            "LOGO (FREESTYLE)",
        )
        self._draw_box(
            painter,
            self._ratio_rect(LIST_X_START, 0.0, LIST_X_END, 1.0),
            QColor("#22D3EE"),
            "LIST SCAN",
        )
        self._draw_box(
            painter,
            self._ratio_rect(LEFT_TITLE_X_START, LEFT_TITLE_Y_START, LEFT_TITLE_X_END, LEFT_TITLE_Y_END),
            QColor("#FF4D4D"),
            "LEFT TITLE OCR",
        )
        self._draw_box(
            painter,
            self._ratio_rect(RIGHT_TITLE_X_START, RIGHT_TITLE_Y_START, RIGHT_TITLE_X_END, RIGHT_TITLE_Y_END),
            QColor("#4D7CFF"),
            "RIGHT TITLE OCR",
        )
        self._draw_box(
            painter,
            self._ratio_rect(LEFT_COMPOSER_X_START, LEFT_COMPOSER_Y_START, LEFT_COMPOSER_X_END, LEFT_COMPOSER_Y_END),
            QColor("#FFD84D"),
            "COMPOSER OCR",
        )


# ------------------------------------------------------------------
# 메인 오버레이 창
# ------------------------------------------------------------------

class OverlayWindow(QWidget):
    def __init__(self, db: VArchiveDB, signals: OverlaySignals):
        super().__init__()
        self.db = db
        self.signals = signals
        self._current_mode = "4B"  # 기본 버튼 모드
        self._panels: dict[str, ButtonModePanel] = {}
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
            | Qt.WindowType.Tool  # 작업표시줄에 안 나타남
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setMinimumWidth(320)

    def _setup_ui(self):
        main_layout = QVBoxLayout(self)
        main_layout.setContentsMargins(8, 8, 8, 8)
        main_layout.setSpacing(6)

        # 헤더 (곡명 + 드래그 핸들)
        header = QFrame()
        header.setStyleSheet("""
            QFrame {
                background: rgba(15, 15, 25, 180);
                border-radius: 8px;
            }
        """)
        header_layout = QHBoxLayout(header)
        header_layout.setContentsMargins(10, 6, 10, 6)

        badge = QLabel("V-Archive")
        badge.setStyleSheet("color: #7B68EE; font-size: 10px; font-weight: bold;")
        header_layout.addWidget(badge)

        self._song_label = QLabel("곡을 선택하세요")
        self._song_label.setStyleSheet("color: #FFFFFF; font-size: 13px; font-weight: bold;")
        self._song_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        header_layout.addWidget(self._song_label, 1)

        hint = QLabel("드래그")
        hint.setStyleSheet("color: #555555; font-size: 9px;")
        header_layout.addWidget(hint)

        main_layout.addWidget(header)

        # 버튼 모드 패널들
        for mode in BUTTON_MODES:
            panel = ButtonModePanel(mode)
            self._panels[mode] = panel
            main_layout.addWidget(panel)

        # 단축키 힌트
        hint_label = QLabel(HINT_LABEL)
        hint_label.setStyleSheet("color: rgba(255,255,255,60); font-size: 8px;")
        hint_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(hint_label)

        self.adjustSize()

    def _connect_signals(self):
        self.signals.song_changed.connect(self._on_song_changed)
        self.signals.screen_changed.connect(self._on_screen_changed)
        self.signals.position_changed.connect(self._on_game_window_moved)

        # 표시/숨김 단축키 토글
        shortcut = QShortcut(QKeySequence(TOGGLE_HOTKEY), self)
        shortcut.activated.connect(self.toggle_visibility)

    # ------------------------------------------------------------------
    # 슬롯
    # ------------------------------------------------------------------

    def _on_song_changed(self, title: str, all_patterns: list):
        """
        all_patterns: 모든 버튼 모드의 패턴 정보
        형식: [{"mode": "4B", "patterns": [...]}, ...]
        """
        self._song_label.setText(title)
        for item in all_patterns:
            mode = item["mode"]
            if mode in self._panels:
                self._panels[mode].update_patterns(item["patterns"])

    def _on_screen_changed(self, is_song_select: bool):
        if is_song_select:
            self.show()
        else:
            self.hide()

    def _on_game_window_moved(self, left, top, width, height):
        """게임 창 위치 변경 시 오버레이도 이동 (기본 위치: 게임 창 우측 하단)"""
        if self._manual_position:
            return
        # 게임 창 오른쪽에 붙이기
        ox = left + width + 10
        oy = top + height - self.height() - 40
        # 화면 밖으로 나가면 게임 창 안쪽으로
        screen = QApplication.primaryScreen().geometry()
        if ox + self.width() > screen.width():
            ox = left - self.width() - 10
        self.move(ox, max(oy, top))

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
    # 드래그로 위치 이동
    # ------------------------------------------------------------------

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
        else:
            self._dragging = False

    # ------------------------------------------------------------------
    # 배경 그리기
    # ------------------------------------------------------------------

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QBrush(QColor(0, 0, 0, 0)))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRect(self.rect())


# ------------------------------------------------------------------
# 오버레이 컨트롤러 (스레드 → Qt 브릿지)
# ------------------------------------------------------------------

class OverlayController:
    def __init__(self, db: VArchiveDB):
        self.db = db
        self.signals = OverlaySignals()
        self._app: Optional[QApplication] = None
        self._window: Optional[OverlayWindow] = None
        self._roi_window: Optional[RoiOverlayWindow] = None
        self._tray_icon: Optional[QSystemTrayIcon] = None
        self._debug_log_cb = None   # set by main.py after DebugController init
        self._debug_toggle_cb = None
        self._last_window_rect: Optional[tuple[int, int, int, int]] = None
        self._position_path = runtime_patch.get_data_dir() / OVERLAY_POSITION_FILE

    def notify_song(self, title: str, composer: str = ""):
        """OCR 스레드에서 호출 - 곡명/작곡가로 패턴 조회 후 시그널 emit"""
        if composer:
            self.log(f"곡 검색: '{title}' / composer='{composer}'")
        else:
            self.log(f"곡 검색: '{title}'")
        song = self.db.search(title, composer=composer)
        if not song:
            self.log(f"'{title}' (composer='{composer}') DB에서 찾을 수 없음")
            return

        all_patterns = []
        for mode in BUTTON_MODES:
            patterns = self.db.format_pattern_info(song, mode)
            all_patterns.append({"mode": mode, "patterns": patterns})

        self.signals.song_changed.emit(song["name"], all_patterns)

    def notify_screen(self, is_song_select: bool):
        self.log(f"화면 알림: {'선곡화면' if is_song_select else '기타화면'}")
        self.signals.screen_changed.emit(is_song_select)

    def notify_window_pos(self, left, top, width, height):
        self.log(f"창 위치: ({left},{top}) {width}x{height}")
        self._last_window_rect = (left, top, width, height)
        self.signals.position_changed.emit(left, top, width, height)

    def set_roi_overlay_enabled(self, enabled: bool):
        if self._roi_window is None:
            return
        self._roi_window.set_enabled(enabled)
        state = "ON" if enabled else "OFF"
        self.log(f"ROI 영역 표시: {state}")
        if enabled and self._last_window_rect is not None:
            left, top, width, height = self._last_window_rect
            self._roi_window.set_game_rect(left, top, width, height)

    def toggle_roi_overlay(self):
        if self._roi_window is None:
            return False
        new_state = not self._roi_window.is_enabled()
        self.set_roi_overlay_enabled(new_state)
        return new_state

    def log(self, msg: str):
        full = f"[Overlay] {msg}"
        print(full)
        if self._debug_log_cb:
            self._debug_log_cb(full)

    def _load_overlay_position(self) -> Optional[tuple[int, int]]:
        try:
            if not self._position_path.exists():
                return None
            with open(self._position_path, encoding="utf-8") as f:
                data = json.load(f)
            x = int(data.get("x"))
            y = int(data.get("y"))
            return (x, y)
        except Exception as e:
            self.log(f"오버레이 위치 로드 실패: {e}")
            return None

    def _save_overlay_position(self, x: int, y: int):
        try:
            self._position_path.parent.mkdir(parents=True, exist_ok=True)
            with open(self._position_path, "w", encoding="utf-8") as f:
                json.dump({"x": int(x), "y": int(y)}, f, ensure_ascii=False, indent=2)
        except Exception as e:
            self.log(f"오버레이 위치 저장 실패: {e}")

    def _on_overlay_user_moved(self, x: int, y: int):
        self._save_overlay_position(x, y)
        self.log(f"오버레이 위치 저장: ({x},{y})")

    def run(self, debug_ctrl=None):
        """Qt 이벤트 루프 실행 (메인 스레드에서 호출)"""
        if not PYQT_AVAILABLE:
            print("[Overlay] PyQt6 없음, 콘솔 모드로 실행")
            import time
            while True:
                time.sleep(1)
            return

        self._app = QApplication(sys.argv)
        self._app.setQuitOnLastWindowClosed(False)
        self._window = OverlayWindow(self.db, self.signals)
        self._window.hide()  # 처음엔 숨김
        self._window.set_user_move_callback(self._on_overlay_user_moved)
        self._roi_window = RoiOverlayWindow()
        self._roi_window.hide()  # 기본 OFF
        self.signals.position_changed.connect(self._roi_window.set_game_rect)

        saved_pos = self._load_overlay_position()
        if saved_pos is not None:
            sx, sy = saved_pos
            screen = self._app.primaryScreen().geometry()
            sx = max(0, min(sx, max(0, screen.width() - self._window.width())))
            sy = max(0, min(sy, max(0, screen.height() - self._window.height())))
            self._window.apply_saved_position(sx, sy)
            self.log(f"오버레이 위치 복원: ({sx},{sy})")

        # 디버그 창 생성 (QApplication 생성 후)
        if debug_ctrl is not None:
            debug_ctrl.create_window()
            debug_ctrl.set_roi_toggle_callback(self.set_roi_overlay_enabled)
            self._debug_toggle_cb = debug_ctrl.toggle_window
        else:
            self._debug_toggle_cb = None

        # 트레이 아이콘 설정
        self._setup_tray_icon()

        self._app.exec()

    def _setup_tray_icon(self):
        """트레이 아이콘 설정"""
        if not QSystemTrayIcon.isSystemTrayAvailable():
            print("[Overlay] 시스템 트레이를 사용할 수 없음")
            return

        # 트레이 아이콘 생성 (아이콘은 기본 Qt 아이콘 사용)
        self._tray_icon = QSystemTrayIcon(self._app)
        self._tray_icon.setIcon(self._app.style().standardIcon(QStyle.StandardPixmap.SP_ComputerIcon))  # 임시 아이콘
        self._tray_icon.setToolTip(TRAY_TOOLTIP)

        # 트레이 메뉴 생성
        tray_menu = QMenu()

        # 오버레이 표시/숨김 액션
        toggle_action = QAction(f"오버레이 표시/숨김 ({TOGGLE_HOTKEY})", self._app)
        toggle_action.triggered.connect(self._window.toggle_visibility)
        tray_menu.addAction(toggle_action)

        # 디버그 창 표시/숨김 액션
        if self._debug_toggle_cb is not None:
            debug_action = QAction("디버그 창 표시/숨김", self._app)
            debug_action.triggered.connect(self._debug_toggle_cb)
            tray_menu.addAction(debug_action)

        tray_menu.addSeparator()

        # 종료 액션
        quit_action = QAction("종료", self._app)
        quit_action.triggered.connect(self._app.quit)
        tray_menu.addAction(quit_action)

        self._tray_icon.setContextMenu(tray_menu)
        self._tray_icon.show()

        # 트레이 아이콘 더블클릭으로 오버레이 토글
        self._tray_icon.activated.connect(self._on_tray_activated)

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self._window.toggle_visibility()
