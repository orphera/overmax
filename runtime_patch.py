"""
runtime_patch.py
PyInstaller로 패키징 시 경로 문제를 해결하는 런타임 패치

main.py 최상단에서 import 해야 함
"""

import sys
import os
from pathlib import Path


def get_base_dir() -> Path:
    """
    실행 환경에 따라 베이스 디렉토리 반환
    - 개발 환경: 스크립트 파일 위치
    - PyInstaller: sys._MEIPASS (압축 해제 임시 폴더)
    """
    if getattr(sys, "frozen", False):
        # PyInstaller로 패키징된 경우
        return Path(sys._MEIPASS)
    else:
        return Path(__file__).parent


def get_data_dir() -> Path:
    """
    사용자 데이터 디렉토리 (EXE와 같은 위치)
    - songs.json 캐시, 설정 파일 등
    """
    if getattr(sys, "frozen", False):
        # EXE 파일이 있는 폴더
        return Path(sys.executable).parent
    else:
        return Path(__file__).parent


def patch_easyocr_model_path():
    """
    EasyOCR 모델 캐시를 EXE 옆 폴더로 고정
    기본값: C:\\Users\\{user}\\.EasyOCR
    변경값: {exe_dir}\\models
    """
    data_dir = get_data_dir()
    model_dir = data_dir / "models"
    model_dir.mkdir(exist_ok=True)

    # EasyOCR이 환경변수로 모델 경로를 받음
    os.environ["EASYOCR_MODULE_PATH"] = str(model_dir)


def patch_cv2():
    """
    OpenCV가 PyInstaller 환경에서 플러그인을 못 찾는 문제 방지
    """
    base = get_base_dir()
    qt_plugin_path = base / "cv2" / "qt" / "plugins"
    if qt_plugin_path.exists():
        os.environ["QT_QPA_PLATFORM_PLUGIN_PATH"] = str(qt_plugin_path)


def apply_all():
    patch_easyocr_model_path()
    patch_cv2()

    # PyQt6 플러그인 경로 (PyInstaller 환경)
    if getattr(sys, "frozen", False):
        base = get_base_dir()
        qt_plugins = base / "PyQt6" / "Qt6" / "plugins"
        if qt_plugins.exists():
            os.environ["QT_PLUGIN_PATH"] = str(qt_plugins)


# 모듈 임포트 시 자동 적용
apply_all()
