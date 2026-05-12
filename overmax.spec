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
    binaries=[],
    datas=[
        ("settings.json", "."),
        ("version_info.txt", "."),
    ],
    hiddenimports=[
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
    hookspath=[],
    excludes=[
        "PyQt6",
        "PySide6",
        "torch",
        "torchvision",
        "easyocr",
        "matplotlib",
        "pandas",
        "tkinter",
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
    console=False,
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
