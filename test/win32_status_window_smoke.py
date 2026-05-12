"""Smoke checks for the Win32 status window used by update workers."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parents[1]
if str(ROOT_DIR) not in sys.path:
    sys.path.insert(0, str(ROOT_DIR))

from infra.gui.status_window import Win32StatusWindow


def run_import_check() -> None:
    print("Win32 status window import ok")


def run_diagnostics() -> None:
    window = Win32StatusWindow("Overmax Update")
    try:
        diagnostics = window.diagnostics()
        print(f"hwnd_created={diagnostics.hwnd_created}")
        print(f"label_created={diagnostics.label_created}")
        print(f"topmost={diagnostics.topmost}")
        print(f"close_disabled={diagnostics.close_disabled}")
        if not diagnostics.hwnd_created or not diagnostics.label_created:
            raise SystemExit(1)
        if diagnostics.topmost or not diagnostics.close_disabled:
            raise SystemExit(1)
    finally:
        window.close()


def run_show(duration_ms: int) -> None:
    window = Win32StatusWindow("Overmax Update")
    try:
        if not window.show("업데이트 파일을 적용하는 중입니다..."):
            raise SystemExit(1)
        window.update("업데이트 완료\n\nv0.0.0 -> v0.0.1\n\n잠시 후 다시 실행합니다...")
        window.pump(duration_ms)
        print("show_ok=True")
    finally:
        window.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--show", action="store_true")
    parser.add_argument("--duration-ms", type=int, default=1000)
    args = parser.parse_args()

    if args.import_only:
        run_import_check()
    elif args.diagnostics:
        run_diagnostics()
    elif args.show:
        run_show(args.duration_ms)
    else:
        parser.error("choose --import-only, --diagnostics, or --show")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
