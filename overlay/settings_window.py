"""PyQt6 settings window with Overlay-like style."""

from PyQt6.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QHBoxLayout,
    QTabWidget,
    QLabel,
    QSlider,
    QPushButton,
    QFrame,
)
from PyQt6.QtCore import Qt, pyqtSignal, QPoint
from PyQt6.QtGui import QColor, QPainter, QBrush

from settings import SETTINGS, save_settings


class SettingsWindow(QWidget):
    """애플리케이션 설정 창 - 오버레이와 통일된 디자인 적용."""
    
    opacity_changed = pyqtSignal(float)

    def __init__(self):
        super().__init__()
        self._dragging = False
        self._drag_pos = QPoint()
        
        self._setup_window()
        self._setup_ui()

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint |
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setMinimumSize(400, 350)

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
        layout.setSpacing(20)
        layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        # 오버레이 투명도 설정
        opacity_cfg = SETTINGS.get("overlay", {})
        base_val = opacity_cfg.get("base_opacity", 0.8)
        
        group_layout = QVBoxLayout()
        
        label_layout = QHBoxLayout()
        label_layout.addWidget(QLabel("오버레이 투명도"))
        self.opacity_val_label = QLabel(f"{base_val:.1f}")
        self.opacity_val_label.setStyleSheet("color: #00D4FF; font-weight: bold; font-size: 14px;")
        label_layout.addWidget(self.opacity_val_label)
        label_layout.addStretch()
        group_layout.addLayout(label_layout)
        
        self.opacity_slider = QSlider(Qt.Orientation.Horizontal)
        self.opacity_slider.setMinimum(1)
        self.opacity_slider.setMaximum(10)
        self.opacity_slider.setValue(int(base_val * 10))
        self.opacity_slider.setStyleSheet("""
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
        self.opacity_slider.valueChanged.connect(self._on_opacity_slider_changed)
        group_layout.addWidget(self.opacity_slider)
        
        layout.addLayout(group_layout)
        return tab

    def _on_opacity_slider_changed(self, value: int):
        float_val = value / 10.0
        self.opacity_val_label.setText(f"{float_val:.1f}")
        
        if "overlay" not in SETTINGS:
            SETTINGS["overlay"] = {}
        SETTINGS["overlay"]["base_opacity"] = float_val
        save_settings()
        self.opacity_changed.emit(float_val)

    def show_window(self):
        self.show()
        self.activateWindow()
        self.raise_()

    # ------------------------------------------------------------------
    # 드래그 구현
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
