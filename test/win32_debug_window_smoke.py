"""Smoke checks for the Win32 debug log window candidate."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parents[1]
if str(ROOT_DIR) not in sys.path:
    sys.path.insert(0, str(ROOT_DIR))

from overlay.win32.debug_window import Win32DebugWindow


def run_import_check() -> None:
    print("Win32 debug window import ok")


def run_diagnostics() -> None:
    window = Win32DebugWindow()
    try:
        diagnostics = window.diagnostics()
        print(f"hwnd_created={diagnostics.hwnd_created}")
        print(f"edit_created={diagnostics.edit_created}")
        print(f"capture_excluded={diagnostics.capture_excluded}")
        print(f"filter_count={diagnostics.filter_count}")
        print(f"max_lines={diagnostics.max_lines}")
        if not diagnostics.hwnd_created or not diagnostics.edit_created:
            raise SystemExit(1)
        if diagnostics.filter_count < 5 or diagnostics.max_lines <= 0:
            raise SystemExit(1)
    finally:
        window.hide()


def run_append_check() -> None:
    window = Win32DebugWindow()
    try:
        window.append_log("[Overlay] Win32 debug smoke")
        window.append_log("[Main] 로그 append 확인")
        diagnostics = window.diagnostics()
        print(f"append_ok={diagnostics.edit_created}")
    finally:
        window.hide()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--append-check", action="store_true")
    args = parser.parse_args()

    if args.import_only:
        run_import_check()
    elif args.diagnostics:
        run_diagnostics()
    elif args.append_check:
        run_append_check()
    else:
        parser.error("choose --import-only, --diagnostics, or --append-check")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
