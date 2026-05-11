"""System tray construction for the overlay runtime."""

from PyQt6.QtGui import QAction
from PyQt6.QtWidgets import QMenu, QStyle, QSystemTrayIcon

from constants import TOGGLE_HOTKEY, TRAY_TOOLTIP


def create_overlay_tray_icon(app, window, settings_window, debug_toggle_cb=None):
    if not QSystemTrayIcon.isSystemTrayAvailable():
        print("[Overlay] 시스템 트레이를 사용할 수 없음")
        return None

    tray_icon = QSystemTrayIcon(app)
    tray_icon.setIcon(app.style().standardIcon(QStyle.StandardPixmap.SP_ComputerIcon))
    tray_icon.setToolTip(TRAY_TOOLTIP)
    tray_icon.setContextMenu(_build_tray_menu(app, window, settings_window, debug_toggle_cb))
    tray_icon.activated.connect(lambda reason: _on_tray_activated(reason, window))
    tray_icon.show()
    return tray_icon


def _build_tray_menu(app, window, settings_window, debug_toggle_cb):
    tray_menu = QMenu()
    toggle_action = QAction(f"오버레이 표시/숨김 ({TOGGLE_HOTKEY})", app)
    toggle_action.triggered.connect(window.toggle_visibility)
    tray_menu.addAction(toggle_action)

    if debug_toggle_cb is not None:
        debug_action = QAction("디버그 창 표시/숨김", app)
        debug_action.triggered.connect(debug_toggle_cb)
        tray_menu.addAction(debug_action)

    settings_action = QAction("설정", app)
    settings_action.triggered.connect(settings_window.show_window)
    tray_menu.addAction(settings_action)

    tray_menu.addSeparator()
    quit_action = QAction("종료", app)
    quit_action.triggered.connect(app.quit)
    tray_menu.addAction(quit_action)
    return tray_menu


def _on_tray_activated(reason, window):
    if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
        window.toggle_visibility()
