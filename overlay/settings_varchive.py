"""V-Archive account controls for the settings window."""

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QCheckBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from data.steam_session import get_all_steam_sessions, get_most_recent_steam_id
from settings import SETTINGS, save_settings


class VArchiveSettingsMixin:
    """SettingsWindow mixin that owns Steam account and V-Archive controls."""

    def _build_varchive_tab(self) -> QWidget:
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
        self._clear_layout(self._varchive_main_layout)
        self._session_rows_by_sid = {}
        self._session_labels_by_sid = {}
        self._session_names_by_sid = {}

        self._add_varchive_description_row()
        sessions = get_all_steam_sessions()
        if not sessions:
            self._varchive_main_layout.addWidget(QLabel("발견된 Steam 계정이 없습니다."))
            return

        user_map = self._ensure_varchive_user_map()
        current_sid = get_most_recent_steam_id()
        self._last_sid = current_sid
        current_session, other_sessions = self._split_current_session(sessions, current_sid)

        if current_session:
            self._varchive_main_layout.addWidget(
                self._build_session_row(current_session, user_map, is_current=True)
            )
        if other_sessions:
            self._add_other_sessions(other_sessions, user_map)

    def _clear_layout(self, layout):
        if layout is None:
            return
        while layout.count():
            item = layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                self._clear_layout(item.layout())

    def _add_varchive_description_row(self):
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

    def _ensure_varchive_user_map(self) -> dict:
        SETTINGS.setdefault("varchive", {})
        SETTINGS["varchive"].setdefault("user_map", {})
        return SETTINGS["varchive"]["user_map"]

    def _split_current_session(self, sessions, current_sid: str | None):
        current_session = next((s for s in sessions if s.steam_id == current_sid), None)
        if not current_session and sessions:
            current_session = sessions[0]
            current_sid = current_session.steam_id
            self._last_sid = current_sid
        other_sessions = [s for s in sessions if s.steam_id != current_sid]
        return current_session, other_sessions

    def _add_other_sessions(self, other_sessions, user_map):
        self._varchive_main_layout.addSpacing(10)
        self.toggle_others_btn = self._build_toggle_others_button(len(other_sessions))
        self._varchive_main_layout.addWidget(self.toggle_others_btn)

        self.others_container = QWidget()
        self.others_container.setStyleSheet("background: transparent;")
        others_layout = QVBoxLayout(self.others_container)
        others_layout.setContentsMargins(0, 0, 0, 0)
        others_layout.setSpacing(10)
        others_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        for session in other_sessions:
            others_layout.addWidget(self._build_session_row(session, user_map, is_current=False))

        self.others_container.setVisible(False)
        self._varchive_main_layout.addWidget(self.others_container)
        self.toggle_others_btn.clicked.connect(self._toggle_others)

    def _build_toggle_others_button(self, count: int) -> QPushButton:
        btn = QPushButton(f"다른 계정 보기 ({count}) ▾")
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setStyleSheet("""
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
        return btn

    def _toggle_others(self):
        is_visible = self.others_container.isVisible()
        self.others_container.setVisible(not is_visible)
        text = self.toggle_others_btn.text()
        if "▾" in text:
            self.toggle_others_btn.setText(text.replace("▾", "▴"))
        else:
            self.toggle_others_btn.setText(text.replace("▴", "▾"))

    def _build_session_row(self, session, user_map, is_current: bool) -> QFrame:
        entry = user_map.get(session.steam_id, {})
        v_id = entry.get("v_id", "") if isinstance(entry, dict) else ""
        account_path = entry.get("account_path", "") if isinstance(entry, dict) else ""

        row = QFrame()
        row.setObjectName(f"SessionRow_{session.steam_id}")
        row.setMinimumHeight(160)
        row_layout = QVBoxLayout(row)
        row_layout.setContentsMargins(12, 12, 12, 12)
        self._session_rows_by_sid[session.steam_id] = row

        self._add_session_info_row(row_layout, session)
        self._add_varchive_fetch_row(row_layout, session, v_id)
        self._add_account_path_row(row_layout, session, account_path)

        self._apply_current_style(session.steam_id, is_current)
        return row

    def _add_session_info_row(self, row_layout, session):
        info_row = QHBoxLayout()
        base_label = f"{session.persona_name} ({session.account_name})"
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

        self._session_labels_by_sid[session.steam_id] = name_label
        self._session_names_by_sid[session.steam_id] = base_label

    def _add_varchive_fetch_row(self, row_layout, session, v_id: str):
        input_row = QHBoxLayout()
        edit = QLineEdit()
        edit.setPlaceholderText("V-Archive ID")
        edit.setText(v_id)
        edit.setStyleSheet(self._line_edit_style())
        edit.textChanged.connect(
            lambda text, sid=session.steam_id: self._on_v_id_changed(sid, text)
        )
        input_row.addWidget(edit, 3)

        btn_layout = QHBoxLayout()
        btn_layout.setSpacing(4)
        for button in [4, 5, 6, 8]:
            btn = self._build_fetch_button(f"{button}B", 30)
            btn.clicked.connect(
                lambda _, sid=session.steam_id, vid_edit=edit, btn_val=button:
                self.fetch_varchive_requested.emit(sid, vid_edit.text().strip(), btn_val)
            )
            btn_layout.addWidget(btn)

        all_btn = self._build_fetch_button("All", 35, is_all=True)
        all_btn.clicked.connect(
            lambda _, sid=session.steam_id, vid_edit=edit:
            self.fetch_varchive_requested.emit(sid, vid_edit.text().strip(), 0)
        )
        btn_layout.addWidget(all_btn)

        input_row.addLayout(btn_layout, 2)
        row_layout.addLayout(input_row)

    def _add_account_path_row(self, row_layout, session, account_path: str):
        row_layout.addSpacing(8)
        account_label = QLabel("V-Archive account.txt")
        account_label.setStyleSheet("color: #8891A7; font-size: 11px; margin-top: 4px;")
        row_layout.addWidget(account_label)

        account_row = QHBoxLayout()
        account_edit = QLineEdit()
        account_edit.setPlaceholderText("account.txt 경로")
        account_edit.setText(account_path)
        account_edit.setReadOnly(True)
        account_edit.setStyleSheet(self._line_edit_style())
        account_row.addWidget(account_edit, 1)

        browse_btn = self._build_fetch_button("찾아보기", 64)
        browse_btn.clicked.connect(
            lambda _, sid=session.steam_id, e=account_edit: self._on_browse_account(sid, e)
        )
        account_row.addWidget(browse_btn)

        sync_btn = self._build_fetch_button("동기화 후보", 85, is_all=True)
        sync_btn.clicked.connect(
            lambda _, sid=session.steam_id, name=session.persona_name, path=account_edit:
            self.sync_requested.emit(sid, name, path.text().strip())
        )
        account_row.addWidget(sync_btn)
        row_layout.addLayout(account_row)

    def _build_fetch_button(self, text: str, width: int, is_all: bool = False) -> QPushButton:
        btn = QPushButton(text)
        btn.setFixedSize(width, 24 if width <= 35 else 28)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setStyleSheet(self._fetch_btn_style(is_all=is_all))
        return btn

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
        self.account_file_changed.emit(steam_id, path)

    def _on_auto_refresh_toggled(self, checked: bool):
        SETTINGS.setdefault("varchive", {})
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
        path, _ = QFileDialog.getOpenFileName(
            self, "account.txt 선택", "", "Text Files (*.txt);;All Files (*)"
        )
        if not path:
            return
        account_edit.setText(path)
        self._on_account_path_changed(steam_id, path)

    @staticmethod
    def _line_edit_style() -> str:
        return """
            QLineEdit {
                background: rgb(24, 32, 50);
                border: 1px solid rgb(60, 75, 110);
                border-radius: 4px;
                color: #F0F4FF;
                padding: 4px 8px;
            }
        """

    def refresh_current_steam_indicator(self):
        current_sid = get_most_recent_steam_id()
        if current_sid != self._last_sid:
            self._refresh_varchive_tab_content()
            return

        for sid in self._session_rows_by_sid:
            self._apply_current_style(sid, sid == current_sid)
