"""Win32 row rendering helpers for the V-Archive sync window."""

from __future__ import annotations
from dataclasses import dataclass
import win32con
import win32gui
from data.sync_manager import SyncCandidate
from overlay.win32 import settings_common as controls

# Column Layout Configuration
COL_X = [16, 60, 104, 310, 396, 420, 506, 582]
COL_WIDTH = [40, 40, 200, 80, 20, 80, 72, 100]

@dataclass(frozen=True)
class SyncRowHandles:
    controls: list[int]
    upload_hwnd: int
    delete_hwnd: int
    status_hwnd: int
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
    row_controls: list[int] = []
    
    # Render Data Columns
    row_controls.append(controls.static(parent, hinst, candidate.difficulty, (COL_X[0], top, COL_WIDTH[0], 22)))
    row_controls.append(controls.static(parent, hinst, candidate.button_mode, (COL_X[1], top, COL_WIDTH[1], 22)))
    row_controls.append(controls.static(parent, hinst, _elide(candidate.song_name, 24), (COL_X[2], top, COL_WIDTH[2], 22)))
    
    # Current (Overmax) Rate
    om_rate = _rate_text(candidate.overmax_rate, candidate.overmax_mc)
    row_controls.append(controls.static(parent, hinst, om_rate, (COL_X[3], top, COL_WIDTH[3], 22), win32con.SS_RIGHT))
    
    row_controls.append(controls.static(parent, hinst, ">", (COL_X[4], top, COL_WIDTH[4], 22), win32con.SS_CENTER))
    
    # V-Archive Rate
    va_rate = _varchive_text(candidate)
    row_controls.append(controls.static(parent, hinst, va_rate, (COL_X[5], top, COL_WIDTH[5], 22), win32con.SS_RIGHT))
    
    # Reason (Muted Text Color would be nice if we had it easily)
    row_controls.append(controls.static(parent, hinst, candidate.reason, (COL_X[6], top, COL_WIDTH[6], 22)))

    # Actions
    upload_id = 4100 + index * 2
    delete_id = upload_id + 1
    
    upload = controls.button(parent, hinst, "등록", upload_id, (COL_X[7], top - 2, 44, 24))
    delete = controls.button(parent, hinst, "삭제", delete_id, (COL_X[7] + 48, top - 2, 44, 24))
    win32gui.EnableWindow(upload, account_ready)
    
    status = controls.static(parent, hinst, "", (COL_X[7], top + 22, 100, 12))
    
    row_controls.extend([upload, delete, status])
    for hwnd in row_controls:
        win32gui.SendMessage(hwnd, win32con.WM_SETFONT, font, True)
    
    return SyncRowHandles(row_controls, upload, delete, status, upload_id, delete_id)

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
