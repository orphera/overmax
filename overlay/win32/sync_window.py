"""Win32 sync window candidate shell."""

from __future__ import annotations

from dataclasses import dataclass
import time
from typing import Sequence

import win32api
import win32con
import win32gui

from data.sync_manager import SyncCandidate
from data.varchive import VArchiveDB
from data.varchive_uploader import AccountInfo, parse_account_file
from data.record_manager import RecordManager
from infra.gui.windowing import WindowCreateSpec, create_window, register_window_class
from overlay.win32 import settings_common as controls
from overlay.win32.sync_row import SyncRowHandles, create_candidate_row

CLASS_NAME = "OvermaxWin32SyncWindow"
WINDOW_SIZE = (700, 500)
REFRESH_ID = 4001
WINDOW_BG = win32api.RGB(0xF3, 0xF4, 0xF6)
TEXT_COLOR = win32api.RGB(0x1F, 0x29, 0x37)


@dataclass(frozen=True)
class SyncWindowDiagnostics:
    hwnd_created: bool
    refresh_enabled: bool
    row_count: int
    status_text: str
    current_steam_id: str


class Win32SyncWindow:
    def __init__(
        self,
        varchive_db: VArchiveDB | None,
        record_manager: RecordManager | None,
        sample_candidates: Sequence[SyncCandidate] | None = None,
    ) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self._vdb = varchive_db
        self._record_manager = record_manager
        self._sample_candidates = list(sample_candidates or [])
        self._font = 0
        self._window_brush = win32gui.CreateSolidBrush(WINDOW_BG)
        self._accounts: dict[str, AccountInfo] = {}
        self._current_steam_id = ""
        self._refresh_btn = 0
        self._status_label = 0
        self._count_label = 0
        self._row_handles: list[SyncRowHandles] = []
        self._base_controls: list[int] = []
        self._register_class()

    def set_account(self, steam_id: str, account: AccountInfo | None) -> None:
        if account:
            self._accounts[steam_id] = account
        else:
            self._accounts.pop(steam_id, None)
        self._update_ui_states()

    def show_window(self, steam_id: str, persona_name: str, account_path: str) -> None:
        self._current_steam_id = steam_id
        self._ensure_window()
        self.set_account(steam_id, _parse_account(account_path))
        win32gui.SetWindowText(self.hwnd, f"V-Archive 동기화 - {persona_name or steam_id}")
        self._render_candidates(self._sample_candidates)
        win32gui.ShowWindow(self.hwnd, win32con.SW_SHOWNORMAL)
        _try_set_foreground(self.hwnd)

    def hide(self) -> None:
        if self.hwnd:
            win32gui.ShowWindow(self.hwnd, win32con.SW_HIDE)

    def pump(self, millis: int = 30) -> None:
        deadline = time.time() + max(0, millis) / 1000.0
        while time.time() < deadline:
            win32gui.PumpWaitingMessages()
            time.sleep(0.01)

    def diagnostics(self) -> SyncWindowDiagnostics:
        self._ensure_window()
        return SyncWindowDiagnostics(
            hwnd_created=bool(self.hwnd),
            refresh_enabled=bool(win32gui.IsWindowEnabled(self._refresh_btn)),
            row_count=len(self._row_handles),
            status_text=win32gui.GetWindowText(self._status_label),
            current_steam_id=self._current_steam_id,
        )

    def simulate_refresh(self) -> None:
        self._ensure_window()
        self._handle_refresh()

    def _ensure_window(self) -> bool:
        if self.hwnd and win32gui.IsWindow(self.hwnd):
            return True
        self.hwnd = create_window(self.hinst, self._create_spec())
        if not self.hwnd:
            return False
        self._font = controls.create_font()
        self._create_controls()
        self._set_status("account.txt를 설정하고 동기화 후보를 확인하세요.")
        self._update_ui_states()
        return True

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title="V-Archive 동기화",
            ex_style=win32con.WS_EX_TOOLWINDOW,
            style=win32con.WS_OVERLAPPED | win32con.WS_CAPTION | win32con.WS_SYSMENU,
            position=controls.center_position(WINDOW_SIZE),
            size=WINDOW_SIZE,
        )

    def _create_controls(self) -> None:
        self._base_controls.extend([
            controls.static(self.hwnd, self.hinst, "V-Archive 동기화", (18, 18, 180, 24)),
            controls.static(self.hwnd, self.hinst, "난이도   모드   곡명                  Overmax     V-Archive   차이", (18, 62, 560, 22)),
        ])
        self._count_label = controls.static(self.hwnd, self.hinst, "", (202, 18, 160, 24))
        self._status_label = controls.static(self.hwnd, self.hinst, "", (18, 442, 480, 24))
        self._refresh_btn = controls.button(self.hwnd, self.hinst, "불러오기", REFRESH_ID, (584, 438, 86, 28))
        self._base_controls.extend([self._count_label, self._status_label, self._refresh_btn])
        for hwnd in self._base_controls:
            win32gui.SendMessage(hwnd, win32con.WM_SETFONT, self._font, True)

    def _render_candidates(self, candidates: Sequence[SyncCandidate]) -> None:
        self._clear_rows()
        if not candidates:
            self._set_count("")
            self._set_status("동기화 창 골격 준비 완료. 실제 스캔은 다음 절편에서 연결합니다.")
            return
        account_ready = self._get_current_account() is not None
        for index, candidate in enumerate(candidates[:8]):
            top = 94 + index * 34
            row = create_candidate_row(self.hwnd, self.hinst, self._font, candidate, index, top, account_ready)
            self._row_handles.append(row)
        self._set_count(f"{len(candidates)}개 후보")
        self._set_status("sample 후보를 표시했습니다. 등록/삭제는 아직 no-op입니다.")

    def _clear_rows(self) -> None:
        for row in self._row_handles:
            for hwnd in row.controls:
                if hwnd and win32gui.IsWindow(hwnd):
                    win32gui.DestroyWindow(hwnd)
        self._row_handles.clear()

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_COMMAND:
            self._handle_command(win32api.LOWORD(wparam))
            return 0
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._window_brush)
            return 1
        if msg in (win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return self._paint_control_background(wparam)
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _handle_command(self, control_id: int) -> None:
        if control_id == REFRESH_ID:
            self._handle_refresh()
        elif control_id >= 4100:
            self._set_status("등록/삭제 동작은 다음 절편에서 dry-run으로 연결합니다.")

    def _handle_refresh(self) -> None:
        if self._get_current_account() is None:
            self._set_status("account.txt를 먼저 설정하세요.")
            return
        self._render_candidates(self._sample_candidates)

    def _update_ui_states(self) -> None:
        if not self._refresh_btn:
            return
        account_ready = self._get_current_account() is not None
        win32gui.EnableWindow(self._refresh_btn, account_ready)
        for row in self._row_handles:
            win32gui.EnableWindow(row.upload_hwnd, account_ready)

    def _get_current_account(self) -> AccountInfo | None:
        return self._accounts.get(self._current_steam_id)

    def _set_status(self, text: str) -> None:
        if self._status_label:
            win32gui.SetWindowText(self._status_label, text)

    def _set_count(self, text: str) -> None:
        if self._count_label:
            win32gui.SetWindowText(self._count_label, text)

    def _paint_control_background(self, hdc: int) -> int:
        win32gui.SetBkColor(hdc, WINDOW_BG)
        win32gui.SetTextColor(hdc, TEXT_COLOR)
        return self._window_brush

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)


def _parse_account(account_path: str) -> AccountInfo | None:
    path = account_path.strip()
    return parse_account_file(path) if path else None


def _try_set_foreground(hwnd: int) -> None:
    try:
        win32gui.SetForegroundWindow(hwnd)
    except win32gui.error:
        pass
