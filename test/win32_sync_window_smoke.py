"""Smoke checks for the Win32 sync window candidate."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parents[1]
if str(ROOT_DIR) not in sys.path:
    sys.path.insert(0, str(ROOT_DIR))

from data.sync_manager import SyncCandidate
from overlay.win32.sync_window import Win32SyncWindow


def run_import_check() -> None:
    print("Win32 sync window import ok")


def run_diagnostics() -> None:
    window = Win32SyncWindow(None, None, sample_candidates=_sample_candidates())
    window.show_window("76561198000000000", "Smoke User", "")
    diagnostics = window.diagnostics()
    print(f"hwnd_created={diagnostics.hwnd_created}")
    print(f"refresh_enabled={diagnostics.refresh_enabled}")
    print(f"row_count={diagnostics.row_count}")
    print(f"status_text={diagnostics.status_text}")
    if not diagnostics.hwnd_created or diagnostics.refresh_enabled:
        raise SystemExit(1)
    if diagnostics.row_count != 30:
        raise SystemExit(1)


def run_account_check() -> None:
    window = Win32SyncWindow(None, None, sample_candidates=_sample_candidates())
    window.show_window("76561198000000000", "Smoke User", "")
    before = window.diagnostics()
    window.set_account("76561198000000000", object())
    after = window.diagnostics()
    print(f"before_refresh_enabled={before.refresh_enabled}")
    print(f"after_refresh_enabled={after.refresh_enabled}")
    if before.refresh_enabled or not after.refresh_enabled:
        raise SystemExit(1)


def run_bridge_check() -> None:
    window = Win32SyncWindow(None, None, sample_candidates=[])
    window.show_window("76561198000000000", "Smoke User", "")
    
    # Simulate worker emitting scan_finished
    candidates = _sample_candidates()
    window._signals.scan_finished.emit(candidates)
    
    # Process messages
    window.pump(500)
    
    diagnostics = window.diagnostics()
    print(f"bridge_row_count={diagnostics.row_count}")
    if diagnostics.row_count != len(candidates):
        raise SystemExit(1)
    print("bridge_ok=True")


def run_show(duration_ms: int) -> None:
    window = Win32SyncWindow(None, None, sample_candidates=_sample_candidates())
    window.show_window("76561198000000000", "Smoke User", "")
    window.pump(duration_ms)
    print("show_ok=True")


def _sample_candidates() -> list[SyncCandidate]:
    return [
        SyncCandidate(i, f"Song {i}", "Smoke", "base", "4B", "MX", 99.12, True, 97.0, False)
        for i in range(30)
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--account-check", action="store_true")
    parser.add_argument("--bridge-check", action="store_true")
    parser.add_argument("--show", action="store_true")
    parser.add_argument("--duration-ms", type=int, default=3000)
    args = parser.parse_args()

    if args.import_only:
        run_import_check()
    elif args.diagnostics:
        run_diagnostics()
    elif args.account_check:
        run_account_check()
    elif args.bridge_check:
        run_bridge_check()
    elif args.show:
        run_show(args.duration_ms)
    else:
        parser.error("choose --import-only, --diagnostics, --account-check, or --show")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
