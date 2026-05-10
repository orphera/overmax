import sys
import os
from pathlib import Path

def get_base_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys._MEIPASS)
    return Path(__file__).parent

def get_data_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).parent
    return Path(__file__).parent

def apply_all():
    # EasyOCR 모델 패치 제거됨
    if getattr(sys, "frozen", False):
        base = get_base_dir()
        qt_plugins = base / "PyQt6" / "Qt6" / "plugins"
        if qt_plugins.exists():
            os.environ["QT_PLUGIN_PATH"] = str(qt_plugins)

apply_all()
