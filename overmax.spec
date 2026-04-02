# -*- mode: python ; coding: utf-8 -*-
#
# overmax.spec
# PyInstaller 스펙 파일
#
# 사용법:
#   pyinstaller overmax.spec
#
# 결과물:
#   dist/overmax/overmax.exe  (--onedir, 권장)

import sys
import os
from pathlib import Path
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

# ------------------------------------------------------------------
# EasyOCR 데이터 수집
# EasyOCR은 내부적으로 모델 파일과 설정 파일을 패키지 안에 가지고 있음
# ------------------------------------------------------------------
easyocr_datas = collect_data_files("easyocr")

# torch (EasyOCR 의존) 동적 라이브러리
torch_libs = collect_dynamic_libs("torch")

# ------------------------------------------------------------------
# 분석
# ------------------------------------------------------------------
a = Analysis(
    ["main.py"],
    pathex=[str(Path(".").resolve())],
    binaries=torch_libs,
    datas=[
        # EasyOCR 내장 데이터
        *easyocr_datas,
        # 앱 아이콘 (있을 경우)
        # ("assets/icon.ico", "assets"),
    ],
    hiddenimports=[
        # PyQt6
        "PyQt6.QtCore",
        "PyQt6.QtGui",
        "PyQt6.QtWidgets",
        "PyQt6.sip",

        # EasyOCR / torch 관련 - import 분석에서 놓치는 경우가 많음
        "easyocr",
        "easyocr.easyocr",
        "easyocr.detection",
        "easyocr.recognition",
        "easyocr.utils",
        "torch",
        "torch.nn",
        "torch.nn.functional",
        "torchvision",
        "torchvision.transforms",

        # OpenCV
        "cv2",

        # 기타
        "mss",
        "mss.windows",
        "win32gui",
        "win32con",
        "win32api",
        "rapidfuzz",
        "rapidfuzz.fuzz",
        "rapidfuzz.process",
        "httpx",
        "numpy",
        "PIL",
        "PIL.Image",

        # Python 내장 중 누락되기 쉬운 것
        "difflib",
        "threading",
        "pathlib",
        "dataclasses",
    ],
    hookspath=["."],         # 커스텀 훅 위치 (아래에서 생성)
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        # 불필요한 torch 백엔드 제외 (용량 절감)
        "torch.distributed",
        "torch.testing",
        "torch.utils.tensorboard",
        "torchvision.datasets",

        # 테스트/개발 도구
        "pytest",
        "IPython",
        "jupyter",
        "matplotlib",
        "pandas",
        "scipy",
    ],
    noarchive=False,
)

# ------------------------------------------------------------------
# PYZ 아카이브
# ------------------------------------------------------------------
pyz = PYZ(a.pure)

# ------------------------------------------------------------------
# EXE
# ------------------------------------------------------------------
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,   # onedir 방식 (onefile보다 실행 빠름)
    name="overmax",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,                # UPX 압축 (설치 시)
    console=False,           # 콘솔 창 숨김 (GUI 앱)
    # console=True,          # 디버그 시 이걸로 교체
    icon=None,               # "assets/icon.ico" 로 교체 가능
    version="version_info.txt",  # 버전 정보 (아래에서 생성)
)

# ------------------------------------------------------------------
# COLLECT - 최종 dist/overmax/ 폴더 구성
# ------------------------------------------------------------------
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[
        "vcruntime140.dll",  # UPX 압축 시 깨지는 DLL들
        "python3*.dll",
        "Qt6*.dll",
    ],
    name="overmax",
)
