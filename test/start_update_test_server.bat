@echo off
setlocal

set "ROOT_DIR=%~dp0.."
set "FEED_DIR=%ROOT_DIR%\cache\update_test_feed"
set "PORT=8765"

if not exist "%FEED_DIR%" (
    echo [ERROR] %FEED_DIR% not found.
    echo         Run: python scripts\prepare_update_test_feed.py
    pause
    exit /b 1
)

echo [LocalUpdateTest] serving: %FEED_DIR%
echo [LocalUpdateTest] url: http://127.0.0.1:%PORT%/
python -m http.server %PORT% --directory "%FEED_DIR%"
