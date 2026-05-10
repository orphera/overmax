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
set "VENV_DIR=%PROJECT_DIR%.venv_build"
set "PYTHON_EXE=%VENV_DIR%\Scripts\python.exe"
set "UPX_DIR=%PROJECT_DIR%tools\upx"
set "RUST_CV_DIR=%PROJECT_DIR%rust\overmax_cv"

echo.
echo  ===================================
echo   Overmax - Build Script
echo  ===================================
echo.

:: --------------------------------------------------
:: 1. Setup Build Environment
:: --------------------------------------------------
echo [1/7] Setting up build environment...

:: Check if build venv exists
if not exist "%VENV_DIR%" (
    echo        Creating build venv: %VENV_DIR%
    python -m venv "%VENV_DIR%"
    if errorlevel 1 (
        echo [ERROR] Failed to create venv.
        pause
        exit /b 1
    )
)

:: Verify python in venv
if not exist "%PYTHON_EXE%" (
    echo [ERROR] Python not found in venv: %PYTHON_EXE%
    pause
    exit /b 1
)

for /f "tokens=2" %%v in ('"%PYTHON_EXE%" --version') do set "PY_VER=%%v"
echo        Python %PY_VER% (Build Venv) OK

:: --------------------------------------------------
:: 2. Install dependencies
:: --------------------------------------------------
echo [2/7] Installing dependencies in build venv...

:: Update pip
"%PYTHON_EXE%" -m pip install --upgrade pip --quiet

:: Install/Update requirements
echo        Checking requirements (this may take a while)
"%PYTHON_EXE%" -m pip install -r "%PROJECT_DIR%requirements.txt" --quiet
if errorlevel 1 goto :pip_error

:: Ensure PyInstaller is installed
"%PYTHON_EXE%" -c "import PyInstaller" >nul 2>&1
if errorlevel 1 (
    echo        Installing PyInstaller
    "%PYTHON_EXE%" -m pip install pyinstaller --quiet
    if errorlevel 1 goto :pip_error
)

:: Ensure maturin is installed for the Rust/PyO3 extension
"%PYTHON_EXE%" -c "import maturin" >nul 2>&1
if errorlevel 1 (
    echo        Installing maturin
    "%PYTHON_EXE%" -m pip install maturin --quiet
    if errorlevel 1 goto :pip_error
)

echo        Dependencies OK

:: --------------------------------------------------
:: 3. Build Rust extension
:: --------------------------------------------------
echo [3/7] Building Rust extension...

where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust cargo not found in PATH. Install Rust before building.
    pause
    exit /b 1
)

if not exist "%RUST_CV_DIR%\Cargo.toml" (
    echo [ERROR] Rust extension not found: %RUST_CV_DIR%
    pause
    exit /b 1
)

pushd "%RUST_CV_DIR%"
set "VIRTUAL_ENV=%VENV_DIR%"
"%PYTHON_EXE%" -m maturin develop --release
set "RUST_BUILD_RESULT=%ERRORLEVEL%"
popd

if not "%RUST_BUILD_RESULT%"=="0" (
    echo [ERROR] Rust extension build failed.
    pause
    exit /b 1
)
echo        Rust extension OK

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

:: Detect/Setup UPX
set "UPX_CMD="
if exist "%UPX_DIR%\upx.exe" (
    echo        UPX detected at %UPX_DIR%
    set "UPX_CMD=--upx-dir="%UPX_DIR%""
) else (
    :: Try to see if UPX is in PATH
    upx --version >nul 2>&1
    if not errorlevel 1 (
        echo        UPX detected in PATH
    ) else (
        echo        UPX not found. Attempting automatic setup...
        if not exist "%UPX_DIR%" mkdir "%UPX_DIR%"
        
        :: Download UPX 4.2.4 (stable) via PowerShell
        powershell -NoProfile -Command ^
            "$url = 'https://github.com/upx/upx/releases/download/v4.2.4/upx-4.2.4-win64.zip';" ^
            "$zip = '%PROJECT_DIR%upx_tmp.zip';" ^
            "echo '       Downloading UPX...';" ^
            "try { Invoke-WebRequest -Uri $url -OutFile $zip -ErrorAction Stop } catch { echo '       [WARN] Failed to download UPX.'; exit 1 };" ^
            "echo '       Extracting...';" ^
            "Expand-Archive -Path $zip -DestinationPath '%UPX_DIR%' -Force;" ^
            "Get-ChildItem -Path '%UPX_DIR%\*\upx.exe' | Move-Item -Destination '%UPX_DIR%' -Force;" ^
            "Remove-Item -Path '%UPX_DIR%\upx-4.2.4-win64' -Recurse -ErrorAction SilentlyContinue;" ^
            "Remove-Item -Path $zip -ErrorAction SilentlyContinue;"
            
        if exist "%UPX_DIR%\upx.exe" (
            echo        UPX setup complete.
            set "UPX_CMD=--upx-dir="%UPX_DIR%""
        ) else (
            echo        [WARN] UPX setup failed. Proceeding without UPX.
        )
    )
)

if "%DEBUG_MODE%"=="1" (
    echo        [DEBUG MODE] Console window will be visible
    "%PYTHON_EXE%" -c "content=open('overmax.spec').read();open('overmax_debug.spec','w').write(content.replace('console=False','console=True'))"
    "%PYTHON_EXE%" -m PyInstaller overmax_debug.spec --noconfirm %UPX_CMD%
    del overmax_debug.spec
) else (
    "%PYTHON_EXE%" -m PyInstaller overmax.spec --noconfirm %UPX_CMD%
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

for /f "usebackq delims=" %%v in (`%PYTHON_EXE% -c "from core.version import APP_VERSION; print(APP_VERSION)"`) do set "APP_VERSION=%%v"
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
