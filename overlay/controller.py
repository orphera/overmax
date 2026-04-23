"""Overlay controller that bridges runtime events to Qt UI."""

import json
import sys
from typing import Optional

from settings import SETTINGS
import runtime_patch

try:
    from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QStyle
    from PyQt6.QtGui import QAction
    PYQT_AVAILABLE = True
except ImportError:
    PYQT_AVAILABLE = False

from data.varchive import VArchiveDB, BUTTON_MODES
from data.recommend import Recommender
from data.record_db import RecordDB
from core.game_state import GameSessionState
from overlay.ui.navigation import RoiOverlayWindow
from overlay.window import OverlaySignals, OverlayWindow
from overlay.settings_window import SettingsWindow

from constants import (
    TOGGLE_HOTKEY,
    TRAY_TOOLTIP,
)


class OverlayController:
    def __init__(self, db: VArchiveDB, record_db: RecordDB):
        self.db = db
        self.record_db = record_db
        self.recommender = Recommender(db, record_db)
        self.signals = OverlaySignals()
        self._app: Optional[QApplication] = None
        self._window: Optional[OverlayWindow] = None
        self._roi_window: Optional[RoiOverlayWindow] = None
        self._settings_window: Optional[SettingsWindow] = None
        self._tray_icon: Optional[QSystemTrayIcon] = None
        self._debug_log_cb = None
        self._debug_toggle_cb = None

        self._song_id: Optional[int] = None
        self._current_mode: Optional[str] = None
        self._current_diff: Optional[str] = None

        self._last_window_rect: Optional[tuple[int, int, int, int]] = None

    def _emit_initial_state(self):
        all_patterns = [{"mode": mode, "patterns": []} for mode in BUTTON_MODES]
        self.signals.song_changed.emit("곡을 선택하세요", all_patterns)
        self.signals.mode_diff_changed.emit("", "")
        self.signals.recommend_ready.emit([], True)

    def notify_screen(self, is_song_select: bool):
        self.log(f"화면 알림: {'선곡화면' if is_song_select else '기타화면'}")
        self.signals.screen_changed.emit(is_song_select)

    def notify_confidence(self, confidence: float):
        self.signals.confidence_changed.emit(confidence)

    def notify_window_pos(self, left, top, width, height):
        self.log(f"창 위치: ({left},{top}) {width}x{height}")
        self._last_window_rect = (left, top, width, height)
        self.signals.position_changed.emit(left, top, width, height)

    def notify_window_lost(self):
        self.log("게임 창 소실 알림 수신: 오버레이 숨김 + ROI OFF")
        self._last_window_rect = None
        self.signals.screen_changed.emit(False)
        self.signals.roi_enabled_changed.emit(False)
        self.signals.position_changed.emit(0, 0, 0, 0)

    def notify_state(self, state: GameSessionState):
        """인식된 게임 상태를 수신하여 UI 시그널 일괄 처리(Batch)."""
        self._check_and_emit_status(state.is_stable)

        if not state.is_stable:
            return

        song_changed, mode_diff_changed = self._check_state_diff(state)
        if not (song_changed or mode_diff_changed):
            return

        self._update_internal_state(state)
        song_name, all_patterns, recommendations = self._fetch_ui_data()

        self._emit_update_signals(song_changed, mode_diff_changed, song_name, all_patterns, recommendations)

    def _check_and_emit_status(self, is_stable: bool):
        if getattr(self, "_last_verified", None) != is_stable:
            self.signals.status_changed.emit(is_stable)
            self._last_verified = is_stable

    def _check_state_diff(self, state: GameSessionState) -> tuple[bool, bool]:
        song_changed = self._song_id != state.song_id
        mode_diff_changed = (
            self._current_mode != state.mode
            or self._current_diff != state.diff
        )
        return song_changed, mode_diff_changed

    def _update_internal_state(self, state: GameSessionState):
        self._song_id = state.song_id
        self._current_mode = state.mode
        self._current_diff = state.diff

    def _fetch_ui_data(self) -> tuple[str, list, list]:
        song_name = "곡을 선택하세요"
        all_patterns = []
        recommendations = []

        if self._song_id is None:
            return song_name, all_patterns, recommendations

        song = self.db.search_by_id(self._song_id)
        if not song:
            self.log(f"ID={self._song_id}를 DB에서 찾을 수 없음")
            return song_name, all_patterns, recommendations

        song_name = song["name"]
        for mode in BUTTON_MODES:
            pts = self.db.format_pattern_info(song, mode)
            all_patterns.append({"mode": mode, "patterns": pts})

        if self._current_mode and self._current_diff:
            recommendations = self.recommender.recommend(
                song_id=self._song_id,
                button_mode=self._current_mode,
                difficulty=self._current_diff,
            )

        return song_name, all_patterns, recommendations

    def _emit_update_signals(self, song_changed: bool, mode_diff_changed: bool, song_name: str, all_patterns: list, recommendations: list):
        if song_changed:
            if self._song_id is None:
                self._emit_initial_state()
            else:
                self.signals.song_changed.emit(song_name, all_patterns)

        if mode_diff_changed:
            self.signals.mode_diff_changed.emit(
                self._current_mode or "",
                self._current_diff or "",
            )

        if song_changed or mode_diff_changed:
            self.signals.recommend_ready.emit(recommendations, False)

    def notify_record_updated(self):
        self._refresh_recommendations()

    def _refresh_recommendations(self):
        if self._song_id is None or not self._current_mode or not self._current_diff:
            self.signals.recommend_ready.emit([], True)
            return

        entries = self.recommender.recommend(
            song_id=self._song_id,
            button_mode=self._current_mode,
            difficulty=self._current_diff,
        )
        self.signals.recommend_ready.emit(entries, False)

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
        self.signals.visibility_toggle_requested.emit()

    def run(self, debug_ctrl=None):
        if not PYQT_AVAILABLE:
            print("[Overlay] PyQt6 없음, 콘솔 모드로 실행")
            import time

            while True:
                time.sleep(1)
            return

        self._app = QApplication(sys.argv)
        self._app.setQuitOnLastWindowClosed(False)
        self._window = OverlayWindow(self.signals)
        self._window.hide()
        self._window.set_user_move_callback(self._on_overlay_user_moved)

        self._settings_window = SettingsWindow()
        self._settings_window.hide()
        self._settings_window.opacity_changed.connect(self._window.update_base_opacity)
        self._settings_window.scale_changed.connect(self.signals.scale_changed)
        self.signals.settings_requested.connect(self._settings_window.show_window)

        self._roi_window = RoiOverlayWindow()
        self._roi_window.hide()
        self.signals.position_changed.connect(self._roi_window.set_game_rect)
        self.signals.roi_enabled_changed.connect(self._roi_window.set_enabled)

        self._restore_window_position()
        self._setup_debug(debug_ctrl)
        self._setup_tray_icon()
        self._app.exec()

    def _restore_window_position(self):
        if self._window is None or self._app is None:
            return

        overlay_cfg = SETTINGS.get("overlay", {})
        pos = overlay_cfg.get("position")
        if pos is None:
            return

        sx, sy = int(pos["x"]), int(pos["y"])
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
        if not QSystemTrayIcon.isSystemTrayAvailable():
            print("[Overlay] 시스템 트레이를 사용할 수 없음")
            return

        self._tray_icon = QSystemTrayIcon(self._app)
        self._tray_icon.setIcon(self._app.style().standardIcon(QStyle.StandardPixmap.SP_ComputerIcon))
        self._tray_icon.setToolTip(TRAY_TOOLTIP)

        tray_menu = QMenu()
        toggle_action = QAction(f"오버레이 표시/숨김 ({TOGGLE_HOTKEY})", self._app)
        toggle_action.triggered.connect(self._window.toggle_visibility)
        tray_menu.addAction(toggle_action)

        if self._debug_toggle_cb is not None:
            debug_action = QAction("디버그 창 표시/숨김", self._app)
            debug_action.triggered.connect(self._debug_toggle_cb)
            tray_menu.addAction(debug_action)

        settings_action = QAction("설정", self._app)
        settings_action.triggered.connect(self._settings_window.show_window)
        tray_menu.addAction(settings_action)

        tray_menu.addSeparator()
        quit_action = QAction("종료", self._app)
        quit_action.triggered.connect(self._app.quit)
        tray_menu.addAction(quit_action)

        self._tray_icon.setContextMenu(tray_menu)
        self._tray_icon.show()
        self._tray_icon.activated.connect(self._on_tray_activated)

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self._window.toggle_visibility()
