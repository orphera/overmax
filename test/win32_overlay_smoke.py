"""Smoke CLI for the production Win32 overlay candidate."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from overlay.win32.geometry import DpiCase, PositionDiagnostics, build_dpi_cases
from overlay.win32.render import (
    RenderDiagnostics,
    TextLayoutDiagnostics,
    render_diagnostics_ok,
    text_layout_diagnostics_ok,
)
from overlay.win32.view_state import Win32OverlayViewState, default_view_state
from overlay.win32.window import (
    WindowDiagnostics,
    Win32OverlayWindow,
    set_process_dpi_awareness,
)
from win32_overlay_payload_sample import (
    long_payload_view_state,
    sample_payload_view_state,
)

DEFAULT_DURATION_MS = 3000


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--import-only", action="store_true")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--position-check", action="store_true")
    parser.add_argument("--dpi-check", action="store_true")
    parser.add_argument("--render-check", action="store_true")
    parser.add_argument("--layout-check", action="store_true")
    parser.add_argument("--show", action="store_true")
    parser.add_argument("--payload-sample", action="store_true")
    parser.add_argument("--long-payload-sample", action="store_true")
    parser.add_argument("--duration-ms", type=int, default=DEFAULT_DURATION_MS)
    return parser.parse_args()


def print_diagnostics(diagnostics: WindowDiagnostics) -> None:
    print(f"capture_excluded={diagnostics.capture_excluded}")
    print(f"style_ok={diagnostics.style_ok}")
    print(f"noactivate={diagnostics.noactivate}")
    print(f"topmost={diagnostics.topmost}")
    print(f"focus_preserved={diagnostics.focus_preserved}")
    print(f"dpi={diagnostics.dpi}")
    print(f"rect={diagnostics.rect}")
    print(f"monitor={diagnostics.monitor}")
    print(f"ex_style=0x{diagnostics.ex_style:08X}")


def print_position_diagnostics(diagnostics: PositionDiagnostics) -> None:
    print(f"calculated={diagnostics.calculated}")
    print(f"saved={diagnostics.saved}")
    print(f"moved={diagnostics.moved}")
    print(f"callback_position={diagnostics.callback_position}")
    print(f"monitor={diagnostics.monitor}")


def print_dpi_cases(cases: list[DpiCase]) -> None:
    for case in cases:
        print(
            "dpi={dpi} scale={scale:.2f} size={size} position={position} "
            "within_monitor={within_monitor} monitor={monitor}".format(
                dpi=case.dpi,
                scale=case.scale,
                size=case.size,
                position=case.position,
                within_monitor=case.within_monitor,
                monitor=case.monitor,
            )
        )


def dpi_cases_ok(cases: list[DpiCase]) -> bool:
    return all(case.within_monitor for case in cases)


def print_render_diagnostics(diagnostics: RenderDiagnostics) -> None:
    print(f"alpha={diagnostics.alpha}")
    print(f"rounded_region={diagnostics.rounded_region}")
    print(f"font_created={diagnostics.font_created}")
    print(f"font_quality={diagnostics.font_quality}")
    print(f"text_extent={diagnostics.text_extent}")


def print_text_layout_diagnostics(diagnostics: TextLayoutDiagnostics) -> None:
    for case in diagnostics.cases:
        print(
            "{name}=width:{text_width}/{width} height:{text_height}/{height} "
            "fits_width:{fits_width} fits_height:{fits_height}".format(
                name=case.name,
                text_width=case.text_width,
                width=case.width,
                text_height=case.text_height,
                height=case.height,
                fits_width=case.fits_width,
                fits_height=case.fits_height,
            )
        )
    print(f"overflowing_cases={len(diagnostics.overflowing_cases)}")


def resolve_view_state(args: argparse.Namespace) -> Win32OverlayViewState:
    if args.long_payload_sample:
        return long_payload_view_state()
    if args.payload_sample:
        return sample_payload_view_state()
    return default_view_state()


def main() -> int:
    args = parse_args()

    if args.import_only:
        print("Win32 import ok")
        return 0

    set_process_dpi_awareness()
    view_state = resolve_view_state(args)

    if args.diagnostics:
        print_diagnostics(Win32OverlayWindow(view_state).diagnostics())
        return 0

    if args.position_check:
        print_position_diagnostics(Win32OverlayWindow(view_state).position_diagnostics())
        return 0

    if args.dpi_check:
        cases = build_dpi_cases()
        print_dpi_cases(cases)
        return 0 if dpi_cases_ok(cases) else 1

    if args.render_check:
        diagnostics = Win32OverlayWindow(view_state).render_diagnostics()
        print_render_diagnostics(diagnostics)
        return 0 if render_diagnostics_ok(diagnostics) else 1

    if args.layout_check:
        diagnostics = Win32OverlayWindow(view_state).text_layout_diagnostics()
        print_text_layout_diagnostics(diagnostics)
        return 0 if text_layout_diagnostics_ok(diagnostics) else 1

    if not args.show:
        print("Use --import-only, --diagnostics, --layout-check, or --show")
        return 2

    return Win32OverlayWindow(view_state).show_for(args.duration_ms)


if __name__ == "__main__":
    raise SystemExit(main())
