"""
PyQt6 투명 오버레이 창
- Always-on-top, 클릭 투과
- 선곡화면에서만 표시
- 현재 선택 곡의 버튼 모드별 난이도 표시
"""

import sys
import threading
from typing import Optional

try:
    from PyQt6.QtWidgets import (
        QApplication, QWidget, QLabel, QVBoxLayout, QHBoxLayout,
        QFrame, QGraphicsOpacityEffect
    )
    from PyQt6.QtCore import (
        Qt, QTimer, pyqtSignal, QObject, QPoint, QRect
    )
    from PyQt6.QtGui import (
        QColor, QPainter, QFont, QFontMetrics, QPen, QBrush,
        QLinearGradient, QKeySequence, QShortcut
    )
    PYQT_AVAILABLE = True
except ImportError:
    print("[Overlay] PyQt6 없음")
    PYQT_AVAILABLE = False

from varchive import VArchiveDB, BUTTON_MODES, DIFFICULTIES, DIFF_COLORS


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
        hint_label = QLabel("F9: 표시/숨김  |  드래그로 위치 이동")
        hint_label.setStyleSheet("color: rgba(255,255,255,60); font-size: 8px;")
        hint_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(hint_label)

        self.adjustSize()

    def _connect_signals(self):
        self.signals.song_changed.connect(self._on_song_changed)
        self.signals.screen_changed.connect(self._on_screen_changed)
        self.signals.position_changed.connect(self._on_game_window_moved)

        # F9: 표시/숨김 토글
        shortcut = QShortcut(QKeySequence("F9"), self)
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
        # 게임 창 오른쪽에 붙이기
        ox = left + width + 10
        oy = top + height - self.height() - 40
        # 화면 밖으로 나가면 게임 창 안쪽으로
        screen = QApplication.primaryScreen().geometry()
        if ox + self.width() > screen.width():
            ox = left - self.width() - 10
        self.move(ox, max(oy, top))

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

    def notify_song(self, title: str):
        """OCR 스레드에서 호출 - 곡명으로 패턴 조회 후 시그널 emit"""
        song = self.db.search(title)
        if not song:
            print(f"[Overlay] '{title}' DB에서 찾을 수 없음")
            return

        all_patterns = []
        for mode in BUTTON_MODES:
            patterns = self.db.format_pattern_info(song, mode)
            all_patterns.append({"mode": mode, "patterns": patterns})

        self.signals.song_changed.emit(song["name"], all_patterns)

    def notify_screen(self, is_song_select: bool):
        self.signals.screen_changed.emit(is_song_select)

    def notify_window_pos(self, left, top, width, height):
        self.signals.position_changed.emit(left, top, width, height)

    def run(self):
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
        self._app.exec()
