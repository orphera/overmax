from PyQt6.QtWidgets import QWidget
from PyQt6.QtCore import Qt, QRect
from PyQt6.QtGui import QColor, QPainter, QFont, QPen

from capture.roi_manager import ROIManager

class RoiOverlayWindow(QWidget):
    """게임 화면 위에 OCR/검출 ROI를 선으로 표시하는 디버그 오버레이"""
    def __init__(self):
        super().__init__()
        self._enabled = False
        self._has_rect = False
        self.roiman = ROIManager()
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
        self.roiman.update_window_size(width, height)
        
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

    def _roi_to_qrect(self, x1: int, y1: int, x2: int, y2: int) -> QRect:
        return QRect(x1, y1, x2 - x1, y2 - y1)

    def _draw_box(self, painter: QPainter, rect: QRect, color: QColor, label: str):
        painter.setPen(QPen(color, 2))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRect(rect)
        painter.setPen(QPen(color, 1))
        painter.setFont(QFont("Consolas", 9, QFont.Weight.Bold))
        # 텍스트가 박스 안에 잘 보이도록 위치 조정
        painter.drawText(rect.left() + 4, max(12, rect.top() - 4), label)

    def paintEvent(self, event):
        if not self._enabled or not self._has_rect:
            return
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # 1. 로고 ROI
        self._draw_box(
            painter,
            self._roi_to_qrect(*self.roiman.get_roi("logo")),
            QColor("#CC66FF"),
            "LOGO (FREESTYLE)",
        )

        # 2. 재킷 ROI
        self._draw_box(
            painter,
            self._roi_to_qrect(*self.roiman.get_roi("jacket")),
            QColor("#FF0000"),
            "JACKET",
        )

        # 3. 버튼 모드 감지 영역
        self._draw_box(
            painter,
            self._roi_to_qrect(*self.roiman.get_roi("btn_mode")),
            QColor("#00FF88"),
            "BTN MODE",
        )

        # 4. Rate OCR 영역
        self._draw_box(
            painter,
            self._roi_to_qrect(*self.roiman.get_roi("rate")),
            QColor("#00AAFF"),
            "RATE OCR",
        )

        # 5. 난이도별 감지 영역
        for diff in ["NM", "HD", "MX", "SC"]:
            self._draw_box(
                painter,
                self._roi_to_qrect(*self.roiman.get_roi("diff_panel") if diff == "NM" else self.roiman.get_diff_panel_roi(diff)),
                QColor("#FFAA00"),
                diff,
            )
