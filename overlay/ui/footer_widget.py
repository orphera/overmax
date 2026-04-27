try:
    from PyQt6.QtWidgets import QFrame, QHBoxLayout, QLabel
    from PyQt6.QtCore import Qt
except ImportError:
    pass

def _s(base: int, scale: float) -> int:
    return max(1, round(base * scale))

class FooterWidget(QFrame):
    def __init__(self, scale: float = 1.0, parent=None):
        super().__init__(parent)
        self._scale = scale
        self._build_ui()

    def _build_ui(self):
        sc = self._scale
        self.setStyleSheet(f"""
            QFrame {{
                background: rgb(22, 30, 48);
                border-radius: {_s(8, sc)}px;
            }}
        """)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(_s(10, sc), _s(5, sc), _s(10, sc), _s(5, sc))

        label = QLabel("유사 구간 평균")
        label.setStyleSheet(f"color: #505870; font-size: {_s(10, sc)}px;")
        layout.addWidget(label)
        layout.addStretch()

        self._avg_rate_label = QLabel("——")
        self._avg_rate_label.setStyleSheet(
            f"color: #505870; font-size: {_s(11, sc)}px; font-weight: 700;"
        )
        layout.addWidget(self._avg_rate_label)

        splitter = QLabel(" | ")
        splitter.setStyleSheet(f"color: #505870; font-size: {_s(11, sc)}px;")
        layout.addWidget(splitter)

        self._total_count_label = QLabel("0/0개 패턴")
        self._total_count_label.setStyleSheet(
            f"color: #505870; font-size: {_s(11, sc)}px; font-weight: 700;"
        )
        layout.addWidget(self._total_count_label)

    def update_footer(self, avg_rate: float, has_record_count: int, total_count: int):
        sc = self._scale
        if avg_rate < 0.0:
            self._avg_rate_label.setText("——")
            self._avg_rate_label.setStyleSheet(
                f"color: #505870; font-size: {_s(11, sc)}px; font-weight: 700;"
            )
        else:
            color = self._avg_rate_color(avg_rate)
            self._avg_rate_label.setText(f"{avg_rate:.2f}%")
            self._avg_rate_label.setStyleSheet(
                f"color: {color}; font-size: {_s(11, sc)}px; font-weight: 700;"
            )

        total_count_color = "#F0F4FF" if total_count > 0 else "#505870"
        self._total_count_label.setText(f"{has_record_count}/{total_count}개 패턴")
        self._total_count_label.setStyleSheet(
            f"color: {total_count_color}; font-size: {_s(11, sc)}px; font-weight: 700;"
        )

    @staticmethod
    def _avg_rate_color(rate: float) -> str:
        if rate >= 100.0: return "#FFD700"
        if rate >= 99.0:  return "#B8DCFF"
        if rate >= 95.0:  return "#7EC8E3"
        if rate >= 90.0:  return "#B5EAD7"
        return "#FF9999"
