"""PyQt6 settings window with Overlay-like style."""

from PyQt6.QtCore import Qt, pyqtSignal, QPoint
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
)

from settings import SETTINGS, save_settings
from core.version import APP_VERSION
from constants import SCALE_PRESETS
from overlay.settings_varchive import VArchiveSettingsMixin





class SettingsWindow(VArchiveSettingsMixin, QWidget):
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

        self.main_frame = self._build_main_frame()
        layout = QVBoxLayout(self.main_frame)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        layout.addWidget(self._build_header())
        layout.addLayout(self._build_content_layout())

        root.addWidget(self.main_frame)

    def _build_main_frame(self) -> QFrame:
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
        return self.main_frame

    def _build_content_layout(self) -> QVBoxLayout:
        content_layout = QVBoxLayout()
        content_layout.setContentsMargins(15, 15, 15, 15)
        self.tabs = self._build_tabs()
        content_layout.addWidget(self.tabs)
        return content_layout

    def _build_tabs(self) -> QTabWidget:
        tabs = QTabWidget()
        tabs.setStyleSheet("""
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
        tabs.addTab(self._build_ui_tab(), "UI")
        tabs.addTab(self._build_varchive_tab(), "V-Archive")
        tabs.addTab(self._build_system_tab(), "System")
        return tabs

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
