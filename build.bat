@echo off
:: ============================================================
::  build.bat - Overmax build script
::  Usage: build.bat [--debug]
:: ============================================================

setlocal enabledelayedexpansion
set "PROJECT_DIR=%~dp0"
set "DIST_DIR=%PROJECT_DIR%dist\overmax"
set "DEBUG_MODE=0"

if "%1"=="--debug" set "DEBUG_MODE=1"

echo.
echo  ===================================
echo   Overmax - Build Script v0.1
echo  ===================================
echo.

:: --------------------------------------------------
:: 1. Check Python
:: --------------------------------------------------
echo [1/6] Checking Python...
python --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Python not found.
    echo         Install Python 3.10+ from https://www.python.org
    pause
    exit /b 1
)
for /f "tokens=2" %%v in ('python --version') do set "PY_VER=%%v"
echo        Python %PY_VER% OK

:: --------------------------------------------------
:: 2. Check / install dependencies
:: --------------------------------------------------
echo [2/6] Checking dependencies...
python -c "import PyInstaller" >nul 2>&1
if errorlevel 1 (
    echo        Installing PyInstaller
    pip install pyinstaller --quiet
    if errorlevel 1 goto :pip_error
)

python -c "import PyQt6" >nul 2>&1
if errorlevel 1 (
    echo        Installing requirements.txt (this may take a while)
    pip install -r "%PROJECT_DIR%requirements.txt" --quiet
    if errorlevel 1 goto :pip_error
)
echo        Dependencies OK

:: --------------------------------------------------
:: 3. EasyOCR model check (Removed - Using Windows OCR)
:: --------------------------------------------------
echo [3/6] Skipping model check (Using Windows Native OCR)...

:: --------------------------------------------------
:: 4. Clean previous build
:: --------------------------------------------------
echo [4/6] Cleaning previous build...
if exist "%PROJECT_DIR%dist" (
    rmdir /s /q "%PROJECT_DIR%dist"
)
if exist "%PROJECT_DIR%build" (
    rmdir /s /q "%PROJECT_DIR%build"
)
echo        Done

:: --------------------------------------------------
:: 5. Run PyInstaller
:: --------------------------------------------------
echo [5/6] Running PyInstaller...

if "%DEBUG_MODE%"=="1" (
    echo        [DEBUG MODE] Console window will be visible
    python -c "content=open('overmax.spec').read();open('overmax_debug.spec','w').write(content.replace('console=False','console=True'))"
    pyinstaller overmax_debug.spec --noconfirm
    del overmax_debug.spec
) else (
    pyinstaller overmax.spec --noconfirm
)

if errorlevel 1 (
    echo [ERROR] PyInstaller build failed.
    pause
    exit /b 1
)

:: --------------------------------------------------
:: 6. Post-process
:: --------------------------------------------------
echo [6/6] Post-processing...

if not exist "%DIST_DIR%\cache" mkdir "%DIST_DIR%\cache"

if exist "%PROJECT_DIR%settings.json" (
    copy /y "%PROJECT_DIR%settings.json" "%DIST_DIR%\settings.json" >nul
    echo        settings.json included
) else (
    echo        settings.json not found - defaults will be used
)

if exist "%PROJECT_DIR%cache\songs.json" (
    copy /y "%PROJECT_DIR%cache\songs.json" "%DIST_DIR%\cache\songs.json" >nul
    echo        songs.json included
) else (
    echo        songs.json not found - will download on first run
)

copy /y "%PROJECT_DIR%README.md" "%DIST_DIR%\README.md" >nul

:: --------------------------------------------------
:: Done
:: --------------------------------------------------
echo.
echo ============================================================
echo  Build complete!
echo  Output: dist\overmax\overmax.exe
echo.
echo  Distribute the entire dist\overmax\ folder.
echo ============================================================
echo.

pause
exit /b 0

:pip_error
echo [ERROR] pip install failed. Check your internet connection.
pause
exit /b 1
