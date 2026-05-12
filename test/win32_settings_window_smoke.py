"""Smoke checks for the Win32 settings window candidate."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parents[1]
if str(ROOT_DIR) not in sys.path:
    sys.path.insert(0, str(ROOT_DIR))

from data.steam_session import SteamSession
from overlay.win32.settings_window import Win32SettingsWindow

SMOKE_SESSION = SteamSession(
    steam_id="76561198000000000",
    account_name="smoke",
    persona_name="Smoke User",
    most_recent=True,
)
SMOKE_OTHER_SESSION = SteamSession(
    steam_id="76561198000000001",
    account_name="other",
    persona_name="Other User",
    most_recent=False,
)


def run_import_check() -> None:
    print("Win32 settings window import ok")


def run_diagnostics() -> None:
    window = _smoke_window()
    diagnostics = window.diagnostics()
    print(f"hwnd_created={diagnostics.hwnd_created}")
    print(f"trackbar_created={diagnostics.trackbar_created}")
    print(f"scale_button_count={diagnostics.scale_button_count}")
    print(f"system_checkbox_created={diagnostics.system_checkbox_created}")
    print(f"varchive_session_count={diagnostics.varchive_session_count}")
    print(f"varchive_edit_created={diagnostics.varchive_edit_created}")
    print(f"other_session_count={diagnostics.other_session_count}")
    print(f"others_visible={diagnostics.others_visible}")
    print(f"current_tab={diagnostics.current_tab}")
    if not diagnostics.hwnd_created or not diagnostics.trackbar_created:
        raise SystemExit(1)
    if diagnostics.scale_button_count < 4 or not diagnostics.system_checkbox_created:
        raise SystemExit(1)
    if diagnostics.varchive_session_count < 2 or not diagnostics.varchive_edit_created:
        raise SystemExit(1)


def run_callback_check() -> None:
    values: dict[str, object] = {}
    window = _smoke_window()
    window.set_opacity_callback(lambda value: values.setdefault("opacity", value))
    window.set_scale_callback(lambda value: values.setdefault("scale", value))
    opacity = window.simulate_opacity_change(7)
    scale = window.simulate_scale_change(1.25)
    print(f"opacity={opacity:.1f}")
    print(f"scale={scale:.2f}")
    print(f"opacity_callback={values.get('opacity')}")
    print(f"scale_callback={values.get('scale')}")
    if values.get("opacity") != opacity or values.get("scale") != scale:
        raise SystemExit(1)


def run_varchive_check() -> None:
    values: dict[str, object] = {}
    window = _smoke_window()
    window.set_fetch_varchive_callback(
        lambda steam_id, v_id, button: values.setdefault("fetch", (steam_id, v_id, button))
    )
    window.set_sync_callback(
        lambda steam_id, persona, path: values.setdefault("sync", (steam_id, persona, path))
    )
    window.set_account_file_callback(
        lambda steam_id, path: values.setdefault("account", (steam_id, path))
    )
    window.simulate_varchive_fetch(SMOKE_SESSION.steam_id, "test-v-id", 4)
    window.simulate_sync(SMOKE_SESSION.steam_id, r"C:\tmp\account.txt")
    print(f"fetch={values.get('fetch')}")
    print(f"sync={values.get('sync')}")
    print(f"account={values.get('account')}")
    if values.get("fetch") != (SMOKE_SESSION.steam_id, "test-v-id", 4):
        raise SystemExit(1)
    if values.get("sync") != (SMOKE_SESSION.steam_id, SMOKE_SESSION.persona_name, r"C:\tmp\account.txt"):
        raise SystemExit(1)


def run_multi_account_check() -> None:
    window = _smoke_window()
    before = window.diagnostics()
    window.simulate_toggle_others()
    after = window.diagnostics()
    print(f"other_session_count={before.other_session_count}")
    print(f"before_visible={before.others_visible}")
    print(f"after_visible={after.others_visible}")
    if before.other_session_count <= 0:
        raise SystemExit(1)
    if before.others_visible or not after.others_visible:
        raise SystemExit(1)


def run_show(duration_ms: int, tab: str) -> None:
    window = _smoke_window()
    window.show_window()
    window.simulate_tab(tab)
    window.pump(duration_ms)
    print("show_ok=True")


def _smoke_window() -> Win32SettingsWindow:
    return Win32SettingsWindow(
        persist=False,
        sessions=[SMOKE_SESSION, SMOKE_OTHER_SESSION],
        current_steam_id=SMOKE_SESSION.steam_id,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--callback-check", action="store_true")
    parser.add_argument("--varchive-check", action="store_true")
    parser.add_argument("--multi-account-check", action="store_true")
    parser.add_argument("--show", action="store_true")
    parser.add_argument("--tab", choices=["ui", "system", "varchive"], default="ui")
    parser.add_argument("--duration-ms", type=int, default=3000)
    args = parser.parse_args()

    if args.import_only:
        run_import_check()
    elif args.diagnostics:
        run_diagnostics()
    elif args.callback_check:
        run_callback_check()
    elif args.varchive_check:
        run_varchive_check()
    elif args.multi_account_check:
        run_multi_account_check()
    elif args.show:
        run_show(args.duration_ms, args.tab)
    else:
        parser.error("choose --import-only, --diagnostics, --callback-check, or --show")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
