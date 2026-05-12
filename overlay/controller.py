"""Overlay controller that bridges runtime events to Win32 UI."""

from __future__ import annotations

import os
import sys
import threading
import win32api
import win32con
import win32gui
from typing import Optional

from settings import SETTINGS
from data.varchive import VArchiveDB
from data.recommend import Recommender
from data.varchive_client import VArchiveRecordClient
from data.varchive_uploader import parse_account_file
from core.game_state import GameSessionState
from overlay.ui_payload import OverlayPayloadBuilder, OverlayUpdatePayload
from overlay.signals import OverlaySignals
from overlay.win32.settings_window import Win32SettingsWindow
from overlay.win32.sync_window import Win32SyncWindow
from overlay.win32.view_state import apply_payload_to_view_state, default_view_state
from overlay.win32.window import Win32OverlayWindow, set_process_dpi_awareness
from infra.gui.tray import Win32TrayIcon, TrayMenuItem
from constants import TOGGLE_HOTKEY, TRAY_TOOLTIP


class OverlayController:
    def __init__(self, db: VArchiveDB, record_db, varchive_client: Optional[VArchiveRecordClient] = None):
        self.db = db
        self.record_db = record_db
        self.varchive_client = varchive_client
        self.recommender = Recommender(db, record_db)
        self.payload_builder = OverlayPayloadBuilder(db, self.recommender, self.log)
        self.signals = OverlaySignals()
        
        self._window: Optional[Win32OverlayWindow] = None
        self._win32_view_state = default_view_state()
        self._settings_window: Optional[Win32SettingsWindow] = None
        self._sync_window: Optional[Win32SyncWindow] = None
        self._tray_icon: Optional[Win32TrayIcon] = None
        
        self._debug_log_cb = None
        self._debug_toggle_cb = None
        self._last_window_rect: Optional[tuple[int, int, int, int]] = None

    def _get_account_path_for_steam_id(self, steam_id: str) -> str:
        varchive_cfg = SETTINGS.get("varchive", {})
        user_map = varchive_cfg.get("user_map", {})
        entry = user_map.get(steam_id, {}) if isinstance(user_map, dict) else {}
        if isinstance(entry, dict):
            path = entry.get("account_path", "")
            if path:
                return str(path)
        legacy_path = varchive_cfg.get("account_path", "")
        return str(legacy_path) if legacy_path else ""

    def _emit_initial_state(self):
        self._emit_payload(self.payload_builder.build_initial())

    def notify_screen(self, is_song_select: bool):
        self.log(f"화면 알림: {'선곡화면' if is_song_select else '기타화면'}")
        self.signals.screen_changed.emit(is_song_select)
        if is_song_select:
            self._window.show()
        else:
            self._window.hide()

    def notify_confidence(self, confidence: float):
        self.signals.confidence_changed.emit(confidence)
        if self._window:
            self._window.update_confidence(confidence)

    def notify_window_pos(self, left, top, width, height):
        self._last_window_rect = (left, top, width, height)
        self.signals.position_changed.emit(left, top, width, height)
        if self._window:
            self._window.move_to_game_rect(left, top, width, height)

    def notify_window_lost(self):
        self.log("게임 창 소실 알림 수신: 오버레이 숨김")
        self._last_window_rect = None
        self.signals.screen_changed.emit(False)
        self.signals.position_changed.emit(0, 0, 0, 0)
        if self._window:
            self._window.hide()

    def notify_state(self, state: GameSessionState):
        payload = self.payload_builder.build_state_update(state)
        self._emit_payload(payload)

    def _emit_payload(self, payload: OverlayUpdatePayload):
        # Update view state
        self._win32_view_state = apply_payload_to_view_state(
            self._win32_view_state,
            payload,
        )
        if self._window:
            self._window.update_view_state(self._win32_view_state)

        # Emit individual signals for windows that might need them
        if payload.status_changed is not None:
            self.signals.status_changed.emit(payload.status_changed)
        if payload.song is not None:
            self.signals.song_changed.emit(payload.song.title, payload.song.all_patterns)
        if payload.mode_diff is not None:
            self.signals.mode_diff_changed.emit(payload.mode_diff.mode, payload.mode_diff.diff)
        if payload.recommendations is not None:
            self.signals.recommend_ready.emit(
                payload.recommendations.result,
                payload.recommendations.no_selection,
            )

    def notify_record_updated(self):
        self._refresh_recommendations()

    def refresh_settings_steam_session(self):
        if self._settings_window:
            self._settings_window.refresh_current_steam_indicator()
        
        if self._sync_window:
            steam_id = self.record_db.get_steam_id() if self.record_db else "__unknown__"
            account_path = self._get_account_path_for_steam_id(steam_id)
            account = parse_account_file(account_path) if account_path else None
            self._sync_window.set_account(steam_id, account)

    def _refresh_recommendations(self):
        payload = self.payload_builder.build_recommendation_refresh()
        self._emit_payload(OverlayUpdatePayload(recommendations=payload))

    def log(self, msg: str):
        full = f"[Overlay] {msg}"
        print(full)
        if self._debug_log_cb:
            self._debug_log_cb(full)

    def _on_overlay_user_moved(self, x: int, y: int):
        if "overlay" not in SETTINGS:
            SETTINGS["overlay"] = {}
        SETTINGS["overlay"]["position"] = {"x": int(x), "y": int(y)}
        from settings import save_settings
        save_settings()
        self.log(f"오버레이 위치 저장 (user.json): ({x},{y})")

    def toggle_visibility(self):
        if self._window:
            self._window.toggle_visibility()

    def _on_fetch_varchive(self, steam_id: str, v_id: str, button: int):
        if not self.varchive_client:
            self.log("VArchiveClient가 초기화되지 않았습니다.")
            return
        if not v_id:
            self.log("V-Archive ID가 입력되지 않았습니다.")
            return

        def work():
            buttons = [4, 5, 6, 8] if button == 0 else [button]
            success_count = 0
            for b in buttons:
                self.log(f"V-Archive 기록 요청 중: {v_id} ({b}B)")
                data = self.varchive_client.fetch_records(v_id, b)
                if data:
                    self.varchive_client.save_to_cache(steam_id, v_id, b, data)
                    success_count += 1
                else:
                    self.log(f"V-Archive {b}B 기록 요청 실패")
            
            if success_count > 0:
                self.log(f"V-Archive 기록 {success_count}개 모드 갱신 완료")
                if hasattr(self.record_db, "refresh"):
                    self.record_db.refresh()
                self.notify_record_updated()

        threading.Thread(target=work, daemon=True).start()

    def _on_account_file_changed(self, steam_id: str, path: str):
        account = parse_account_file(path) if path else None
        if self._sync_window:
            self._sync_window.set_account(steam_id, account)

    def run(self, debug_ctrl=None):
        set_process_dpi_awareness()
        
        # Initialize windows
        self._window = Win32OverlayWindow(self._win32_view_state)
        self._window.hide()
        self._window.set_user_move_callback(self._on_overlay_user_moved)
        self._window.set_settings_callback(lambda: self.signals.settings_requested.emit())
        
        self._settings_window = Win32SettingsWindow()
        self._settings_window.hide()
        
        self._sync_window = Win32SyncWindow(self.db, self.record_db)
        
        # Connect callbacks
        self._settings_window.set_opacity_callback(self._window.update_base_opacity)
        self._settings_window.set_scale_callback(self.signals.scale_changed.emit)
        self._settings_window.set_fetch_varchive_callback(self._on_fetch_varchive)
        self._settings_window.set_sync_callback(self._sync_window.show_window)
        self._settings_window.set_account_file_callback(self._on_account_file_changed)
        
        self.signals.scale_changed.connect(self._window.rebuild_ui)
        self.signals.settings_requested.connect(self._settings_window.show_window)
        
        self._emit_initial_state()
        self._restore_window_position()
        self._setup_debug(debug_ctrl)
        self._setup_tray_icon()
        self._handle_auto_refresh()
        
        # Start Win32 message pump (for tray icon and windows)
        # Note: Win32OverlayWindow and others run in their own threads or use their own loops,
        # but the tray icon and general message handling need a main pump.
        win32gui.PumpMessages()

    def _handle_auto_refresh(self):
        if not SETTINGS.get("varchive", {}).get("auto_refresh", False):
            return
        from data.steam_session import get_most_recent_steam_id
        sid = get_most_recent_steam_id()
        if not sid:
            return
        entry = SETTINGS.get("varchive", {}).get("user_map", {}).get(sid, {})
        v_id = entry.get("v_id", "") if isinstance(entry, dict) else entry
        if v_id:
            self._on_fetch_varchive(sid, v_id, 0)

    def _restore_window_position(self):
        if self._window is None:
            return
        overlay_cfg = SETTINGS.get("overlay", {})
        pos = overlay_cfg.get("position")
        if pos is None:
            return
        self._window.apply_saved_position(int(pos["x"]), int(pos["y"]))

    def _setup_debug(self, debug_ctrl):
        if debug_ctrl is None:
            return
        debug_ctrl.create_window()
        self._debug_toggle_cb = debug_ctrl.toggle_window

    def _setup_tray_icon(self):
        menu_items = [
            TrayMenuItem(f"오버레이 표시/숨김 ({TOGGLE_HOTKEY})", self.toggle_visibility, True),
            TrayMenuItem("설정", lambda: self.signals.settings_requested.emit()),
        ]
        if self._debug_toggle_cb:
            menu_items.append(TrayMenuItem("디버그 창 표시/숨김", self._debug_toggle_cb))
        
        menu_items.append(TrayMenuItem("", lambda: None)) # Separator
        menu_items.append(TrayMenuItem("종료", lambda: win32gui.PostQuitMessage(0)))
        
        self._tray_icon = Win32TrayIcon(
            TRAY_TOOLTIP,
            menu_items,
            on_double_click=self.toggle_visibility
        )
        self._tray_icon.start()

    def stop(self):
        if self._tray_icon:
            self._tray_icon.stop()
        win32gui.PostQuitMessage(0)
