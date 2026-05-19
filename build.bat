@echo off
setlocal enabledelayedexpansion

echo.
echo  ===================================
echo   Overmax - Build Script (Rust)
echo  ===================================
echo.

where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust cargo not found in PATH. Please install Rust (rustup).
    pause
    exit /b 1
)

echo [1/2] Building Overmax App (Release)...
cargo build -p overmax-app --release
if errorlevel 1 (
    echo [ERROR] Rust build failed.
    pause
    exit /b 1
)

echo [2/2] Packaging...
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-rust.ps1
if errorlevel 1 (
    echo [ERROR] Packaging failed.
    pause
    exit /b 1
)

echo.
echo Build Successful!
echo Output: dist/overmax-rust/
echo.
pause
