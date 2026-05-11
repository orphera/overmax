"""
sync_window.py - V-Archive 동기화 창

overmax 수집 기록과 V-Archive 기록을 비교하여
등록 후보를 표시하고 행별 API 등록을 지원한다.
"""

from __future__ import annotations

import threading
from typing import Optional

from PyQt6.QtCore import Qt, QPoint, pyqtSignal, QObject
from PyQt6.QtGui import QColor, QPainter, QBrush, QFont
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QFrame,
    QLabel, QPushButton, QScrollArea, QApplication,
)

from data.sync_manager import SyncCandidate, build_candidates
from data.varchive_uploader import AccountInfo, upload_score
from data.varchive import VArchiveDB
from data.record_manager import RecordManager


# ------------------------------------------------------------------
# 시그널 브릿지 (worker thread → Qt)
# ------------------------------------------------------------------

class _SyncSignals(QObject):
    row_status_changed = pyqtSignal(int, str, str)   # index, status, message
    scan_finished      = pyqtSignal(list)             # list[SyncCandidate]
    action_finished    = pyqtSignal()                 # 등록/삭제 완료 후 재스캔


# ------------------------------------------------------------------
# 난이도별 색상
# ------------------------------------------------------------------

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

_BUTTON_NUM_BY_MODE = {
    "4B": 4,
    "5B": 5,
    "6B": 6,
    "8B": 8,
}


def _s(base: int, scale: float = 1.0) -> int:
    return max(1, round(base * scale))


# ------------------------------------------------------------------
# 행 위젯
# ------------------------------------------------------------------

class _CandidateRow(QFrame):
    upload_requested = pyqtSignal(int)   # candidate index
    delete_requested = pyqtSignal(int)   # candidate index

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

        # 난이도 뱃지
        diff_badge = QLabel(self.candidate.difficulty)
        diff_badge.setFixedWidth(32)
        diff_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        color = _DIFF_COLORS.get(self.candidate.difficulty, "#FFFFFF")
        diff_badge.setStyleSheet(
            f"background: {color}; color: #FFFFFF; font-size: 10px; "
            f"font-weight: 700; border-radius: 4px; padding: 2px 0;"
        )
        layout.addWidget(diff_badge)

        # 버튼 모드 뱃지
        mode_badge = QLabel(self.candidate.button_mode)
        mode_badge.setFixedWidth(28)
        mode_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        mode_color = _BTN_COLORS.get(self.candidate.button_mode, "#444")
        mode_badge.setStyleSheet(
            f"background: {mode_color}; color: #FFFFFF; font-size: 9px; "
            f"font-weight: 700; border-radius: 3px; padding: 2px 0;"
        )
        layout.addWidget(mode_badge)

        # 곡명
        name_label = QLabel(self.candidate.song_name)
        name_label.setStyleSheet("color: #E8EEFF; font-size: 12px; font-weight: 600;")
        try:
            fm = name_label.fontMetrics()
            elided = fm.elidedText(self.candidate.song_name, Qt.TextElideMode.ElideRight, 180)
            name_label.setText(elided)
        except Exception:
            pass
        layout.addWidget(name_label, 1)

        # overmax rate
        om_label = QLabel(f"{self.candidate.overmax_rate:.2f}%")
        om_label.setFixedWidth(64)
        om_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        om_label.setStyleSheet("color: #00D4FF; font-size: 12px; font-weight: 700;")
        if self.candidate.overmax_mc:
            om_label.setText(om_label.text() + " M")
        layout.addWidget(om_label)

        # 구분자 →
        arrow = QLabel("→")
        arrow.setFixedWidth(16)
        arrow.setAlignment(Qt.AlignmentFlag.AlignCenter)
        arrow.setStyleSheet("color: #505870; font-size: 11px;")
        layout.addWidget(arrow)

        # v아카이브 rate
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
        layout.addWidget(va_label)

        # 이유 태그
        reason_label = QLabel(self.candidate.reason)
        reason_label.setFixedWidth(64)
        reason_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        reason_label.setStyleSheet(
            "color: #FFD166; font-size: 10px; font-weight: 600;"
        )
        layout.addWidget(reason_label)

        # 등록 버튼 / 상태 표시
        self._upload_btn = self._build_upload_btn()
        self._delete_btn = self._build_delete_btn()
        action_layout = QHBoxLayout()
        action_layout.setSpacing(2)
        action_layout.addWidget(self._upload_btn)
        action_layout.addWidget(self._delete_btn)
        layout.addLayout(action_layout)

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
        """등록 결과 반영: success / no_update / error / pending"""
        style_map = {
            "pending":   ("rgb(180, 140, 0)",   "처리 중"),
            "success":   ("rgb(0, 180, 120)",   "완료"),
            "no_update": ("rgb(60, 80, 120)",   "최신"),
            "error":     ("rgb(180, 60, 60)",   "실패"),
        }
        bg, text = style_map.get(status, ("rgb(60, 80, 120)", status))

        if isinstance(self._upload_btn, QPushButton):
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


# ------------------------------------------------------------------
# 동기화 창
# ------------------------------------------------------------------

class SyncWindow(QWidget):
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
        self._rows: list[_CandidateRow] = []
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
    # 스캔 (백그라운드)
    # ------------------------------------------------------------------

    def _start_scan(self):
        if self._record_manager is None:
            self._status_label.setText("기록 관리자가 초기화되지 않았습니다.")
            return
        if self._scan_in_progress:
            self._rescan_queued = True
            return
        self._scan_in_progress = True
        self._update_ui_states()
        self._status_label.setText("비교 중...")
        self._clear_list()
        self._empty_label.setText("분석 중...")
        self._empty_label.show()

        threading.Thread(target=self._scan_worker, daemon=True).start()

    def _scan_worker(self):
        try:
            candidates = build_candidates(self._vdb, self._record_manager)
        except Exception as e:
            candidates = []
            print(f"[SyncWindow] 스캔 오류: {e}")
        self._signals.scan_finished.emit(candidates)

    def _on_scan_finished(self, candidates: list[SyncCandidate]):
        self._scan_in_progress = False
        self._candidates = candidates
        
        self._update_ui_states()

        self._clear_list()

        if not candidates:
            self._empty_label.setText("동기화 후보가 없습니다. V-Archive 기록이 이미 최신입니다.")
            self._empty_label.show()
            self._count_label.setText("")
            self._status_label.setText("최신 상태입니다.")
            if self._rescan_queued:
                self._rescan_queued = False
                self._start_scan()
            return

        self._empty_label.hide()
        has_account = self._get_current_account() is not None
        for i, c in enumerate(candidates):
            row = _CandidateRow(i, c)
            row.set_upload_enabled(has_account)
            row.upload_requested.connect(self._on_upload_requested)
            row.delete_requested.connect(self._on_delete_requested)
            self._list_layout.addWidget(row)
            self._rows.append(row)

        count = len(candidates)
        self._count_label.setText(f"— {count}개 후보")
        self._status_label.setText(f"{count}개의 갱신 후보를 찾았습니다.")
        self.adjustSize()

        if self._rescan_queued:
            self._rescan_queued = False
            self._start_scan()

    def _clear_list(self):
        for i in range(self._list_layout.count()):
            item = self._list_layout.itemAt(i)
            if item and item.widget() and item.widget() != self._empty_label:
                item.widget().deleteLater()
        self._rows = []
        # self._empty_label은 이미 layout에 있으므로 재추가하지 말고 show만
        self._empty_label.show()

    # ------------------------------------------------------------------
    # 등록 (백그라운드)
    # ------------------------------------------------------------------

    def _on_upload_requested(self, index: int):
        account = self._get_current_account()
        if account is None:
            return
        if index >= len(self._candidates):
            return

        if index < len(self._rows):
            self._rows[index].set_status("pending", "")

        threading.Thread(
            target=self._upload_worker,
            args=(index, self._candidates[index], account),
            daemon=True,
        ).start()

    def _on_delete_requested(self, index: int):
        if index >= len(self._candidates):
            return

        if index < len(self._rows):
            self._rows[index].set_status("pending", "")

        threading.Thread(
            target=self._delete_worker,
            args=(index, self._candidates[index]),
            daemon=True,
        ).start()

    def _upload_worker(self, index: int, candidate: SyncCandidate, account: AccountInfo):
        result = upload_score(
            account=account,
            song_name=candidate.song_name,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
            composer=candidate.composer,
        )

        if result.success:
            status = "success" if result.updated else "no_update"
            message = ""
            if result.updated:
                self._update_varchive_cache_after_upload(candidate)
        else:
            status = "error"
            message = result.message

        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _update_varchive_cache_after_upload(self, candidate: SyncCandidate):
        button = _BUTTON_NUM_BY_MODE.get(candidate.button_mode)
        if button is None:
            return

        steam_id = self._current_steam_id
        if steam_id == "__unknown__":
            return

        vclient = getattr(self._record_manager, "vclient", None)
        if vclient is None:
            return

        success = vclient.upsert_cached_record(
            steam_id=steam_id,
            button=button,
            song_id=candidate.song_id,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
        )
        if success:
            self._record_manager.refresh()

    def _delete_worker(self, index: int, candidate: SyncCandidate):
        # overmax 로컬 기록 삭제
        success = self._record_manager.delete(
            song_id=candidate.song_id,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
        )

        if success:
            status = "success"
            message = ""
        else:
            status = "error"
            message = "삭제 실패"

        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _on_row_status(self, index: int, status: str, message: str):
        if index < len(self._rows):
            self._rows[index].set_status(status, message)

    def _on_action_finished(self):
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
