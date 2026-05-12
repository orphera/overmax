"""Win32 row rendering helpers for the V-Archive sync window."""

from __future__ import annotations

from dataclasses import dataclass

import win32con
import win32gui

from data.sync_manager import SyncCandidate
from overlay.win32 import settings_common as controls


@dataclass(frozen=True)
class SyncRowHandles:
    controls: list[int]
    upload_hwnd: int
    delete_hwnd: int
    upload_id: int
    delete_id: int


def create_candidate_row(
    parent: int,
    hinst: int,
    font: int,
    candidate: SyncCandidate,
    index: int,
    top: int,
    account_ready: bool,
) -> SyncRowHandles:
    """Create a native-control row; action buttons are callbacks for later slices."""
    row_controls: list[int] = []
    row_controls.append(controls.static(parent, hinst, candidate.difficulty, (18, top, 34, 22)))
    row_controls.append(controls.static(parent, hinst, candidate.button_mode, (58, top, 34, 22)))
    row_controls.append(controls.static(parent, hinst, _elide(candidate.song_name), (100, top, 190, 22)))
    row_controls.append(controls.static(parent, hinst, _rate_text(candidate.overmax_rate, candidate.overmax_mc), (300, top, 72, 22)))
    row_controls.append(controls.static(parent, hinst, ">", (378, top, 18, 22)))
    row_controls.append(controls.static(parent, hinst, _varchive_text(candidate), (402, top, 72, 22)))
    row_controls.append(controls.static(parent, hinst, candidate.reason, (480, top, 74, 22)))

    upload_id = 4100 + index * 2
    delete_id = upload_id + 1
    upload = controls.button(parent, hinst, "등록", upload_id, (562, top - 2, 48, 24))
    delete = controls.button(parent, hinst, "삭제", delete_id, (616, top - 2, 48, 24))
    win32gui.EnableWindow(upload, account_ready)
    row_controls.extend([upload, delete])
    for hwnd in row_controls:
        win32gui.SendMessage(hwnd, win32con.WM_SETFONT, font, True)
    return SyncRowHandles(row_controls, upload, delete, upload_id, delete_id)


def _rate_text(rate: float, max_combo: bool) -> str:
    suffix = " M" if max_combo else ""
    return f"{rate:.2f}%{suffix}"


def _varchive_text(candidate: SyncCandidate) -> str:
    if candidate.varchive_rate is None:
        return "--"
    return _rate_text(candidate.varchive_rate, bool(candidate.varchive_mc))


def _elide(text: str, limit: int = 28) -> str:
    if len(text) <= limit:
        return text
    return text[: max(0, limit - 3)] + "..."
