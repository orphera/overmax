"""
sync_window.py - V-Archive 동기화 창

overmax 수집 기록과 V-Archive 기록을 비교하여
등록 후보를 표시하고 행별 API 등록을 지원한다.
"""

from typing import Optional

from PyQt6.QtCore import Qt, QPoint, pyqtSignal, QObject
from PyQt6.QtGui import QColor, QPainter, QBrush
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QFrame,
    QLabel, QPushButton, QScrollArea, QApplication,
)

from data.sync_manager import SyncCandidate
from data.varchive_uploader import AccountInfo
from data.varchive import VArchiveDB
from data.record_manager import RecordManager
from overlay.sync_actions import SyncActionsMixin
from overlay.sync_candidate_row import CandidateRow


# ------------------------------------------------------------------
# 시그널 브릿지 (worker thread → Qt)
# ------------------------------------------------------------------

class _SyncSignals(QObject):
    row_status_changed = pyqtSignal(int, str, str)   # index, status, message
    scan_finished      = pyqtSignal(list)             # list[SyncCandidate]
    action_finished    = pyqtSignal()                 # 등록/삭제 완료 후 재스캔


# ------------------------------------------------------------------
# 동기화 창
# ------------------------------------------------------------------

class SyncWindow(SyncActionsMixin, QWidget):
    def __init__(
        self,
        varchive_db: VArchiveDB,
        record_manager: RecordManager,
        parent=None,
    ):
        super().__init__(parent)
        self._vdb = varchive_db
        self._record_manager = record_manager
        self._accounts: dict[str, AccountInfo] = {}   # steam_id → AccountInfo
        self._candidates: list[SyncCandidate] = []
        self._rows: list[CandidateRow] = []
        self._signals = _SyncSignals()
        self._dragging = False
        self._drag_pos = QPoint()
        self._scan_in_progress = False
        self._rescan_queued = False
        self._current_steam_id: Optional[str] = None

        self._setup_window()
        self._build_ui()
        self._connect_signals()

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setMinimumWidth(680)

    def _connect_signals(self):
        self._signals.row_status_changed.connect(self._on_row_status)
        self._signals.scan_finished.connect(self._on_scan_finished)
        self._signals.action_finished.connect(self._on_action_finished)

    # ------------------------------------------------------------------
    # UI 구성
    # ------------------------------------------------------------------

    def _build_ui(self):
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)

        panel = QFrame()
        panel.setStyleSheet("""
            QFrame {
                background: rgb(18, 24, 38);
                border-radius: 14px;
            }
        """)
        panel_layout = QVBoxLayout(panel)
        panel_layout.setContentsMargins(0, 0, 0, 0)
        panel_layout.setSpacing(0)

        panel_layout.addWidget(self._build_header())
        panel_layout.addWidget(self._build_column_header())
        panel_layout.addWidget(self._build_scroll_area(), 1)
        panel_layout.addWidget(self._build_footer())

        root.addWidget(panel)

    def _build_header(self) -> QFrame:
        header = QFrame()
        header.setFixedHeight(48)
        header.setStyleSheet("""
            QFrame {
                background: rgb(30, 40, 62);
                border-top-left-radius: 14px;
                border-top-right-radius: 14px;
            }
        """)
        layout = QHBoxLayout(header)
        layout.setContentsMargins(16, 0, 12, 0)
        layout.setSpacing(8)

        icon = QLabel("⟳")
        icon.setStyleSheet("color: #00D4FF; font-size: 16px; font-weight: bold;")
        layout.addWidget(icon)

        title = QLabel("V-Archive 동기화")
        title.setStyleSheet("color: #F0F4FF; font-size: 14px; font-weight: bold;")
        layout.addWidget(title)

        self._count_label = QLabel("")
        self._count_label.setStyleSheet("color: #505870; font-size: 12px;")
        layout.addWidget(self._count_label)

        layout.addStretch()

        close_btn = QPushButton("✕")
        close_btn.setFixedSize(28, 28)
        close_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        close_btn.setStyleSheet("""
            QPushButton { color: #505870; background: transparent; border: none; font-size: 16px; }
            QPushButton:hover { color: #FF4B4B; }
        """)
        close_btn.clicked.connect(self.hide)
        layout.addWidget(close_btn)
        return header

    def _build_column_header(self) -> QFrame:
        col = QFrame()
        col.setFixedHeight(28)
        col.setStyleSheet("QFrame { background: rgb(22, 30, 48); border: none; }")
        layout = QHBoxLayout(col)
        layout.setContentsMargins(12, 0, 8, 0)
        layout.setSpacing(8)

        def _col_label(text: str, width: Optional[int] = None, stretch: int = 0) -> QLabel:
            lbl = QLabel(text)
            lbl.setStyleSheet("color: #505870; font-size: 10px; font-weight: 600;")
            lbl.setAlignment(Qt.AlignmentFlag.AlignVCenter)
            if width:
                lbl.setFixedWidth(width)
            return lbl

        layout.addWidget(_col_label("난이도", 32))
        layout.addWidget(_col_label("모드", 28))
        layout.addWidget(_col_label("곡명"), 1)
        layout.addWidget(_col_label("Overmax", 64))
        layout.addWidget(_col_label("", 16))
        layout.addWidget(_col_label("V-Archive", 72))
        layout.addWidget(_col_label("차이", 64))
        layout.addWidget(_col_label("", 48))
        return col

    def _build_scroll_area(self) -> QScrollArea:
        self._list_widget = QWidget()
        self._list_widget.setStyleSheet("background: transparent;")
        self._list_layout = QVBoxLayout(self._list_widget)
        self._list_layout.setContentsMargins(8, 8, 8, 8)
        self._list_layout.setSpacing(4)
        self._list_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        self._empty_label = QLabel("동기화 후보를 불러오는 중...")
        self._empty_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._empty_label.setStyleSheet("color: #505870; font-size: 13px; padding: 40px 0;")
        self._list_layout.addWidget(self._empty_label)

        scroll = QScrollArea()
        scroll.setWidget(self._list_widget)
        scroll.setWidgetResizable(True)
        scroll.setFixedHeight(420)
        scroll.setStyleSheet("""
            QScrollArea { border: none; background: transparent; }
            QScrollBar:vertical {
                background: rgb(22, 30, 48);
                width: 6px;
                border-radius: 3px;
            }
            QScrollBar::handle:vertical {
                background: rgb(60, 75, 110);
                border-radius: 3px;
                min-height: 20px;
            }
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0px; }
        """)
        return scroll

    def _build_footer(self) -> QFrame:
        footer = QFrame()
        footer.setFixedHeight(48)
        footer.setStyleSheet("""
            QFrame {
                background: rgb(22, 30, 48);
                border-bottom-left-radius: 14px;
                border-bottom-right-radius: 14px;
            }
        """)
        layout = QHBoxLayout(footer)
        layout.setContentsMargins(16, 0, 12, 0)

        self._status_label = QLabel("account.txt를 설정하고 불러오기를 눌러주세요.")
        self._status_label.setStyleSheet("color: #8891A7; font-size: 11px;")
        layout.addWidget(self._status_label, 1)

        self._refresh_btn = QPushButton("불러오기")
        self._refresh_btn.setFixedSize(72, 28)
        self._refresh_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._refresh_btn.setStyleSheet("""
            QPushButton {
                background: rgb(0, 140, 200);
                color: #FFFFFF;
                border: none;
                border-radius: 5px;
                font-size: 11px;
                font-weight: 700;
            }
            QPushButton:hover { background: rgb(0, 180, 240); }
            QPushButton:disabled { background: rgb(40, 50, 80); color: #505870; }
        """)
        self._refresh_btn.clicked.connect(self._start_scan)
        layout.addWidget(self._refresh_btn)
        return footer

    def _get_current_account(self) -> Optional[AccountInfo]:
        if not self._current_steam_id:
            return None
        return self._accounts.get(self._current_steam_id)

    # ------------------------------------------------------------------
    # 공개 API
    # ------------------------------------------------------------------

    def set_account(self, steam_id: str, account: Optional[AccountInfo]):
        if account:
            self._accounts[steam_id] = account
        else:
            self._accounts.pop(steam_id, None)
        
        self._update_ui_states()

    def _update_ui_states(self):
        """현재 계정 상태와 스캔 상태에 따라 UI 활성화 여부를 결정한다."""
        has_account = self._get_current_account() is not None
        is_scanning = self._scan_in_progress
        
        # 불러오기 버튼은 계정이 있고 스캔 중이 아닐 때만 활성화
        self._refresh_btn.setEnabled(has_account and not is_scanning)
        
        # 각 행의 등록 버튼도 계정 여부에 따라 업데이트
        for row in self._rows:
            row.set_upload_enabled(has_account)

    def show_window(self, steam_id: str, persona_name: str, account_path: str):
        self._current_steam_id = steam_id

        path = account_path.strip()
        account = None
        if path:
            from data.varchive_uploader import parse_account_file
            account = parse_account_file(path)
        self.set_account(steam_id, account)

        # title 설정
        title_name = persona_name.strip() or steam_id
        self.setWindowTitle(f"V-Archive 동기화 - {title_name}")

        self.adjustSize()
        screen = QApplication.primaryScreen().geometry()
        x = (screen.width() - self.width()) // 2
        y = (screen.height() - self.height()) // 2
        self.move(x, y)
        self.show()
        self.raise_()
        self.activateWindow()
        if self._record_manager is not None and self._get_current_account() is not None and not self._candidates:
            self._start_scan()

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
        self._dragging = False

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QBrush(QColor(0, 0, 0, 60)))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(self.rect().adjusted(3, 4, -1, -1), 14, 14)
