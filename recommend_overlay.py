"""
recommend_overlay.py - 유사 난이도 패턴 추천 오버레이 창

F2 단축키로 토글. 현재 선택된 패턴 기준으로
비슷한 난이도의 패턴 목록을 표시한다.

- 선택된 패턴 없음 → "선택된 패턴이 없습니다" 표시
- 기록 있는 패턴은 Rate 표시, rate 낮은 순(약한 패턴) 우선
- 기록이 업데이트되면 목록 자동 갱신
"""

from __future__ import annotations

from typing import Optional

try:
    from PyQt6.QtWidgets import (
        QWidget, QVBoxLayout, QHBoxLayout, QLabel,
        QScrollArea, QFrame,
    )
    from PyQt6.QtCore import Qt, QObject, pyqtSignal, QPoint
    from PyQt6.QtGui import QColor, QPainter, QPen, QBrush
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

from recommend import Recommender, RecommendEntry
from varchive import VArchiveDB
from record_db import RecordDB


# ------------------------------------------------------------------
# 시그널 브릿지
# ------------------------------------------------------------------

class RecommendSignals(QObject):
    toggle_requested = pyqtSignal()
    data_ready = pyqtSignal(list, str, bool)


# ------------------------------------------------------------------
# 개별 패턴 행 위젯
# ------------------------------------------------------------------

class PatternRow(QFrame):
    def __init__(self, entry: RecommendEntry, parent=None):
        super().__init__(parent)
        self.entry = entry
        self._setup_ui()

    def _setup_ui(self):
        try:
            self.setFixedHeight(44)
            self.setStyleSheet("background: rgba(25, 25, 40, 180); border: 1px solid rgba(255,255,255,20); border-radius: 5px;")

            layout = QHBoxLayout(self)
            layout.setContentsMargins(10, 4, 10, 4)
            layout.setSpacing(8)

            e = self.entry

            # 난이도 뱃지
            badge = QLabel(f"{e.button_mode}\n{e.difficulty}")
            badge.setFixedWidth(44)
            badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
            badge.setStyleSheet(f"background: {e.color}; color: white; font-size: 9px; font-weight: bold; border-radius: 3px; padding: 2px;")
            layout.addWidget(badge)

            # floor 표시
            floor_str = e.floor_name if e.floor_name else (f"Lv.{e.level}" if e.level else "?")
            floor_label = QLabel(floor_str)
            floor_label.setFixedWidth(36)
            floor_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
            floor_label.setStyleSheet("color: #FFD6A5; font-size: 11px; font-weight: bold;")
            layout.addWidget(floor_label)

            # 구분선
            sep = QFrame()
            sep.setFrameShape(QFrame.Shape.VLine)
            sep.setStyleSheet("color: rgba(255,255,255,30);")
            layout.addWidget(sep)

            # 곡명 + 작곡가
            name_col = QVBoxLayout()
            name_col.setSpacing(1)
            name_col.setContentsMargins(0, 0, 0, 0)

            song_label = QLabel(e.song_name)
            song_label.setStyleSheet("color: #FFFFFF; font-size: 11px; font-weight: bold;")
            song_label.setMaximumWidth(210)
            try:
                elided = song_label.fontMetrics().elidedText(
                    e.song_name, Qt.TextElideMode.ElideRight, 210
                )
                song_label.setText(elided)
            except Exception:
                song_label.setText(e.song_name[:20] + "..." if len(e.song_name) > 20 else e.song_name)
            name_col.addWidget(song_label)

            comp_label = QLabel(e.composer)
            comp_label.setStyleSheet("color: #777777; font-size: 9px;")
            comp_label.setMaximumWidth(210)
            try:
                comp_elided = comp_label.fontMetrics().elidedText(
                    e.composer, Qt.TextElideMode.ElideRight, 210
                )
                comp_label.setText(comp_elided)
            except Exception:
                comp_label.setText(e.composer[:20] + "..." if len(e.composer) > 20 else e.composer)
            name_col.addWidget(comp_label)

            layout.addLayout(name_col)
            layout.addStretch()

            # Rate
            if e.is_played:
                rate_label = QLabel(f"{e.rate:.2f}%")
                rate_label.setFixedWidth(52)
                rate_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
                rate_label.setStyleSheet(
                    f"color: {self._rate_color(e.rate)}; font-size: 11px; font-weight: bold;"
                )
                layout.addWidget(rate_label)
            else:
                dash = QLabel("—")
                dash.setFixedWidth(52)
                dash.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
                dash.setStyleSheet("color: #444444; font-size: 11px;")
                layout.addWidget(dash)
        except Exception as ex:
            print(f"[PatternRow] _setup_ui 오류: {ex}")
            # 오류 시 최소 UI
            layout = QHBoxLayout(self)
            error_label = QLabel("UI 오류")
            layout.addWidget(error_label)

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


# ------------------------------------------------------------------
# 추천 오버레이 창
# ------------------------------------------------------------------

class RecommendOverlay(QWidget):
    def __init__(self, signals: RecommendSignals, parent=None):
        super().__init__(parent)
        self.signals     = signals
        self._entries:   list[RecommendEntry] = []
        self._pivot_str: str  = ""
        self._no_selection = True
        self._dragging  = False
        self._drag_pos  = QPoint()
        self._user_move_cb = None
        self._manual_position = False

        self._setup_window()
        self._setup_ui()
        self.signals.toggle_requested.connect(self.toggle)
        self.signals.data_ready.connect(self._on_data_ready)

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setFixedWidth(420)

    def _setup_ui(self):
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        self._container = QFrame()
        self._container.setStyleSheet("background: rgba(10, 10, 20, 215); border: 1px solid rgba(150, 150, 255, 60); border-radius: 10px;")
        outer.addWidget(self._container)

        inner = QVBoxLayout(self._container)
        inner.setContentsMargins(10, 10, 10, 10)
        inner.setSpacing(6)

        # 헤더
        header = QHBoxLayout()
        title = QLabel("유사 난이도 추천")
        title.setStyleSheet("color: #7B68EE; font-size: 11px; font-weight: bold;")
        header.addWidget(title)
        header.addStretch()
        hint = QLabel("F2: 닫기  |  드래그")
        hint.setStyleSheet("color: #444444; font-size: 9px;")
        header.addWidget(hint)
        inner.addLayout(header)

        # 기준 패턴 라벨
        self._pivot_label = QLabel("선택된 패턴이 없습니다")
        self._pivot_label.setStyleSheet("color: #AAAAAA; font-size: 10px; padding: 1px 0;")
        inner.addWidget(self._pivot_label)

        # 구분선
        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet("color: rgba(255,255,255,25);")
        inner.addWidget(line)

        # 스크롤 영역 설정
        self._scroll = QScrollArea()
        self._scroll.setWidgetResizable(True)
        self._scroll.setFixedHeight(430)
        self._scroll.setFrameShape(QFrame.Shape.NoFrame)
        self._scroll.setStyleSheet("""
            QScrollArea { background: transparent; }
            QScrollBar:vertical {
                background: transparent;
                width: 5px;
                margin: 0px;
            }
            QScrollBar::handle:vertical {
                background: rgba(123, 104, 238, 100);
                min-height: 20px;
                border-radius: 2px;
            }
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
                height: 0px;
            }
        """)

        self._list_widget = QWidget()
        self._list_widget.setStyleSheet("background: transparent;")
        self._list_layout = QVBoxLayout(self._list_widget)
        self._list_layout.setContentsMargins(0, 0, 5, 0)
        self._list_layout.setSpacing(4)
        
        self._scroll.setWidget(self._list_widget)
        inner.addWidget(self._scroll)

        # 하단 카운트
        self._count_label = QLabel("")
        self._count_label.setAlignment(Qt.AlignmentFlag.AlignRight)
        self._count_label.setStyleSheet("color: #555555; font-size: 9px;")
        inner.addWidget(self._count_label)

    # ------------------------------------------------------------------
    # 외부 인터페이스
    # ------------------------------------------------------------------

    def _on_data_ready(self, entries: list[RecommendEntry], pivot_str: str, no_selection: bool):
        self._entries = entries
        self._pivot_str = pivot_str
        self._no_selection = no_selection
        self._rebuild_list()

    # ------------------------------------------------------------------
    # 목록 재빌드 (Qt 메인스레드)
    # ------------------------------------------------------------------

    def _rebuild_list(self):
        try:
            # 레이아웃 내의 모든 위젯과 스페이스 제거
            while self._list_layout.count() > 0:
                item = self._list_layout.takeAt(0)
                if item and item.widget():
                    item.widget().deleteLater()

            if self._no_selection:
                self._pivot_label.setText("선택된 패턴이 없습니다")
                self._count_label.setText("")
                # 선택된 패턴 없을 때도 레이아웃 균형을 위해 stretch 추가
                self._list_layout.addStretch()
                return

            self._pivot_label.setText(f"기준: {self._pivot_str}")

            if not self._entries:
                empty = QLabel("유사한 패턴이 없습니다")
                empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
                empty.setStyleSheet("color: #555555; font-size: 11px; padding: 20px;")
                self._list_layout.addWidget(empty)
                self._list_layout.addStretch()
                self._count_label.setText("")
                return

            for entry in self._entries:
                row = PatternRow(entry)
                self._list_layout.addWidget(row)

            # 아래쪽에 여백을 추가하여 목록을 위로 밀어올림
            self._list_layout.addStretch()

            played = sum(1 for e in self._entries if e.is_played)
            self._count_label.setText(f"총 {len(self._entries)}개  |  기록 있음 {played}개")
        except Exception as e:
            print(f"[RecommendOverlay] _rebuild_list 오류: {e}")
            # 오류 시 기본 상태로 복구
            self._pivot_label.setText("오류 발생")
            self._count_label.setText("")

    # ------------------------------------------------------------------
    # 드래그
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
            if self._user_move_cb:
                self._user_move_cb(self.x(), self.y())
        else:
            self._dragging = False

    def set_user_move_callback(self, cb):
        self._user_move_cb = cb

    def apply_saved_position(self, x, y):
        self._manual_position = True
        self.move(x, y)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QBrush(QColor(0, 0, 0, 0)))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRect(self.rect())

    def toggle(self):
        if self.isVisible():
            self.hide()
        else:
            self.show()
            self.raise_()


# ------------------------------------------------------------------
# 컨트롤러
# ------------------------------------------------------------------

class RecommendController:
    """
    song / mode / diff 변경 → Recommender 호출 → RecommendOverlay 갱신.
    notify_record_updated() 호출 시 현재 목록을 rate 포함해 재조회.
    """

    def __init__(self, varchive_db: VArchiveDB, record_db: RecordDB):
        self.recommender   = Recommender(varchive_db, record_db)
        self.signals       = RecommendSignals()
        self._window: Optional[RecommendOverlay] = None

        self._song_id:     Optional[int] = None
        self._button_mode: Optional[str] = None
        self._difficulty:  Optional[str] = None
        
        self._pos_file = runtime_patch.get_data_dir() / "cache/recommend_position.json"

    def create_window(self) -> Optional[RecommendOverlay]:
        try:
            if not PYQT_AVAILABLE:
                return None
            if self._window is None:
                self._window = RecommendOverlay(self.signals)
                self._window.set_user_move_callback(self._save_position)
                self._load_position()
                self._window.hide()
            return self._window
        except Exception as e:
            print(f"[RecommendController] create_window 오류: {e}")
            return None

    def _save_position(self, x, y):
        try:
            import json
            self._pos_file.parent.mkdir(parents=True, exist_ok=True)
            with open(self._pos_file, "w", encoding="utf-8") as f:
                json.dump({"x": x, "y": y}, f)
        except Exception as e:
            print(f"[RecommendController] _save_position 오류: {e}")

    def _load_position(self):
        if self._window and self._pos_file.exists():
            try:
                import json
                with open(self._pos_file, "r", encoding="utf-8") as f:
                    data = json.load(f)
                self._window.apply_saved_position(data["x"], data["y"])
            except Exception as e:
                print(f"[RecommendController] _load_position 오류: {e}")

    def toggle(self):
        # 백그라운드 스레드(Hotkey)에서 안전하게 호출하기 위해 시그널 사용
        self.signals.toggle_requested.emit()

    # ------------------------------------------------------------------
    # 상태 업데이트 (ScreenCapture 콜백에서 호출)
    # ------------------------------------------------------------------

    def notify_song(self, song_id: int):
        if self._song_id != song_id:
            self._song_id = song_id
            self._refresh()

    def notify_mode_diff(self, button_mode: str, difficulty: str):
        if self._button_mode != button_mode or self._difficulty != difficulty:
            self._button_mode = button_mode
            self._difficulty  = difficulty
            self._refresh()

    def notify_record_updated(self):
        """새 기록이 저장된 후 호출 → rate 포함 재조회."""
        self._refresh()

    def _refresh(self):
        if self._window is None:
            return

        if not self._song_id or not self._button_mode or not self._difficulty:
            self.signals.data_ready.emit([], "", True)
            return

        entries = self.recommender.recommend(
            song_id=self._song_id,
            button_mode=self._button_mode,
            difficulty=self._difficulty,
        )
        pivot = f"{self._button_mode} {self._difficulty}"
        # 결과 데이터를 시그널에 실어 메인 스레드로 전달
        self.signals.data_ready.emit(entries, pivot, False)
