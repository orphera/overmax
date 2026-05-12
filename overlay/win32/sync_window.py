"""Win32 sync window candidate shell."""

from __future__ import annotations

from dataclasses import dataclass
import threading
import time
from typing import Sequence

import win32api
import win32con
import win32gui

from data.sync_manager import SyncCandidate, build_candidates
from data.varchive import VArchiveDB
from data.varchive_uploader import AccountInfo, parse_account_file
from data.record_manager import RecordManager
from infra.gui.windowing import WindowCreateSpec, create_window, register_window_class
from overlay.win32 import settings_common as controls
from overlay.win32.sync_bridge import (
    Win32SyncSignals, WM_SYNC_SCAN_FINISHED, WM_SYNC_ROW_STATUS, WM_SYNC_ACTION_FINISHED
)
from overlay.win32.sync_row import SyncRowHandles, create_candidate_row

CLASS_NAME = "OvermaxWin32SyncWindow"
CONTAINER_CLASS_NAME = "OvermaxWin32SyncContainer"
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
        self._empty_label = 0
        self._row_handles: list[SyncRowHandles] = []
        self._base_controls: list[int] = []
        self._signals = Win32SyncSignals(0)  # HWND set after creation
        self._candidates: list[SyncCandidate] = []
        self._scan_in_progress = False
        self._rescan_queued = False
        self._scroll_pos = 0
        self._max_scroll = 0
        self._list_container = 0
        self._content_window = 0
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
        win32gui.ShowWindow(self.hwnd, win32con.SW_SHOWNORMAL)
        _try_set_foreground(self.hwnd)
        if self._record_manager and self._get_current_account() and not self._candidates:
            self._start_scan()

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
        self._signals = Win32SyncSignals(self.hwnd)
        self._font = controls.create_font()
        self._create_controls()
        self._set_status("account.txt를 설정하고 불러오기를 눌러주세요.")
        self._update_ui_states()
        return True

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title="V-Archive 동기화",
            ex_style=win32con.WS_EX_TOOLWINDOW,
            style=win32con.WS_OVERLAPPED | win32con.WS_CAPTION | win32con.WS_SYSMENU | win32con.WS_VSCROLL,
            position=controls.center_position(WINDOW_SIZE),
            size=WINDOW_SIZE,
        )

    def _create_controls(self) -> None:
        # Header Area
        header_ctx = controls.LayoutContext((0, 0, WINDOW_SIZE[0], 90), controls.LayoutPadding(20, 16, 20, 8))
        self._title_label = controls.static(self.hwnd, self.hinst, "동기화 후보 목록", header_ctx.next_rect(26), win32con.SS_LEFT)
        self._count_label = controls.static(self.hwnd, self.hinst, "", (WINDOW_SIZE[0] - 180, 16, 160, 26), win32con.SS_RIGHT)
        
        # Column Header (Perfectly aligned with sync_row.COL_X)
        from overlay.win32.sync_row import COL_X, COL_WIDTH
        header_y = 66
        self._header_labels = [
            controls.static(self.hwnd, self.hinst, "난이도", (COL_X[0], header_y, COL_WIDTH[0], 22)),
            controls.static(self.hwnd, self.hinst, "모드", (COL_X[1], header_y, COL_WIDTH[1], 22)),
            controls.static(self.hwnd, self.hinst, "곡 제목", (COL_X[2], header_y, COL_WIDTH[2], 22)),
            controls.static(self.hwnd, self.hinst, "내 기록", (COL_X[3], header_y, COL_WIDTH[3], 22), win32con.SS_RIGHT),
            controls.static(self.hwnd, self.hinst, "V-Archive", (COL_X[5], header_y, COL_WIDTH[5], 22), win32con.SS_RIGHT),
            controls.static(self.hwnd, self.hinst, "사유", (COL_X[6], header_y, COL_WIDTH[6], 22)),
            controls.static(self.hwnd, self.hinst, "상태", (COL_X[7], header_y, COL_WIDTH[7], 22)),
        ]
        
        # List Area (Middle Viewport) - between 90 and 420
        self._list_container = create_window(self.hinst, WindowCreateSpec(
            class_name=CONTAINER_CLASS_NAME, title="", ex_style=0,
            style=win32con.WS_CHILD | win32con.WS_VISIBLE | win32con.WS_CLIPCHILDREN,
            position=(0, 90), size=(WINDOW_SIZE[0], 330), parent=self.hwnd
        ))
        
        # Inner Content Window (The one that actually scrolls)
        self._content_window = create_window(self.hinst, WindowCreateSpec(
            class_name=CONTAINER_CLASS_NAME, title="", ex_style=0,
            style=win32con.WS_CHILD | win32con.WS_VISIBLE,
            position=(0, 0), size=(WINDOW_SIZE[0], 10000), parent=self._list_container
        ))

        # Empty Label (centered in list_container)
        self._empty_label = controls.static(self._list_container, self.hinst, "", (0, 140, WINDOW_SIZE[0], 40), win32con.SS_CENTER)

        # Footer Area
        footer_y = 420
        self._status_label = controls.static(self.hwnd, self.hinst, "", (20, footer_y + 8, 380, 24))
        self._refresh_btn = controls.button(self.hwnd, self.hinst, "불러오기", REFRESH_ID, (WINDOW_SIZE[0] - 110, footer_y + 4, 90, 30))
        
        self._base_controls.extend([
            self._title_label, self._count_label, 
            *self._header_labels,
            self._status_label, self._refresh_btn, self._list_container
        ])
        
        bold_font = controls.create_font(height=-18, weight=win32con.FW_BOLD)
        win32gui.SendMessage(self._title_label, win32con.WM_SETFONT, bold_font, True)
        
        for hwnd in self._base_controls:
            if hwnd != self._title_label:
                win32gui.SendMessage(hwnd, win32con.WM_SETFONT, self._font, True)

    def _render_candidates(self, candidates: Sequence[SyncCandidate]) -> None:
        self._clear_rows()
        if not candidates:
            self._set_empty_text("동기화 후보가 없습니다. V-Archive 기록이 이미 최신입니다.")
            self._set_count("")
            self._set_status("최신 상태입니다.")
            return
        
        self._set_empty_text("")
        account_ready = self._get_current_account() is not None
        for index, candidate in enumerate(candidates):
            top = index * 34
            row = create_candidate_row(self._content_window, self.hinst, self._font, candidate, index, top, account_ready)
            self._row_handles.append(row)
        
        self._update_scrollbar(len(candidates))
        self._set_count(f" — {len(candidates)}개 후보")
        self._set_status(f"{len(candidates)}개의 갱신 후보를 찾았습니다.")

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
        if msg == WM_SYNC_SCAN_FINISHED:
            if args := self._signals.scan_finished.pull():
                self._on_scan_finished(*args)
            return 0
        if msg == WM_SYNC_ROW_STATUS:
            if args := self._signals.row_status_changed.pull():
                self._on_row_status(*args)
            return 0
        if msg == WM_SYNC_ACTION_FINISHED:
            if args := self._signals.action_finished.pull():
                self._on_action_finished(*args)
            return 0
        if msg == win32con.WM_VSCROLL:
            self._handle_scroll(win32api.LOWORD(wparam), win32api.HIWORD(wparam))
            return 0
        if msg == win32con.WM_MOUSEWHEEL:
            # Extract signed high word
            hiword = (wparam >> 16) & 0xFFFF
            delta = hiword - 65536 if hiword > 32767 else hiword
            action = win32con.SB_LINEUP if delta > 0 else win32con.SB_LINEDOWN
            self._handle_scroll(action, 0)
            return 0
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._window_brush)
            return 1
        if msg in (win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return self._paint_control_background(wparam)
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _container_wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        """Forward commands and painting from container to parent window."""
        if msg in (win32con.WM_COMMAND, win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return win32gui.SendMessage(win32gui.GetParent(hwnd), msg, wparam, lparam)
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _handle_command(self, control_id: int) -> None:
        if control_id == REFRESH_ID:
            self._handle_refresh()
        elif control_id >= 4100:
            index = (control_id - 4100) // 2
            is_delete = (control_id - 4100) % 2 == 1
            if is_delete:
                self._on_delete_requested(index)
            else:
                self._on_upload_requested(index)

    def _on_upload_requested(self, index: int) -> None:
        account = self._get_current_account()
        if account is None or index >= len(self._candidates):
            return

        # Set status in row UI
        if index < len(self._row_handles):
            win32gui.SetWindowText(self._row_handles[index].status_hwnd, "업로드 중...")

        threading.Thread(
            target=self._upload_worker,
            args=(index, self._candidates[index], account),
            daemon=True,
        ).start()

    def _on_delete_requested(self, index: int) -> None:
        if index >= len(self._candidates):
            return

        # Set status in row UI
        if index < len(self._row_handles):
            win32gui.SetWindowText(self._row_handles[index].status_hwnd, "삭제 중...")

        threading.Thread(
            target=self._delete_worker,
            args=(index, self._candidates[index]),
            daemon=True,
        ).start()

    def _upload_worker(self, index: int, candidate: SyncCandidate, account: AccountInfo) -> None:
        result = upload_score(
            account=account,
            song_name=candidate.song_name,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
            composer=candidate.composer,
        )
        if result.success:
            status = "success" if result.updated else "no_update"
            message = ""
            if result.updated:
                self._update_varchive_cache_after_upload(candidate)
        else:
            status = "error"
            message = result.message

        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _delete_worker(self, index: int, candidate: SyncCandidate) -> None:
        if self._record_manager is None:
            return
        success = self._record_manager.delete(
            song_id=candidate.song_id,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
        )
        status = "success" if success else "error"
        message = "" if success else "삭제 실패"
        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _update_varchive_cache_after_upload(self, candidate: SyncCandidate) -> None:
        from overlay.sync_actions import _BUTTON_NUM_BY_MODE
        button = _BUTTON_NUM_BY_MODE.get(candidate.button_mode)
        if button is None or not self._current_steam_id:
            return
        vclient = getattr(self._record_manager, "vclient", None)
        if vclient is None:
            return
        success = vclient.upsert_cached_record(
            steam_id=self._current_steam_id,
            button=button,
            song_id=candidate.song_id,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
        )
        if success and self._record_manager:
            self._record_manager.refresh()

    def _handle_refresh(self) -> None:
        if self._get_current_account() is None:
            self._set_status("account.txt를 먼저 설정하세요.")
            return
        self._start_scan()

    def _start_scan(self) -> None:
        if self._record_manager is None:
            self._set_status("기록 관리자가 초기화되지 않았습니다.")
            return
        if self._scan_in_progress:
            self._rescan_queued = True
            return

        self._scan_in_progress = True
        self._update_ui_states()
        self._set_status("비교 중...")
        self._clear_rows()
        self._set_empty_text("분석 중...")
        threading.Thread(target=self._scan_worker, daemon=True).start()

    def _scan_worker(self) -> None:
        try:
            if not self._current_steam_id:
                raise ValueError("steam_id is not set")
            candidates = build_candidates(self._vdb, self._record_manager, self._current_steam_id)
        except Exception as e:
            candidates = []
            print(f"[Win32SyncWindow] Scan error: {e}")
        self._signals.scan_finished.emit(candidates)

    def _on_scan_finished(self, candidates: list[SyncCandidate]) -> None:
        self._scan_in_progress = False
        self._candidates = candidates
        self._update_ui_states()
        self._render_candidates(candidates)
        self._start_queued_rescan_if_needed()

    def _on_row_status(self, index: int, status: str, message: str) -> None:
        if index < len(self._row_handles):
            hwnd = self._row_handles[index].status_hwnd
            if status == "success":
                win32gui.SetWindowText(hwnd, "완료")
            elif status == "no_update":
                win32gui.SetWindowText(hwnd, "이미 최신")
            elif status == "error":
                win32gui.SetWindowText(hwnd, message or "실패")
            else:
                win32gui.SetWindowText(hwnd, status)
        
        self._set_status(f"[{index}] {status}: {message}" if message else f"[{index}] {status}")

    def _on_action_finished(self) -> None:
        self._start_scan()

    def _start_queued_rescan_if_needed(self) -> None:
        if not self._rescan_queued:
            return
        self._rescan_queued = False
        self._start_scan()

    def _update_ui_states(self) -> None:
        if not self._refresh_btn:
            return
        account_ready = self._get_current_account() is not None
        win32gui.EnableWindow(self._refresh_btn, account_ready)
        for row in self._row_handles:
            win32gui.EnableWindow(row.upload_hwnd, account_ready)

    def _get_current_account(self) -> AccountInfo | None:
        return self._accounts.get(self._current_steam_id)

    def _update_scrollbar(self, count: int) -> None:
        if not self.hwnd:
            return
        total_height = count * 34
        page_height = 330
        self._max_scroll = max(0, total_height - page_height)
        self._scroll_pos = min(self._scroll_pos, self._max_scroll)
        
        si = (win32con.SIF_RANGE | win32con.SIF_PAGE | win32con.SIF_POS, 0, total_height, page_height, self._scroll_pos, 0)
        win32gui.SetScrollInfo(self.hwnd, win32con.SB_VERT, si, True)
        self._apply_scroll()

    def _handle_scroll(self, action: int, pos: int) -> None:
        if action == win32con.SB_LINEUP:
            self._scroll_pos -= 34
        elif action == win32con.SB_LINEDOWN:
            self._scroll_pos += 34
        elif action == win32con.SB_PAGEUP:
            self._scroll_pos -= 330
        elif action == win32con.SB_PAGEDOWN:
            self._scroll_pos += 330
        elif action == win32con.SB_THUMBTRACK:
            self._scroll_pos = pos
        
        self._scroll_pos = max(0, min(self._scroll_pos, self._max_scroll))
        si = (win32con.SIF_POS, 0, 0, 0, self._scroll_pos, 0)
        win32gui.SetScrollInfo(self.hwnd, win32con.SB_VERT, si, True)
        self._apply_scroll()

    def _apply_scroll(self) -> None:
        if self._content_window:
            win32gui.MoveWindow(self._content_window, 0, -self._scroll_pos, WINDOW_SIZE[0], 10000, True)

    def _set_status(self, text: str) -> None:
        if self._status_label:
            win32gui.SetWindowText(self._status_label, text)

    def _set_count(self, text: str) -> None:
        if self._count_label:
            win32gui.SetWindowText(self._count_label, text)

    def _set_empty_text(self, text: str) -> None:
        if self._empty_label:
            win32gui.SetWindowText(self._empty_label, text)
            win32gui.ShowWindow(self._empty_label, win32con.SW_SHOW if text else win32con.SW_HIDE)

    def _paint_control_background(self, hdc: int) -> int:
        win32gui.SetBkColor(hdc, WINDOW_BG)
        win32gui.SetTextColor(hdc, TEXT_COLOR)
        return self._window_brush

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)
        register_window_class(self.hinst, CONTAINER_CLASS_NAME, self._container_wnd_proc)

    def _container_wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg in (win32con.WM_COMMAND, win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return win32gui.SendMessage(win32gui.GetParent(hwnd), msg, wparam, lparam)
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._window_brush)
            return 1
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)


def _parse_account(account_path: str) -> AccountInfo | None:
    path = account_path.strip()
    return parse_account_file(path) if path else None


def _try_set_foreground(hwnd: int) -> None:
    try:
        win32gui.SetForegroundWindow(hwnd)
    except win32gui.error:
        pass
