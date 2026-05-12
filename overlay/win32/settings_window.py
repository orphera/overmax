"""Win32 settings window candidate for UI/System settings."""

from __future__ import annotations

from dataclasses import dataclass
import time
from typing import Callable, Optional, Sequence

import win32api
import win32con
import win32gui

from constants import SCALE_PRESETS
from core.version import APP_VERSION
from data.steam_session import SteamSession, get_all_steam_sessions, get_most_recent_steam_id
from infra.gui.windowing import WindowCreateSpec, create_window, register_window_class
from overlay.win32 import settings_common as controls
from settings import SETTINGS, save_settings

CLASS_NAME = "OvermaxWin32SettingsWindow"
WINDOW_SIZE = (520, 460)
TRACKBAR_CLASS = "msctls_trackbar32"
TBM_GETPOS = win32con.WM_USER
TBM_SETPOS = win32con.WM_USER + 5
TBM_SETRANGE = win32con.WM_USER + 6
TAB_IDS = {"ui": 2001, "system": 2002, "varchive": 2003}
CONTROL_IDS = {"close": 2101, "auto_update": 2102}
SCALE_BASE_ID = 2200
VARCHIVE_BASE_ID = 2300
VARCHIVE_AUTO_REFRESH_ID = 2390
WINDOW_BG = win32api.RGB(0xF3, 0xF4, 0xF6)
PANEL_BG = win32api.RGB(0xFF, 0xFF, 0xFF)
TEXT_COLOR = win32api.RGB(0x1F, 0x29, 0x37)
MUTED_TEXT_COLOR = win32api.RGB(0x6B, 0x72, 0x80)
CONTROL_GAP = 8


@dataclass(frozen=True)
class SettingsWindowDiagnostics:
    hwnd_created: bool
    trackbar_created: bool
    scale_button_count: int
    system_checkbox_created: bool
    varchive_session_count: int
    varchive_edit_created: bool
    other_session_count: int
    others_visible: bool
    current_tab: str


class Win32SettingsWindow:
    def __init__(
        self,
        persist: bool = True,
        sessions: Sequence[SteamSession] | None = None,
        current_steam_id: str | None = None,
    ) -> None:
        self.hinst = win32api.GetModuleHandle(None)
        self.hwnd = 0
        self._persist = persist
        self._font = 0
        self._window_brush = win32gui.CreateSolidBrush(WINDOW_BG)
        self._panel_brush = win32gui.CreateSolidBrush(PANEL_BG)
        self._tab = "ui"
        self._sessions_override = list(sessions) if sessions is not None else None
        self._current_steam_id_override = current_steam_id
        self._opacity_cb: Optional[Callable[[float], None]] = None
        self._scale_cb: Optional[Callable[[float], None]] = None
        self._fetch_cb: Optional[Callable[[str, str, int], None]] = None
        self._sync_cb: Optional[Callable[[str, str, str], None]] = None
        self._account_file_cb: Optional[Callable[[str, str], None]] = None
        self._opacity_track = 0
        self._opacity_value = 0
        self._auto_update = 0
        self._tab_buttons: dict[str, int] = {}
        self._scale_buttons: dict[float, int] = {}
        self._v_id_edits: dict[str, int] = {}
        self._account_edits: dict[str, int] = {}
        self._varchive_actions: dict[int, tuple[str, str, int]] = {}
        self._other_session_controls: list[int] = []
        self._toggle_others_hwnd = 0
        self._other_sessions_visible = False
        self._ui_controls: list[int] = []
        self._system_controls: list[int] = []
        self._varchive_controls: list[int] = []
        self._register_class()

    def set_opacity_callback(self, callback: Callable[[float], None]) -> None:
        self._opacity_cb = callback

    def set_scale_callback(self, callback: Callable[[float], None]) -> None:
        self._scale_cb = callback

    def set_fetch_varchive_callback(self, callback: Callable[[str, str, int], None]) -> None:
        self._fetch_cb = callback

    def set_sync_callback(self, callback: Callable[[str, str, str], None]) -> None:
        self._sync_cb = callback

    def set_account_file_callback(self, callback: Callable[[str, str], None]) -> None:
        self._account_file_cb = callback

    def refresh_current_steam_indicator(self) -> None:
        if not self.hwnd:
            return
        was_visible = bool(win32gui.IsWindowVisible(self.hwnd))
        win32gui.DestroyWindow(self.hwnd)
        self._reset_control_state()
        if was_visible:
            self.show_window()

    def show_window(self) -> None:
        if not self._ensure_window():
            return
        win32gui.ShowWindow(self.hwnd, win32con.SW_SHOWNORMAL)
        win32gui.SetForegroundWindow(self.hwnd)

    def hide(self) -> None:
        if self.hwnd:
            win32gui.ShowWindow(self.hwnd, win32con.SW_HIDE)

    def pump(self, millis: int = 30) -> None:
        deadline = time.time() + max(0, millis) / 1000.0
        while time.time() < deadline:
            win32gui.PumpWaitingMessages()
            time.sleep(0.01)

    def diagnostics(self) -> SettingsWindowDiagnostics:
        created = self._ensure_window()
        return SettingsWindowDiagnostics(
            hwnd_created=bool(created and self.hwnd),
            trackbar_created=bool(self._opacity_track),
            scale_button_count=len(self._scale_buttons),
            system_checkbox_created=bool(self._auto_update),
            varchive_session_count=len(self._v_id_edits),
            varchive_edit_created=bool(self._v_id_edits),
            other_session_count=len(self._other_session_controls),
            others_visible=self._other_sessions_visible,
            current_tab=self._tab,
        )

    def simulate_opacity_change(self, value: int) -> float:
        self._ensure_window()
        clamped = max(1, min(10, int(value)))
        win32gui.SendMessage(self._opacity_track, TBM_SETPOS, True, clamped)
        return self._apply_opacity_from_track()

    def simulate_scale_change(self, scale: float) -> float:
        self._ensure_window()
        self._apply_scale(scale)
        return float(SETTINGS.get("overlay", {}).get("scale", 1.0))

    def simulate_tab(self, tab: str) -> None:
        self._ensure_window()
        self._switch_tab(tab)

    def simulate_toggle_others(self) -> None:
        self._ensure_window()
        self._toggle_other_sessions()

    def simulate_varchive_fetch(self, steam_id: str, v_id: str, button: int) -> None:
        self._ensure_window()
        self._set_edit_text(self._v_id_edits[steam_id], v_id)
        self._apply_v_id(steam_id)
        self._emit_fetch(steam_id, v_id, button)

    def simulate_sync(self, steam_id: str, account_path: str) -> None:
        self._ensure_window()
        self._set_edit_text(self._account_edits[steam_id], account_path)
        self._apply_account_path(steam_id)
        self._emit_sync(steam_id)

    def _ensure_window(self) -> bool:
        if self.hwnd and win32gui.IsWindow(self.hwnd):
            return True
        win32gui.InitCommonControls()
        self.hwnd = create_window(self.hinst, self._create_spec())
        if not self.hwnd:
            return False
        self._font = controls.create_font()
        self._create_controls()
        self._switch_tab("ui")
        return True

    def _create_spec(self) -> WindowCreateSpec:
        return WindowCreateSpec(
            class_name=CLASS_NAME,
            title="Overmax 설정",
            ex_style=win32con.WS_EX_TOPMOST | win32con.WS_EX_TOOLWINDOW,
            style=win32con.WS_OVERLAPPED | win32con.WS_CAPTION | win32con.WS_SYSMENU,
            position=controls.center_position(WINDOW_SIZE),
            size=WINDOW_SIZE,
        )

    def _create_controls(self) -> None:
        self._create_tabs()
        self._create_ui_controls()
        self._create_system_controls()
        self._create_varchive_controls()
        for hwnd in self._all_child_hwnds():
            win32gui.SendMessage(hwnd, win32con.WM_SETFONT, self._font, True)

    def _create_tabs(self) -> None:
        x = 18
        for key, text in (("ui", "UI"), ("system", "System"), ("varchive", "V-Archive")):
            width = self._button_width(text)
            hwnd = controls.button(self.hwnd, self.hinst, text, TAB_IDS[key], (x, 18, width, 28))
            self._tab_buttons[key] = hwnd
            x += width + CONTROL_GAP

    def _create_ui_controls(self) -> None:
        ui_controls = self._ui_controls
        ui_controls.append(controls.static(self.hwnd, self.hinst, "오버레이 투명도", (28, 76, 180, 22)))
        self._opacity_value = controls.static(self.hwnd, self.hinst, "", (212, 76, 60, 22))
        ui_controls.append(self._opacity_value)
        self._opacity_track = controls.trackbar(self.hwnd, self.hinst, TRACKBAR_CLASS, (28, 104, 380, 34))
        ui_controls.append(self._opacity_track)
        self._initialize_opacity_track()
        ui_controls.append(controls.static(self.hwnd, self.hinst, "오버레이 크기", (28, 160, 160, 22)))
        self._create_scale_buttons()

    def _create_scale_buttons(self) -> None:
        x = 28
        current = float(SETTINGS.get("overlay", {}).get("scale", 1.0))
        for index, (text, scale) in enumerate(SCALE_PRESETS):
            width = self._button_width(text)
            hwnd = controls.button(self.hwnd, self.hinst, text, SCALE_BASE_ID + index, (x, 192, width, 30))
            self._scale_buttons[scale] = hwnd
            self._ui_controls.append(hwnd)
            self._set_button_checked(hwnd, abs(scale - current) < 0.01)
            x += width + CONTROL_GAP

    def _create_system_controls(self) -> None:
        auto = bool(SETTINGS.get("app_update", {}).get("enabled", True))
        text = "자동 업데이트"
        width = self._check_width(text)
        self._auto_update = controls.check(self.hwnd, self.hinst, text, CONTROL_IDS["auto_update"], (28, 76, width, 24))
        self._set_button_checked(self._auto_update, auto)
        self._system_controls.append(self._auto_update)
        version = controls.static(self.hwnd, self.hinst, f"현재 버전: {APP_VERSION}", (28, 116, 220, 22))
        self._system_controls.append(version)

    def _create_varchive_controls(self) -> None:
        sessions = self._settings_sessions()
        self._create_auto_refresh_control()
        if not sessions:
            text = "발견된 Steam 계정이 없습니다."
            self._varchive_controls.append(controls.static(self.hwnd, self.hinst, text, (28, 112, 300, 24)))
            return
        self._create_session_controls(sessions[0], 112)
        if len(sessions) > 1:
            self._create_other_sessions(sessions[1:], 238)

    def _create_auto_refresh_control(self) -> None:
        auto = bool(SETTINGS.get("varchive", {}).get("auto_refresh", False))
        text = "시작 시 자동 갱신"
        hwnd = controls.check(self.hwnd, self.hinst, text, VARCHIVE_AUTO_REFRESH_ID, (28, 76, self._check_width(text), 24))
        self._set_button_checked(hwnd, auto)
        self._varchive_controls.append(hwnd)

    def _create_session_controls(self, session: SteamSession, top: int) -> None:
        self._varchive_controls.append(controls.static(self.hwnd, self.hinst, _session_label(session), (28, top, 360, 22)))
        self._create_v_id_row(session, top + 32)
        self._create_account_row(session, top + 72)

    def _create_other_sessions(self, sessions: list[SteamSession], top: int) -> None:
        text = f"다른 계정 보기 ({len(sessions)})"
        self._toggle_others_hwnd = controls.button(
            self.hwnd, self.hinst, text, VARCHIVE_BASE_ID + 200, (28, top, self._button_width(text), 24)
        )
        self._varchive_actions[VARCHIVE_BASE_ID + 200] = ("toggle_others", "", 0)
        self._varchive_controls.append(self._toggle_others_hwnd)
        y = top + 34
        for session in sessions:
            before = len(self._varchive_controls)
            self._create_session_controls(session, y)
            self._other_session_controls.extend(self._varchive_controls[before:])
            y += 124
        controls.show_many(self._other_session_controls, False)

    def _create_v_id_row(self, session: SteamSession, y: int) -> None:
        v_id = _varchive_entry(session.steam_id).get("v_id", "")
        edit = controls.edit(self.hwnd, self.hinst, v_id, (28, y, 126, 24))
        self._v_id_edits[session.steam_id] = edit
        self._varchive_controls.append(edit)
        x = 162
        for button in (4, 5, 6, 8, 0):
            text = "All" if button == 0 else f"{button}B"
            control_id = VARCHIVE_BASE_ID + len(self._varchive_actions)
            hwnd = controls.button(self.hwnd, self.hinst, text, control_id, (x, y, self._button_width(text), 24))
            self._varchive_actions[control_id] = ("fetch", session.steam_id, button)
            self._varchive_controls.append(hwnd)
            x += self._button_width(text) + 4

    def _create_account_row(self, session: SteamSession, y: int) -> None:
        entry = _varchive_entry(session.steam_id)
        edit = controls.edit(self.hwnd, self.hinst, entry.get("account_path", ""), (28, y, 250, 24))
        self._account_edits[session.steam_id] = edit
        self._varchive_controls.append(edit)
        control_id = VARCHIVE_BASE_ID + len(self._varchive_actions)
        browse_id = control_id
        browse = controls.button(self.hwnd, self.hinst, "찾기", browse_id, (286, y, self._button_width("찾기"), 24))
        self._varchive_actions[browse_id] = ("browse", session.steam_id, 0)
        self._varchive_controls.append(browse)
        control_id = VARCHIVE_BASE_ID + len(self._varchive_actions)
        button = controls.button(self.hwnd, self.hinst, "동기화 후보", control_id, (348, y, self._button_width("동기화 후보"), 24))
        self._varchive_actions[control_id] = ("sync", session.steam_id, 0)
        self._varchive_controls.append(button)

    def _wnd_proc(self, hwnd: int, msg: int, wparam: int, lparam: int) -> int:
        if msg == win32con.WM_COMMAND:
            self._handle_command(win32api.LOWORD(wparam))
            return 0
        if msg == win32con.WM_HSCROLL and lparam == self._opacity_track:
            self._apply_opacity_from_track()
            return 0
        if msg == win32con.WM_ERASEBKGND:
            win32gui.FillRect(wparam, win32gui.GetClientRect(hwnd), self._window_brush)
            return 1
        if msg == win32con.WM_CTLCOLOREDIT:
            return self._paint_edit_background(wparam)
        if msg in (win32con.WM_CTLCOLORSTATIC, win32con.WM_CTLCOLORBTN):
            return self._paint_control_background(wparam)
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    def _handle_command(self, control_id: int) -> None:
        tab = next((key for key, value in TAB_IDS.items() if value == control_id), None)
        if tab:
            self._switch_tab(tab)
        elif control_id == CONTROL_IDS["auto_update"]:
            self._apply_auto_update()
        elif control_id == VARCHIVE_AUTO_REFRESH_ID:
            self._apply_auto_refresh()
        elif control_id in self._varchive_actions:
            self._handle_varchive_action(control_id)
        elif SCALE_BASE_ID <= control_id < SCALE_BASE_ID + len(SCALE_PRESETS):
            self._apply_scale(SCALE_PRESETS[control_id - SCALE_BASE_ID][1])

    def _switch_tab(self, tab: str) -> None:
        self._tab = tab
        controls.show_many(self._ui_controls, tab == "ui")
        controls.show_many(self._system_controls, tab == "system")
        controls.show_many(self._varchive_controls, tab == "varchive")

    def _initialize_opacity_track(self) -> None:
        value = round(float(SETTINGS.get("overlay", {}).get("base_opacity", 0.8)) * 10)
        win32gui.SendMessage(self._opacity_track, TBM_SETRANGE, True, win32api.MAKELONG(1, 10))
        win32gui.SendMessage(self._opacity_track, TBM_SETPOS, True, max(1, min(10, value)))
        self._update_opacity_label(value / 10.0)

    def _apply_opacity_from_track(self) -> float:
        value = int(win32gui.SendMessage(self._opacity_track, TBM_GETPOS, 0, 0)) / 10.0
        self._update_opacity_label(value)
        SETTINGS.setdefault("overlay", {})["base_opacity"] = value
        self._save_if_enabled()
        if self._opacity_cb:
            self._opacity_cb(value)
        return value

    def _apply_scale(self, scale: float) -> None:
        SETTINGS.setdefault("overlay", {})["scale"] = scale
        for value, hwnd in self._scale_buttons.items():
            self._set_button_checked(hwnd, abs(value - scale) < 0.01)
        self._save_if_enabled()
        if self._scale_cb:
            self._scale_cb(scale)

    def _apply_auto_update(self) -> None:
        checked = self._button_checked(self._auto_update)
        SETTINGS.setdefault("app_update", {})["enabled"] = checked
        self._save_if_enabled()

    def _apply_auto_refresh(self) -> None:
        hwnd = self._find_control(VARCHIVE_AUTO_REFRESH_ID)
        SETTINGS.setdefault("varchive", {})["auto_refresh"] = self._button_checked(hwnd)
        self._save_if_enabled()

    def _handle_varchive_action(self, control_id: int) -> None:
        action, steam_id, button = self._varchive_actions[control_id]
        if action == "fetch":
            self._apply_v_id(steam_id)
            self._emit_fetch(steam_id, self._edit_text(self._v_id_edits[steam_id]), button)
        elif action == "sync":
            self._apply_account_path(steam_id)
            self._emit_sync(steam_id)
        elif action == "browse":
            self._browse_account_file(steam_id)
        elif action == "toggle_others":
            self._toggle_other_sessions()

    def _apply_v_id(self, steam_id: str) -> None:
        entry = _ensure_varchive_entry(steam_id)
        entry["v_id"] = self._edit_text(self._v_id_edits[steam_id]).strip()
        self._save_if_enabled()

    def _apply_account_path(self, steam_id: str) -> None:
        entry = _ensure_varchive_entry(steam_id)
        entry["account_path"] = self._edit_text(self._account_edits[steam_id]).strip()
        self._save_if_enabled()
        if self._account_file_cb:
            self._account_file_cb(steam_id, entry["account_path"])

    def _emit_fetch(self, steam_id: str, v_id: str, button: int) -> None:
        if self._fetch_cb:
            self._fetch_cb(steam_id, v_id.strip(), button)

    def _emit_sync(self, steam_id: str) -> None:
        if self._sync_cb:
            session = self._session_by_id(steam_id)
            self._sync_cb(steam_id, session.persona_name if session else "", self._edit_text(self._account_edits[steam_id]).strip())

    def _browse_account_file(self, steam_id: str) -> None:
        path = _open_account_file_dialog()
        if not path:
            return
        self._set_edit_text(self._account_edits[steam_id], path)
        self._apply_account_path(steam_id)

    def _toggle_other_sessions(self) -> None:
        self._other_sessions_visible = not self._other_sessions_visible
        controls.show_many(self._other_session_controls, self._other_sessions_visible)
        label = "다른 계정 숨기기" if self._other_sessions_visible else "다른 계정 보기"
        win32gui.SetWindowText(self._toggle_others_hwnd, label)

    def _update_opacity_label(self, value: float) -> None:
        win32gui.SetWindowText(self._opacity_value, f"{value:.1f}")

    def _paint_control_background(self, hdc: int) -> int:
        win32gui.SetBkColor(hdc, WINDOW_BG)
        win32gui.SetTextColor(hdc, TEXT_COLOR)
        return self._window_brush

    def _paint_edit_background(self, hdc: int) -> int:
        win32gui.SetBkColor(hdc, PANEL_BG)
        win32gui.SetTextColor(hdc, TEXT_COLOR)
        return self._panel_brush

    def _save_if_enabled(self) -> None:
        if self._persist:
            save_settings()

    def _settings_sessions(self) -> list[SteamSession]:
        sessions = list(self._sessions_override) if self._sessions_override is not None else get_all_steam_sessions()
        current = self._current_steam_id_override or get_most_recent_steam_id()
        sessions.sort(key=lambda session: session.steam_id != current)
        return sessions

    def _session_by_id(self, steam_id: str) -> SteamSession | None:
        return next((session for session in self._settings_sessions() if session.steam_id == steam_id), None)

    def _find_control(self, control_id: int) -> int:
        for hwnd in self._all_child_hwnds():
            if win32gui.GetDlgCtrlID(hwnd) == control_id:
                return hwnd
        return 0

    def _edit_text(self, hwnd: int) -> str:
        return win32gui.GetWindowText(hwnd)

    def _set_edit_text(self, hwnd: int, text: str) -> None:
        win32gui.SetWindowText(hwnd, text)

    def _reset_control_state(self) -> None:
        self.hwnd = 0
        self._opacity_track = 0
        self._opacity_value = 0
        self._auto_update = 0
        self._tab_buttons.clear()
        self._scale_buttons.clear()
        self._v_id_edits.clear()
        self._account_edits.clear()
        self._varchive_actions.clear()
        self._ui_controls.clear()
        self._system_controls.clear()
        self._varchive_controls.clear()
        self._other_session_controls.clear()
        self._toggle_others_hwnd = 0
        self._other_sessions_visible = False

    def _all_child_hwnds(self) -> list[int]:
        return [
            *self._tab_buttons.values(),
            *self._ui_controls,
            *self._system_controls,
            *self._varchive_controls,
        ]

    def _button_width(self, text: str) -> int:
        return max(72, controls.text_width(self.hwnd, self._font, text) + 28)

    def _check_width(self, text: str) -> int:
        return max(120, controls.text_width(self.hwnd, self._font, text) + 32)

    def _button_checked(self, hwnd: int) -> bool:
        return win32gui.SendMessage(hwnd, win32con.BM_GETCHECK, 0, 0) == win32con.BST_CHECKED

    def _set_button_checked(self, hwnd: int, checked: bool) -> None:
        state = win32con.BST_CHECKED if checked else win32con.BST_UNCHECKED
        win32gui.SendMessage(hwnd, win32con.BM_SETCHECK, state, 0)

    def _register_class(self) -> None:
        register_window_class(self.hinst, CLASS_NAME, self._wnd_proc)


def _session_label(session: SteamSession) -> str:
    marker = "Current - " if session.most_recent else ""
    return f"{marker}{session.persona_name} ({session.account_name})"


def _ensure_varchive_entry(steam_id: str) -> dict:
    SETTINGS.setdefault("varchive", {})
    user_map = SETTINGS["varchive"].setdefault("user_map", {})
    entry = user_map.setdefault(steam_id, {})
    if isinstance(entry, str):
        entry = {"v_id": entry, "account_path": ""}
        user_map[steam_id] = entry
    return entry


def _varchive_entry(steam_id: str) -> dict[str, str]:
    entry = _ensure_varchive_entry(steam_id)
    return {
        "v_id": str(entry.get("v_id", "")),
        "account_path": str(entry.get("account_path", "")),
    }


def _open_account_file_dialog() -> str:
    try:
        filename, _custom_filter, _flags = win32gui.GetOpenFileNameW(
            Title="account.txt 선택",
            File="account.txt",
            DefExt="txt",
            Filter="Text Files (*.txt)\0*.txt\0All Files (*.*)\0*.*\0",
            Flags=win32con.OFN_FILEMUSTEXIST | win32con.OFN_PATHMUSTEXIST,
        )
        return str(filename)
    except Exception:
        return ""
