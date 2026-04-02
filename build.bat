@echo off
:: ============================================================
::  build.bat - Overmax 빌드 스크립트
::  사용법: build.bat [--debug]
:: ============================================================

setlocal enabledelayedexpansion
set "PROJECT_DIR=%~dp0"
set "DIST_DIR=%PROJECT_DIR%dist\overmax"
set "DEBUG_MODE=0"

if "%1"=="--debug" set "DEBUG_MODE=1"

echo.
echo  ██████╗ ██╗   ██╗███████╗██████╗ ███╗   ███╗ █████╗ ██╗  ██╗
echo  ██╔══██╗██║   ██║██╔════╝██╔══██╗████╗ ████║██╔══██╗╚██╗██╔╝
echo  ██║  ██║██║   ██║█████╗  ██████╔╝██╔████╔██║███████║ ╚███╔╝
echo  ██║  ██║╚██╗ ██╔╝██╔══╝  ██╔══██╗██║╚██╔╝██║██╔══██║ ██╔██╗
echo  ██████╔╝ ╚████╔╝ ███████╗██║  ██║██║ ╚═╝ ██║██║  ██║██╔╝ ██╗
echo  ╚═════╝   ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝
echo  Build Script v0.1
echo.

:: --------------------------------------------------
:: 1. Python 확인
:: --------------------------------------------------
echo [1/6] Python 환경 확인...
python --version >nul 2>&1
if errorlevel 1 (
    echo [오류] Python을 찾을 수 없습니다.
    echo        https://www.python.org 에서 Python 3.10 이상을 설치하세요.
    pause
    exit /b 1
)
for /f "tokens=2" %%v in ('python --version') do set "PY_VER=%%v"
echo        Python %PY_VER% 확인됨

:: --------------------------------------------------
:: 2. 의존성 설치 확인
:: --------------------------------------------------
echo [2/6] 의존성 확인 및 설치...
python -c "import PyInstaller" >nul 2>&1
if errorlevel 1 (
    echo        PyInstaller 설치 중...
    pip install pyinstaller --quiet
    if errorlevel 1 goto :pip_error
)

python -c "import PyQt6" >nul 2>&1
if errorlevel 1 (
    echo        requirements.txt 설치 중... (시간이 걸릴 수 있습니다)
    pip install -r "%PROJECT_DIR%requirements.txt" --quiet
    if errorlevel 1 goto :pip_error
)
echo        의존성 OK

:: --------------------------------------------------
:: 3. EasyOCR 모델 사전 다운로드
::    패키징 전에 모델이 캐시에 있어야 함
::    (실행 파일에는 모델 포함 안 됨 - 첫 실행 시 자동 다운로드)
:: --------------------------------------------------
echo [3/6] EasyOCR 모델 확인...
python -c "import easyocr; print('  모델 캐시 확인 중...'); r = easyocr.Reader(['ko', 'en'], gpu=False, verbose=False); print('  EasyOCR 모델 OK')"
if errorlevel 1 (
    echo [경고] EasyOCR 모델 준비 실패 - 첫 실행 시 자동 다운로드됩니다.
)

:: --------------------------------------------------
:: 4. 이전 빌드 정리
:: --------------------------------------------------
echo [4/6] 이전 빌드 정리...
if exist "%PROJECT_DIR%dist" (
    rmdir /s /q "%PROJECT_DIR%dist"
)
if exist "%PROJECT_DIR%build" (
    rmdir /s /q "%PROJECT_DIR%build"
)
echo        정리 완료

:: --------------------------------------------------
:: 5. PyInstaller 실행
:: --------------------------------------------------
echo [5/6] PyInstaller 빌드 중...

if "%DEBUG_MODE%"=="1" (
    echo        [디버그 모드] 콘솔 창 표시됨
    :: spec 파일의 console=False를 일시적으로 True로 패치
    python -c "
content = open('overmax.spec').read()
content = content.replace('console=False', 'console=True')
open('overmax_debug.spec', 'w').write(content)
"
    pyinstaller overmax_debug.spec --noconfirm
    del overmax_debug.spec
) else (
    pyinstaller overmax.spec --noconfirm
)

if errorlevel 1 (
    echo [오류] PyInstaller 빌드 실패
    pause
    exit /b 1
)

:: --------------------------------------------------
:: 6. 후처리 - songs.json 캐시 복사
:: --------------------------------------------------
echo [6/6] 후처리...

:: cache 폴더 생성
if not exist "%DIST_DIR%\cache" mkdir "%DIST_DIR%\cache"

:: songs.json이 있으면 동봉
if exist "%PROJECT_DIR%cache\songs.json" (
    copy /y "%PROJECT_DIR%cache\songs.json" "%DIST_DIR%\cache\songs.json" >nul
    echo        songs.json 포함됨
) else (
    echo        songs.json 없음 - 첫 실행 시 자동 다운로드됩니다
)

:: README 복사
copy /y "%PROJECT_DIR%README.md" "%DIST_DIR%\README.md" >nul

:: --------------------------------------------------
:: 완료
:: --------------------------------------------------
echo.
echo ============================================================
echo  빌드 완료!
echo  실행 파일: dist\overmax\overmax.exe
echo.
echo  배포 시 dist\overmax\ 폴더 전체를 전달하세요.
echo ============================================================
echo.

:: 빌드 결과 크기 출력
for /f "tokens=3" %%s in ('dir /s /-c "%DIST_DIR%" ^| findstr "파일"') do set "SIZE=%%s"
echo  폴더 크기: %SIZE% bytes

pause
exit /b 0

:pip_error
echo [오류] pip 설치 실패. 인터넷 연결을 확인하세요.
pause
exit /b 1
