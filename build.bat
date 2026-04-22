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
echo [1/7] Checking Python...
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
echo [2/7] Checking dependencies...
python -c "import PyInstaller" >nul 2>&1
if errorlevel 1 (
    echo        Installing PyInstaller
    python -m pip install pyinstaller --quiet
    if errorlevel 1 goto :pip_error
)

python -c "import PyQt6" >nul 2>&1
if errorlevel 1 (
    echo        Installing requirements.txt (this may take a while)
    python -m pip install -r "%PROJECT_DIR%requirements.txt" --quiet
    if errorlevel 1 goto :pip_error
)
echo        Dependencies OK

:: --------------------------------------------------
:: 3. EasyOCR model check (Removed - Using Windows OCR)
:: --------------------------------------------------
echo [3/7] Skipping model check (Using Windows Native OCR)...

:: --------------------------------------------------
:: 4. Clean previous build
:: --------------------------------------------------
echo [4/7] Cleaning previous build...
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
echo [5/7] Running PyInstaller...

if "%DEBUG_MODE%"=="1" (
    echo        [DEBUG MODE] Console window will be visible
    python -c "content=open('overmax.spec').read();open('overmax_debug.spec','w').write(content.replace('console=False','console=True'))"
    python -m PyInstaller overmax_debug.spec --noconfirm
    del overmax_debug.spec
) else (
    python -m PyInstaller overmax.spec --noconfirm
)

if errorlevel 1 (
    echo [ERROR] PyInstaller build failed.
    pause
    exit /b 1
)

:: --------------------------------------------------
:: 6. Post-process
:: --------------------------------------------------
echo [6/7] Post-processing...

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

if exist "%PROJECT_DIR%cache\image_index.db" (
    copy /y "%PROJECT_DIR%cache\image_index.db" "%DIST_DIR%\cache\image_index.db" >nul
    echo        image_index.db included
) else (
    echo        image_index.db not found - will download on first run
)

copy /y "%PROJECT_DIR%README.md" "%DIST_DIR%\README.md" >nul

:: --------------------------------------------------
:: 7. Build Release Artifacts (overmax.zip + manifest)
:: --------------------------------------------------
echo [7/7] Building release artifacts...
set "ZIP_PATH=%PROJECT_DIR%dist\overmax.zip"
set "MANIFEST_PATH=%PROJECT_DIR%dist\release_manifest.json"

if exist "%ZIP_PATH%" del /f /q "%ZIP_PATH%"
if exist "%MANIFEST_PATH%" del /f /q "%MANIFEST_PATH%"

powershell -NoProfile -Command "Compress-Archive -Path '%DIST_DIR%\*' -DestinationPath '%ZIP_PATH%' -Force"
if errorlevel 1 goto :package_error
echo        overmax.zip created

for /f "usebackq delims=" %%h in (`powershell -NoProfile -Command "(Get-FileHash -Path '%ZIP_PATH%' -Algorithm SHA256).Hash.ToLower()"`) do set "ZIP_SHA256=%%h"
if "%ZIP_SHA256%"=="" goto :package_error

for /f "usebackq delims=" %%v in (`python -c "from core.version import APP_VERSION; print(APP_VERSION)"`) do set "APP_VERSION=%%v"
if "%APP_VERSION%"=="" goto :package_error

powershell -NoProfile -Command "$manifest = @{ version = 'v%APP_VERSION%'; generated_at = (Get-Date).ToUniversalTime().ToString('o'); assets = @(@{ name = 'overmax.zip'; sha256 = '%ZIP_SHA256%' }) }; $manifest | ConvertTo-Json -Depth 5 | Set-Content -Path '%MANIFEST_PATH%' -Encoding UTF8"
if errorlevel 1 goto :package_error
echo        release_manifest.json created

:: --------------------------------------------------
:: Done
:: --------------------------------------------------
echo.
echo ============================================================
echo  Build complete!
echo  Output: dist\overmax\overmax.exe
echo  Release zip: dist\overmax.zip
echo  Manifest: dist\release_manifest.json
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

:package_error
echo [ERROR] Failed to create release artifacts.
pause
exit /b 1
