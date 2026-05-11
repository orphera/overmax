"""PyQt6 settings window with Overlay-like style."""

from PyQt6.QtCore import Qt, pyqtSignal, QPoint
from PyQt6.QtGui import QColor, QPainter, QBrush
from PyQt6.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QHBoxLayout,
    QTabWidget,
    QLabel,
    QSlider,
    QPushButton,
    QFrame,
    QButtonGroup,
    QCheckBox,
    QLineEdit,
    QScrollArea,
)

from settings import SETTINGS, save_settings
from data.steam_session import get_all_steam_sessions, mask_steam_id, get_most_recent_steam_id
from core.version import APP_VERSION
from constants import SCALE_PRESETS





class SettingsWindow(QWidget):
    """애플리케이션 설정 창 - 오버레이와 통일된 디자인 적용."""

    opacity_changed = pyqtSignal(float)
    scale_changed   = pyqtSignal(float)
    fetch_varchive_requested = pyqtSignal(str, str, int)  # steam_id, v_id, button (0 for all)
    sync_requested = pyqtSignal(str, str, str)   # 동기화 창 열기 요청 (steam_id, persona_name, account_path)
    account_file_changed = pyqtSignal(str, str)  # steam_id, account_path

    def __init__(self):
        super().__init__()
        self._dragging = False
        self._drag_pos = QPoint()
        self._session_rows_by_sid: dict[str, QFrame] = {}
        self._session_labels_by_sid: dict[str, QLabel] = {}
        self._session_names_by_sid: dict[str, str] = {}
        self._account_edits_by_sid: dict[str, QLineEdit] = {}
        self._last_sid: str | None = None

        self._setup_window()
        self._setup_ui()

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint |
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setMinimumSize(400, 450)

    def _setup_ui(self):
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)

        self.main_frame = QFrame()
        self.main_frame.setObjectName("MainFrame")
        self.main_frame.setStyleSheet("""
            QFrame#MainFrame {
                background: rgb(18, 24, 38);
                border-radius: 14px;
                border: 1px solid rgb(40, 50, 80);
            }
            QLabel {
                color: #F0F4FF;
            }
        """)

        layout = QVBoxLayout(self.main_frame)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # 1. 헤더 (드래그 가능)
        layout.addWidget(self._build_header())

        # 2. 컨텐츠 (탭)
        content_layout = QVBoxLayout()
        content_layout.setContentsMargins(15, 15, 15, 15)

        self.tabs = QTabWidget()
        self.tabs.setStyleSheet("""
            QTabWidget::pane {
                border: 1px solid rgb(40, 50, 80);
                background: rgb(24, 32, 50);
                border-radius: 8px;
                top: -1px;
            }
            QTabBar::tab {
                background: rgb(30, 40, 62);
                color: #8891A7;
                padding: 8px 20px;
                border-top-left-radius: 8px;
                border-top-right-radius: 8px;
                margin-right: 2px;
            }
            QTabBar::tab:selected {
                background: rgb(24, 32, 50);
                color: #F0F4FF;
                border-bottom: 2px solid #00D4FF;
            }
            QTabBar::tab:hover:!selected {
                background: rgb(35, 45, 70);
                color: #F0F4FF;
            }
        """)

        self.tabs.addTab(self._build_ui_tab(), "UI")
        self.tabs.addTab(self._build_varchive_tab(), "V-Archive")
        self.tabs.addTab(self._build_system_tab(), "System")
        content_layout.addWidget(self.tabs)
        layout.addLayout(content_layout)

        root.addWidget(self.main_frame)

    def _build_header(self) -> QFrame:
        header = QFrame()
        header.setFixedHeight(45)
        header.setStyleSheet("""
            QFrame {
                background: rgb(30, 40, 62);
                border-top-left-radius: 14px;
                border-top-right-radius: 14px;
            }
            QLabel {
                color: #F0F4FF;
                font-size: 14px;
                font-weight: bold;
            }
        """)

        layout = QHBoxLayout(header)
        layout.setContentsMargins(15, 0, 10, 0)

        title = QLabel("Overmax 설정")
        layout.addWidget(title)
        layout.addStretch()

        close_btn = QPushButton("✕")
        close_btn.setFixedSize(28, 28)
        close_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        close_btn.setStyleSheet("""
            QPushButton {
                color: #8891A7;
                background: transparent;
                border: none;
                font-size: 16px;
            }
            QPushButton:hover {
                color: #FF4B4B;
                background: rgba(255, 75, 75, 20);
                border-radius: 14px;
            }
        """)
        close_btn.clicked.connect(self.hide)
        layout.addWidget(close_btn)
        return header

    def _build_ui_tab(self) -> QWidget:
        tab = QWidget()
        tab.setStyleSheet("background: transparent;")
        layout = QVBoxLayout(tab)
        layout.setContentsMargins(15, 20, 15, 20)
        layout.setSpacing(24)
        layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        layout.addWidget(self._build_opacity_row())
        layout.addWidget(self._build_scale_row())

        return tab

    def _build_varchive_tab(self) -> QWidget:
        # 탭 전체를 스크롤 가능하게 구성
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("""
            QScrollArea {
                border: none;
                background: transparent;
            }
            QScrollBar:vertical {
                background: rgb(24, 32, 50);
                width: 8px;
                border-radius: 4px;
            }
            QScrollBar::handle:vertical {
                background: rgb(60, 75, 110);
                min-height: 20px;
                border-radius: 4px;
            }
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
                height: 0px;
            }
        """)

        container = QWidget()
        container.setStyleSheet("background: transparent;")
        self._varchive_main_layout = QVBoxLayout(container)
        self._varchive_main_layout.setContentsMargins(15, 20, 15, 20)
        self._varchive_main_layout.setSpacing(15)
        self._varchive_main_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        scroll.setWidget(container)
        self._refresh_varchive_tab_content()
        return scroll

    def _refresh_varchive_tab_content(self):
        """V-Archive 탭의 내용을 현재 스팀 세션에 맞춰 재구성."""
        # 기존 내용 삭제
        while self._varchive_main_layout.count():
            item = self._varchive_main_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                self._clear_layout(item.layout())

        self._session_rows_by_sid = {}
        self._session_labels_by_sid = {}
        self._session_names_by_sid = {}

        # 1. Header description & Auto-refresh
        desc_row = QHBoxLayout()
        desc = QLabel("Steam 계정별 V-Archive ID를 입력하세요.")
        desc.setStyleSheet("color: #8891A7; font-size: 12px; margin-bottom: 5px;")
        desc_row.addWidget(desc)
        desc_row.addStretch()
        
        auto_cb = QCheckBox("시작 시 자동 갱신")
        auto_cb.setChecked(SETTINGS.get("varchive", {}).get("auto_refresh", False))
        auto_cb.setStyleSheet("color: #00D4FF; font-size: 11px;")
        auto_cb.toggled.connect(self._on_auto_refresh_toggled)
        desc_row.addWidget(auto_cb)
        self._varchive_main_layout.addLayout(desc_row)

        sessions = get_all_steam_sessions()
        if not sessions:
            self._varchive_main_layout.addWidget(QLabel("발견된 Steam 계정이 없습니다."))
            return

        if "varchive" not in SETTINGS:
            SETTINGS["varchive"] = {}
        if "user_map" not in SETTINGS["varchive"]:
            SETTINGS["varchive"]["user_map"] = {}

        user_map = SETTINGS["varchive"]["user_map"]
        current_sid = get_most_recent_steam_id()
        self._last_sid = current_sid

        # Find current session and others
        current_session = None
        for s in sessions:
            if s.steam_id == current_sid:
                current_session = s
                break
        
        if not current_session and sessions:
            current_session = sessions[0]
            current_sid = current_session.steam_id

        other_sessions = [s for s in sessions if s.steam_id != current_sid]

        # 2. Current Account
        if current_session:
            current_row = self._build_session_row(current_session, user_map, is_current=True)
            self._varchive_main_layout.addWidget(current_row)

        # 3. Other Accounts (Collapsible)
        if other_sessions:
            self._varchive_main_layout.addSpacing(10)
            
            # Toggle Button
            self.toggle_others_btn = QPushButton(f"다른 계정 보기 ({len(other_sessions)}) ▾")
            self.toggle_others_btn.setCursor(Qt.CursorShape.PointingHandCursor)
            self.toggle_others_btn.setStyleSheet("""
                QPushButton {
                    background: transparent;
                    color: #8891A7;
                    border: none;
                    font-size: 11px;
                    text-align: left;
                    padding: 5px 0;
                }
                QPushButton:hover {
                    color: #F0F4FF;
                }
            """)
            self._varchive_main_layout.addWidget(self.toggle_others_btn)

            # Others List (Simple Container)
            self.others_container = QWidget()
            self.others_container.setStyleSheet("background: transparent;")
            others_layout = QVBoxLayout(self.others_container)
            others_layout.setContentsMargins(0, 0, 0, 0)
            others_layout.setSpacing(10)
            others_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

            for s in other_sessions:
                others_layout.addWidget(self._build_session_row(s, user_map, is_current=False))
            
            self.others_container.setVisible(False)
            self._varchive_main_layout.addWidget(self.others_container)

            self.toggle_others_btn.clicked.connect(self._toggle_others)

    def _clear_layout(self, layout):
        if layout is None: return
        while layout.count():
            item = layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                self._clear_layout(item.layout())

    def _toggle_others(self):
        is_visible = self.others_container.isVisible()
        self.others_container.setVisible(not is_visible)
        text = self.toggle_others_btn.text()
        if "▾" in text:
            self.toggle_others_btn.setText(text.replace("▾", "▴"))
        else:
            self.toggle_others_btn.setText(text.replace("▴", "▾"))

    def _build_session_row(self, s, user_map, is_current: bool) -> QFrame:
        entry = user_map.get(s.steam_id, {})
        v_id = entry.get("v_id", "") if isinstance(entry, dict) else ""
        account_path = entry.get("account_path", "") if isinstance(entry, dict) else ""

        row = QFrame()
        row.setObjectName(f"SessionRow_{s.steam_id}")
        row.setMinimumHeight(160) # 찌그러짐 방지
        row_layout = QVBoxLayout(row)
        row_layout.setContentsMargins(12, 12, 12, 12)
        
        # 상단: 계정 정보
        info_row = QHBoxLayout()
        base_label = f"{s.persona_name} ({s.account_name})"
        name_label = QLabel(base_label)
        badge_label = QLabel("Current")
        badge_label.setVisible(False)
        badge_label.setStyleSheet("""
            QLabel {
                color: #021620;
                background: #00D4FF;
                border: 1px solid #6FE7FF;
                border-radius: 9px;
                font-size: 10px;
                font-weight: 800;
                padding: 1px 8px;
            }
        """)
        info_row.addWidget(name_label)
        info_row.addStretch()
        info_row.addWidget(badge_label)
        row_layout.addLayout(info_row)

        # 하단: 입력 및 버튼
        input_row = QHBoxLayout()
        edit = QLineEdit()
        edit.setPlaceholderText("V-Archive ID")
        edit.setText(v_id)
        edit.setStyleSheet("""
            QLineEdit {
                background: rgb(24, 32, 50);
                border: 1px solid rgb(60, 75, 110);
                border-radius: 4px;
                color: #F0F4FF;
                padding: 4px 8px;
            }
        """)
        edit.textChanged.connect(lambda text, sid=s.steam_id: self._on_v_id_changed(sid, text))
        input_row.addWidget(edit, 3)

        btn_layout = QHBoxLayout()
        btn_layout.setSpacing(4)
        for b in [4, 5, 6, 8]:
            btn = QPushButton(f"{b}B")
            btn.setFixedSize(30, 24)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setStyleSheet(self._fetch_btn_style())
            btn.clicked.connect(lambda _, sid=s.steam_id, vid_edit=edit, btn_val=b: 
                              self.fetch_varchive_requested.emit(sid, vid_edit.text().strip(), btn_val))
            btn_layout.addWidget(btn)
        
        all_btn = QPushButton("All")
        all_btn.setFixedSize(35, 24)
        all_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        all_btn.setStyleSheet(self._fetch_btn_style(is_all=True))
        all_btn.clicked.connect(lambda _, sid=s.steam_id, vid_edit=edit:
                               self.fetch_varchive_requested.emit(sid, vid_edit.text().strip(), 0))
        btn_layout.addWidget(all_btn)
        
        input_row.addLayout(btn_layout, 2)
        row_layout.addLayout(input_row)

        row_layout.addSpacing(8)
        account_label = QLabel("V-Archive account.txt")
        account_label.setStyleSheet("color: #8891A7; font-size: 11px; margin-top: 4px;")
        row_layout.addWidget(account_label)

        account_row = QHBoxLayout()
        account_edit = QLineEdit()
        account_edit.setPlaceholderText("account.txt 경로")
        account_edit.setText(account_path)
        account_edit.setReadOnly(True)
        account_edit.setStyleSheet("""
            QLineEdit {
                background: rgb(24, 32, 50);
                border: 1px solid rgb(60, 75, 110);
                border-radius: 4px;
                color: #F0F4FF;
                padding: 4px 8px;
            }
        """)
        account_row.addWidget(account_edit, 1)

        browse_btn = QPushButton("찾아보기")
        browse_btn.setFixedSize(64, 28)
        browse_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        browse_btn.setStyleSheet(self._fetch_btn_style())
        browse_btn.clicked.connect(lambda _, sid=s.steam_id, e=account_edit: self._on_browse_account(sid, e))
        account_row.addWidget(browse_btn)

        sync_btn = QPushButton("동기화 후보")
        sync_btn.setFixedSize(85, 28)
        sync_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        sync_btn.setStyleSheet(self._fetch_btn_style(is_all=True))
        sync_btn.clicked.connect(lambda _, sid=s.steam_id, name=s.persona_name, path=account_edit: self.sync_requested.emit(sid, name, path.text().strip()))
        account_row.addWidget(sync_btn)

        row_layout.addLayout(account_row)

        self._session_rows_by_sid[s.steam_id] = row
        self._session_labels_by_sid[s.steam_id] = name_label
        self._session_names_by_sid[s.steam_id] = base_label
        self._apply_current_style(s.steam_id, is_current)
        
        return row

    def _apply_current_style(self, steam_id: str, is_current: bool):
        row = self._session_rows_by_sid.get(steam_id)
        label = self._session_labels_by_sid.get(steam_id)
        base_name = self._session_names_by_sid.get(steam_id)
        if row is None or label is None or base_name is None:
            return

        border_color = "rgb(0, 212, 255)" if is_current else "rgb(40, 50, 80)"
        text_color = "#00D4FF" if is_current else "#F0F4FF"
        row.setStyleSheet(f"""
            QFrame#SessionRow_{steam_id} {{
                background: rgb(30, 40, 62);
                border-radius: 8px;
                border: 1px solid {border_color};
            }}
        """)
        label.setText(base_name)
        label.setStyleSheet(f"font-weight: bold; border: none; color: {text_color};")

    def _on_v_id_changed(self, steam_id: str, v_id: str):
        entry = SETTINGS["varchive"]["user_map"].setdefault(steam_id, {})
        if isinstance(entry, str):
            entry = {"v_id": entry, "account_path": ""}
            SETTINGS["varchive"]["user_map"][steam_id] = entry
        entry["v_id"] = v_id.strip()
        save_settings()

    def _on_account_path_changed(self, steam_id: str, path: str):
        entry = SETTINGS["varchive"]["user_map"].setdefault(steam_id, {})
        entry["account_path"] = path
        save_settings()
        # controller에 알림
        self.account_file_changed.emit(steam_id, path)

    def _on_auto_refresh_toggled(self, checked: bool):
        if "varchive" not in SETTINGS:
            SETTINGS["varchive"] = {}
        SETTINGS["varchive"]["auto_refresh"] = checked
        save_settings()

    def _fetch_btn_style(self, is_all=False) -> str:
        color = "#00D4FF" if not is_all else "#00FF88"
        return f"""
            QPushButton {{
                background: rgb(40, 54, 84);
                color: {color};
                border: 1px solid rgb(60, 75, 110);
                border-radius: 4px;
                font-size: 10px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                background: {color};
                color: rgb(18, 24, 38);
            }}
        """

    def _on_browse_account(self, steam_id: str, account_edit: QLineEdit):
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            self, "account.txt 선택", "", "Text Files (*.txt);;All Files (*)"
        )
        if not path:
            return
        account_edit.setText(path)
        self._on_account_path_changed(steam_id, path)

    def _build_system_tab(self) -> QWidget:
        tab = QWidget()
        tab.setStyleSheet("background: transparent;")
        layout = QVBoxLayout(tab)
        layout.setContentsMargins(15, 20, 15, 20)
        layout.setSpacing(24)
        layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        enabled_checkbox = QCheckBox("자동 업데이트")
        enabled_checkbox.setChecked(SETTINGS.get("app_update", {}).get("enabled", True))
        enabled_checkbox.setStyleSheet("color: #F0F4FF;")
        enabled_checkbox.toggled.connect(self._on_enabled_toggled)
        layout.addWidget(enabled_checkbox)

        current_version_label = QLabel(f"현재 버전: {APP_VERSION}")
        current_version_label.setStyleSheet("color: #F0F4FF; font-size: 16px; font-weight: 700;")
        layout.addWidget(current_version_label)

        return tab

    def _on_enabled_toggled(self, checked: bool):
        SETTINGS.get("app_update", {})["enabled"] = checked
        save_settings()

    # ------------------------------------------------------------------
    # 투명도 슬라이더
    # ------------------------------------------------------------------

    def _build_opacity_row(self) -> QWidget:
        container = QWidget()
        container.setStyleSheet("background: transparent;")
        layout = QVBoxLayout(container)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        label_row = QHBoxLayout()
        label_row.addWidget(QLabel("오버레이 투명도"))
        self._opacity_val_label = QLabel()
        self._opacity_val_label.setStyleSheet(
            "color: #00D4FF; font-weight: bold; font-size: 14px;"
        )
        label_row.addWidget(self._opacity_val_label)
        label_row.addStretch()
        layout.addLayout(label_row)

        base_val = float(SETTINGS.get("overlay", {}).get("base_opacity", 0.8))
        self._opacity_val_label.setText(f"{base_val:.1f}")

        self._opacity_slider = QSlider(Qt.Orientation.Horizontal)
        self._opacity_slider.setMinimum(1)
        self._opacity_slider.setMaximum(10)
        self._opacity_slider.setValue(round(base_val * 10))
        self._opacity_slider.setStyleSheet("""
            QSlider::handle:horizontal {
                background: #00D4FF;
                border: 1px solid #00D4FF;
                width: 16px;
                height: 16px;
                margin: -5px 0;
                border-radius: 8px;
            }
            QSlider::groove:horizontal {
                height: 6px;
                background: rgb(40, 50, 80);
                border-radius: 3px;
            }
        """)
        self._opacity_slider.valueChanged.connect(self._on_opacity_changed)
        layout.addWidget(self._opacity_slider)
        return container

    def _on_opacity_changed(self, value: int):
        float_val = value / 10.0
        self._opacity_val_label.setText(f"{float_val:.1f}")
        if "overlay" not in SETTINGS:
            SETTINGS["overlay"] = {}
        SETTINGS["overlay"]["base_opacity"] = float_val
        save_settings()
        self.opacity_changed.emit(float_val)

    # ------------------------------------------------------------------
    # 크기 프리셋 버튼
    # ------------------------------------------------------------------

    def _build_scale_row(self) -> QWidget:
        container = QWidget()
        container.setStyleSheet("background: transparent;")
        layout = QVBoxLayout(container)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        layout.addWidget(QLabel("오버레이 크기"))

        btn_row = QHBoxLayout()
        btn_row.setSpacing(6)

        current_scale = float(SETTINGS.get("overlay", {}).get("scale", 1.0))
        self._scale_btn_group = QButtonGroup(self)
        self._scale_btn_group.setExclusive(True)

        for label, scale_val in SCALE_PRESETS:
            btn = QPushButton(label)
            btn.setCheckable(True)
            btn.setChecked(abs(scale_val - current_scale) < 0.01)
            btn.setProperty("scale_val", scale_val)
            btn.setStyleSheet(self._preset_btn_style(active=btn.isChecked()))
            btn.toggled.connect(lambda checked, b=btn, v=scale_val: self._on_preset_toggled(b, v, checked))
            self._scale_btn_group.addButton(btn)
            btn_row.addWidget(btn)

        layout.addLayout(btn_row)
        return container

    def _on_preset_toggled(self, btn: QPushButton, scale_val: float, checked: bool):
        if not checked:
            btn.setStyleSheet(self._preset_btn_style(active=False))
            return

        btn.setStyleSheet(self._preset_btn_style(active=True))
        if "overlay" not in SETTINGS:
            SETTINGS["overlay"] = {}
        SETTINGS["overlay"]["scale"] = scale_val
        save_settings()
        self.scale_changed.emit(scale_val)

    @staticmethod
    def _preset_btn_style(active: bool) -> str:
        if active:
            return """
                QPushButton {
                    background: #00D4FF;
                    color: rgb(18, 24, 38);
                    border: none;
                    border-radius: 6px;
                    padding: 6px 0;
                    font-weight: 700;
                    font-size: 12px;
                }
            """
        return """
            QPushButton {
                background: rgb(30, 40, 62);
                color: #8891A7;
                border: 1px solid rgb(40, 50, 80);
                border-radius: 6px;
                padding: 6px 0;
                font-weight: 600;
                font-size: 12px;
            }
            QPushButton:hover {
                background: rgb(40, 54, 84);
                color: #F0F4FF;
            }
        """

    # ------------------------------------------------------------------
    # 공개 API
    # ------------------------------------------------------------------

    def refresh_current_steam_indicator(self):
        current_sid = get_most_recent_steam_id()
        if current_sid != self._last_sid:
            self._refresh_varchive_tab_content()
        else:
            for sid in self._session_rows_by_sid:
                self._apply_current_style(sid, sid == current_sid)

    def show_window(self):
        self.refresh_current_steam_indicator()
        self.show()
        self.activateWindow()
        self.raise_()

    # ------------------------------------------------------------------
    # 드래그
    # ------------------------------------------------------------------

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = True
            self._drag_pos = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
            self.setCursor(Qt.CursorShape.ClosedHandCursor)

    def mouseMoveEvent(self, event):
        if self._dragging:
            self.move(event.globalPosition().toPoint() - self._drag_pos)

    def mouseReleaseEvent(self, event):
        if self._dragging:
            self._dragging = False
            self.setCursor(Qt.CursorShape.ArrowCursor)

    def paintEvent(self, event):
        # 그림자 효과를 위한 여백 공간 유지 (WA_TranslucentBackground 환경)
        pass
