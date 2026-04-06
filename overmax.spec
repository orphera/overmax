# -*- mode: python ; coding: utf-8 -*-
import sys
import os
from pathlib import Path

# ------------------------------------------------------------------
# 분석 (EasyOCR/Torch 관련 데이터 및 라이브러리 제거)
# ------------------------------------------------------------------
a = Analysis(
    ["main.py"],
    pathex=[str(Path(".").resolve())],
    binaries=[], # torch_libs 제거 
    datas=[
        ("settings.json", "."),
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
        "cv2",
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
        "scipy",
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
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    name="overmax",
)
