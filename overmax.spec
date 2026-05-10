# -*- mode: python ; coding: utf-8 -*-
import sys
import os
from pathlib import Path

# ------------------------------------------------------------------
# 분석 (현재 런타임 의존성 기준)
# ------------------------------------------------------------------
a = Analysis(
    ["main.py"],
    pathex=[str(Path(".").resolve())],
    binaries=[], # torch_libs 제거 
    datas=[
        ("settings.json", "."),
        ("version_info.txt", "."),
    ],    # easyocr_datas 제거 
    hiddenimports=[
        "PyQt6.QtCore",
        "PyQt6.QtGui",
        "PyQt6.QtWidgets",
        "winrt.windows.media.ocr",
        "winrt.windows.graphics.imaging",
        "winrt.windows.storage.streams",
        "winrt.windows.globalization",
        "winrt.windows.foundation",
        "winrt.windows.foundation.collections",
        "mss",
        "mss.windows",
        "win32gui",
        "win32con",
        "win32api",
        "rapidfuzz",
        "httpx",
        "numpy",
    ],
    hookspath=[], # 커스텀 훅 불필요
    excludes=[
        "torch",      # 절대 포함되지 않도록 명시 [cite: 30]
        "torchvision",
        "easyocr",    # 제거 [cite: 30]
        "matplotlib",
        "pandas",
        # Qt 미사용 모듈들
        "PyQt6.QtNetwork",
        "PyQt6.QtSql",
        "PyQt6.QtXml",
        "PyQt6.QtBluetooth",
        "PyQt6.QtMultimedia",
        "PyQt6.QtWebEngine",
        "PyQt6.QtTest",
        "PyQt6.Qt3D",
    ],
    noarchive=False,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="overmax",
    debug=False,
    console=False, # 실배포용 [cite: 34]
    upx=True,
    version='version_info.txt',
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    name="overmax",
)
