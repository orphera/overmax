"""Overlay controller that bridges runtime events to Qt UI."""

import os
import sys
import threading
from typing import Optional

from settings import SETTINGS
import runtime_patch

try:
    from PyQt6.QtWidgets import QApplication, QSystemTrayIcon
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

from data.varchive import VArchiveDB
from data.recommend import Recommender
from data.varchive_client import VArchiveRecordClient
from data.varchive_uploader import parse_account_file
from core.game_state import GameSessionState
from overlay.ui.navigation import RoiOverlayWindow
from overlay.ui_payload import OverlayPayloadBuilder, OverlayUpdatePayload
from overlay.window import OverlaySignals, OverlayWindow
from overlay.settings_window import SettingsWindow
from overlay.sync_window import SyncWindow
from overlay.win32.view_state import apply_payload_to_view_state, default_view_state
from overlay.win32.window import Win32OverlayWindow, set_process_dpi_awareness

OVERLAY_BACKEND_PYQT6 = "pyqt6"
OVERLAY_BACKEND_WIN32 = "win32"
OVERLAY_BACKENDS = {OVERLAY_BACKEND_PYQT6, OVERLAY_BACKEND_WIN32}


def _resolve_overlay_backend() -> str:
    cli_backend = _resolve_overlay_backend_from_argv(sys.argv[1:])
    raw_backend = cli_backend or os.getenv("OVERMAX_OVERLAY_BACKEND")
    if raw_backend is None:
        raw_backend = SETTINGS.get("overlay", {}).get("main_backend", OVERLAY_BACKEND_PYQT6)

    backend = str(raw_backend).strip().lower()
    if backend in OVERLAY_BACKENDS:
        return backend
    print(f"[Overlay] 알 수 없는 main_backend={backend!r}, pyqt6로 실행")
    return OVERLAY_BACKEND_PYQT6


def _resolve_overlay_backend_from_argv(argv: list[str]) -> Optional[str]:
    for arg in argv:
        if arg == "--win32-overlay":
            return OVERLAY_BACKEND_WIN32
        if arg.startswith("--overlay-backend="):
            return arg.split("=", 1)[1]
    return None


def _qt_argv() -> list[str]:
    return [
        arg for arg in sys.argv
        if arg != "--win32-overlay" and not arg.startswith("--overlay-backend=")
    ]


class OverlayController:
    def __init__(self, db: VArchiveDB, record_db, varchive_client: Optional[VArchiveRecordClient] = None):
        self.db = db
        self.record_db = record_db
        self.varchive_client = varchive_client
        self.recommender = Recommender(db, record_db)
        self.payload_builder = OverlayPayloadBuilder(db, self.recommender, self.log)
        self.signals = OverlaySignals()
        self._app: Optional[QApplication] = None
        self._window: Optional[OverlayWindow | Win32OverlayWindow] = None
        self._overlay_backend = _resolve_overlay_backend()
        self._win32_view_state = default_view_state()
        self._roi_window: Optional[RoiOverlayWindow] = None
        self._sync_window: Optional[SyncWindow] = None
        self._settings_window: Optional[SettingsWindow] = None
        self._tray_icon: Optional[QSystemTrayIcon] = None
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
        # 하위 호환: 구버전 단일 account_path
        legacy_path = varchive_cfg.get("account_path", "")
        return str(legacy_path) if legacy_path else ""

    def _emit_initial_state(self):
        self._emit_payload(self.payload_builder.build_initial())

    def notify_screen(self, is_song_select: bool):
        self.log(f"화면 알림: {'선곡화면' if is_song_select else '기타화면'}")
        self.signals.screen_changed.emit(is_song_select)
        if self._using_win32_overlay():
            self._show_or_hide_win32_overlay(is_song_select)

    def notify_confidence(self, confidence: float):
        self.signals.confidence_changed.emit(confidence)

    def notify_window_pos(self, left, top, width, height):
        self.log(f"창 위치: ({left},{top}) {width}x{height}")
        self._last_window_rect = (left, top, width, height)
        self.signals.position_changed.emit(left, top, width, height)
        if self._using_win32_overlay():
            self._window.move_to_game_rect(left, top, width, height)

    def notify_window_lost(self):
        self.log("게임 창 소실 알림 수신: 오버레이 숨김 + ROI OFF")
        self._last_window_rect = None
        self.signals.screen_changed.emit(False)
        self.signals.roi_enabled_changed.emit(False)
        self.signals.position_changed.emit(0, 0, 0, 0)
        if self._using_win32_overlay():
            self._window.hide()

    def notify_state(self, state: GameSessionState):
        """인식된 게임 상태를 수신하여 UI 시그널 일괄 처리(Batch)."""
        payload = self.payload_builder.build_state_update(state)
        self._emit_payload(payload)

    def _emit_payload(self, payload: OverlayUpdatePayload):
        if self._using_win32_overlay():
            self._update_win32_payload(payload)
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

    def set_roi_overlay_enabled(self, enabled: bool):
        if self._roi_window is None:
            return

        self._roi_window.set_enabled(enabled)
        self.log(f"ROI 영역 표시: {'ON' if enabled else 'OFF'}")
        if enabled and self._last_window_rect is not None:
            left, top, width, height = self._last_window_rect
            self._roi_window.set_game_rect(left, top, width, height)

    def toggle_roi_overlay(self):
        if self._roi_window is None:
            return False
        new_state = not self._roi_window.is_enabled()
        self.set_roi_overlay_enabled(new_state)
        return new_state

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
        if self._using_win32_overlay():
            self._window.toggle_visibility()
            return
        self.signals.visibility_toggle_requested.emit()

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
                # RecordManager에게 캐시 다시 읽으라고 알림
                if hasattr(self.record_db, "refresh"):
                    self.record_db.refresh()
                
                self.notify_record_updated()

        threading.Thread(target=work, daemon=True).start()

    def _on_account_file_changed(self, steam_id: str, path: str):
        from data.varchive_uploader import parse_account_file
        account = parse_account_file(path) if path else None
        if self._sync_window:
            self._sync_window.set_account(steam_id, account)

    def run(self, debug_ctrl=None):
        if not PYQT_AVAILABLE:
            print("[Overlay] PyQt6 없음, 콘솔 모드로 실행")
            import time

            while True:
                time.sleep(1)
            return

        self._app = QApplication(_qt_argv())
        self._app.setQuitOnLastWindowClosed(False)
        self._window = self._create_main_overlay()
        self._window.hide()
        self._window.set_user_move_callback(self._on_overlay_user_moved)

        self._settings_window = SettingsWindow()
        self._settings_window.hide()
        if isinstance(self._window, OverlayWindow):
            self._settings_window.opacity_changed.connect(self._window.update_base_opacity)
        self._settings_window.scale_changed.connect(self.signals.scale_changed)
        self._settings_window.fetch_varchive_requested.connect(self._on_fetch_varchive)
        self.signals.settings_requested.connect(self._settings_window.show_window)

        self._sync_window = SyncWindow(self.db, self.record_db)

        # 시그널 연결
        self._settings_window.sync_requested.connect(
            lambda steam_id, persona_name, account_path: self._sync_window.show_window(steam_id, persona_name, account_path)
        )
        self._settings_window.account_file_changed.connect(self._on_account_file_changed)

        self._roi_window = RoiOverlayWindow()
        self._roi_window.hide()
        self.signals.position_changed.connect(self._roi_window.set_game_rect)
        self.signals.roi_enabled_changed.connect(self._roi_window.set_enabled)

        self._emit_initial_state()
        self._restore_window_position()
        self._setup_debug(debug_ctrl)
        self._setup_tray_icon()
        
        # 시작 시 자동 갱신
        self._handle_auto_refresh()
        
        self._app.exec()

    def _create_main_overlay(self):
        if self._overlay_backend == OVERLAY_BACKEND_WIN32:
            set_process_dpi_awareness()
            self.log("메인 오버레이 backend: win32")
            return Win32OverlayWindow(self._win32_view_state)
        self.log("메인 오버레이 backend: pyqt6")
        return OverlayWindow(self.signals)

    def _using_win32_overlay(self) -> bool:
        return isinstance(self._window, Win32OverlayWindow)

    def _update_win32_payload(self, payload: OverlayUpdatePayload) -> None:
        self._win32_view_state = apply_payload_to_view_state(
            self._win32_view_state,
            payload,
        )
        self._window.update_view_state(self._win32_view_state)

    def _show_or_hide_win32_overlay(self, should_show: bool) -> None:
        if should_show:
            self._window.show()
            return
        self._window.hide()

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
            self.log(f"자동 갱신 시작 (SteamID: {sid}, V-ID: {v_id})")
            self._on_fetch_varchive(sid, v_id, 0) # 0 for all buttons

    def _restore_window_position(self):
        if self._window is None or self._app is None:
            return

        overlay_cfg = SETTINGS.get("overlay", {})
        pos = overlay_cfg.get("position")
        if pos is None:
            return

        sx, sy = int(pos["x"]), int(pos["y"])
        if self._using_win32_overlay():
            self._window.apply_saved_position(sx, sy)
            self.log(f"오버레이 위치 복원: ({sx},{sy})")
            return

        screen = self._app.primaryScreen().geometry()
        sx = max(0, min(sx, max(0, screen.width() - self._window.width())))
        sy = max(0, min(sy, max(0, screen.height() - self._window.height())))
        self._window.apply_saved_position(sx, sy)
        self.log(f"오버레이 위치 복원: ({sx},{sy})")

    def _setup_debug(self, debug_ctrl):
        if debug_ctrl is None:
            self._debug_toggle_cb = None
            return
        debug_ctrl.create_window()
        debug_ctrl.set_roi_toggle_callback(self.set_roi_overlay_enabled)
        self._debug_toggle_cb = debug_ctrl.toggle_window

    def _setup_tray_icon(self):
        from overlay.tray_icon import create_overlay_tray_icon

        self._tray_icon = create_overlay_tray_icon(
            app=self._app,
            window=self._window,
            settings_window=self._settings_window,
            debug_toggle_cb=self._debug_toggle_cb,
        )
