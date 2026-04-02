# hook-easyocr.py
# PyInstaller가 EasyOCR의 숨겨진 의존성을 찾도록 도와주는 커스텀 훅
#
# 이 파일은 overmax.spec의 hookspath=["."] 설정으로 자동으로 로드됨

from PyInstaller.utils.hooks import collect_data_files, collect_submodules

# EasyOCR 서브모듈 전체 수집
hiddenimports = collect_submodules("easyocr")

# 데이터 파일 (config, 언어팩 등)
datas = collect_data_files("easyocr")
