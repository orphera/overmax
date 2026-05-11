"""Minimal PySide6 overlay spike.

This script is intentionally separate from production overlay code. It lets us
check whether PySide6 can satisfy the same basic window constraints as PyQt6.
"""

from __future__ import annotations

import argparse
import ctypes
import sys

WDA_EXCLUDEFROMCAPTURE = 0x00000011


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--show", action="store_true")
    return parser.parse_args()


def import_pyside6():
    try:
        from PySide6.QtCore import Qt, QTimer, Signal, QObject
        from PySide6.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget
    except ImportError as exc:
        raise SystemExit(f"PySide6 import failed: {exc}") from exc

    return QApplication, QLabel, QVBoxLayout, QWidget, Qt, QTimer, Signal, QObject


def set_capture_exclusion(widget) -> bool:
    hwnd = int(widget.winId())
    try:
        ctypes.windll.user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
        return True
    except Exception as exc:
        print(f"SetWindowDisplayAffinity failed: {exc}")
        return False


def build_window(qt_modules):
    QApplication, QLabel, QVBoxLayout, QWidget, Qt, QTimer, *_ = qt_modules
    app = QApplication(sys.argv)
    window = QWidget()
    window.setWindowTitle("Overmax PySide6 smoke")
    window.setWindowFlags(
        Qt.WindowType.FramelessWindowHint
        | Qt.WindowType.WindowStaysOnTopHint
        | Qt.WindowType.Tool
    )
    window.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
    window.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
    window.setFixedWidth(320)

    layout = QVBoxLayout(window)
    label = QLabel("PySide6 overlay smoke")
    label.setStyleSheet(
        "color: white; background: rgba(20, 20, 20, 190);"
        "padding: 18px; border-radius: 10px;"
    )
    layout.addWidget(label)

    window.show()
    capture_excluded = set_capture_exclusion(window)
    QTimer.singleShot(2000, app.quit)
    return app, capture_excluded


def main() -> int:
    args = parse_args()
    qt_modules = import_pyside6()

    if args.import_only:
        print("PySide6 import ok")
        return 0

    if not args.show:
        print("Use --import-only or --show")
        return 2

    app, capture_excluded = build_window(qt_modules)
    print(f"capture_excluded={capture_excluded}")
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
