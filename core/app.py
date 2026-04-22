"""
Application lifecycle manager for Overmax.
"""

import sys
import threading
import ctypes
import os
import signal
from pathlib import Path
from typing import Optional

from data.varchive import VArchiveDB
from capture.window_tracker import WindowTracker
from capture.screen_capture import ScreenCapture
from overlay.controller import OverlayController
from core.global_hotkey import GlobalHotkey
from overlay.debug_window import DebugController
from detection.image_db import ImageDB
from data.record_db import RecordDB
from settings import SETTINGS
from data.steam_session import get_most_recent_steam_id
from core.game_state import GameSessionState
from data.image_db_updater import check_and_update
from core.utils import show_error_message, check_environment

from constants import (
    WINDOW_TITLE,
    TOGGLE_HOTKEY,
    CACHE_PATH,
    RECORD_DB_PATH,
)

_SINGLE_INSTANCE_MUTEX_NAME = "OvermaxSingleInstanceMutex"
_ERROR_ALREADY_EXISTS = 183


class OvermaxApp:
    def __init__(self):
        check_environment()
        self._mutex_handle = self._acquire_mutex()
        if self._mutex_handle is None:
            msg = "이미 Overmax가 실행 중입니다. 기존 인스턴스를 종료한 뒤 다시 실행하세요."
            print(f"[Main] {msg}")
            show_error_message(msg)
            sys.exit(0)

        self.varchive_db: Optional[VArchiveDB] = None
        self.image_db: Optional[ImageDB] = None
        self.record_db: Optional[RecordDB] = None
        self.debug_ctrl: Optional[DebugController] = None
        self.overlay_ctrl: Optional[OverlayController] = None
        self.tracker: Optional[WindowTracker] = None
        self.capture: Optional[ScreenCapture] = None
        self.hotkey: Optional[GlobalHotkey] = None
        self._capture_thread: Optional[threading.Thread] = None

    def run(self):
        print("=" * 50)
        print("  Overmax - DJMAX Respect V 난이도 오버레이")
        print("  V-Archive 데이터 기반")
        print("=" * 50)

        try:
            self._init_databases()
            self._init_components()
            self._bind_events()
            self._start_workers()
            self._run_event_loop()
        finally:
            self._cleanup()

    def _init_databases(self):
        self.varchive_db = VArchiveDB()
        local = str(CACHE_PATH) if CACHE_PATH.exists() else None
        try:
            self.varchive_db.load(local_path=local)
        except Exception as e:
            msg = f"DB 로드 실패: {e}\nsongs.json을 cache/ 폴더에 넣거나 인터넷 연결을 확인하세요."
            print(f"[Main] {msg}")
            show_error_message(msg)
            sys.exit(1)

        image_cfg = SETTINGS["jacket_matcher"]
        db_path = Path(str(image_cfg["db_path"]))

        check_and_update(owner="orphera", repo="overmax-image-db", db_path=db_path, log=print)

        self.image_db = ImageDB(
            db_path=str(db_path),
            similarity_threshold=float(image_cfg["similarity_threshold"]),
        )
        if self.image_db.initialize():
            self.image_db.load()
            print(f"[Main] ImageDB 준비 완료: {self.image_db.song_count}곡 등록됨")
        else:
            print("[Main] ImageDB 초기화 실패 - OCR 전용 모드로 실행")
            self.image_db = None

        self.record_db = RecordDB(db_path=RECORD_DB_PATH)
        if self.record_db.initialize():
            changed, before_sid, after_sid = self.record_db.set_steam_id(get_most_recent_steam_id())
            if changed:
                print(f"[Main] Steam 세션 갱신: {before_sid} -> {after_sid}")
            stats = self.record_db.stats()
            print(f"[Main] RecordDB 준비 완료: {stats.get('total', 0)}개 레코드 (steam_id={stats.get('steam_id', 'unknown')})")
        else:
            print("[Main] RecordDB 초기화 실패 - 기록 수집 비활성")
            self.record_db = None

    def _init_components(self):
        self.debug_ctrl = DebugController()
        self.overlay_ctrl = OverlayController(self.varchive_db, self.record_db)
        self.tracker = WindowTracker()
        self.capture = ScreenCapture(self.tracker, image_db=self.image_db, record_db=self.record_db)
        self.hotkey = GlobalHotkey()

    def _bind_events(self):
        self.tracker.on_found(self._on_window_found)
        self.tracker.on_lost(self._on_window_lost)
        self.tracker.on_changed(self._on_window_changed)

        self.capture.on_state_changed = self._on_state_changed
        self.capture.on_screen_changed = self._on_screen_changed
        self.capture.on_confidence_changed = self.overlay_ctrl.notify_confidence
        self.capture.on_debug_log = self.debug_ctrl.log
        self.overlay_ctrl._debug_log_cb = self.debug_ctrl.log

        if self.record_db:
            self.capture.on_record_updated = self.overlay_ctrl.notify_record_updated

        self.hotkey.register(TOGGLE_HOTKEY, self.overlay_ctrl.toggle_visibility)

    def _start_workers(self):
        self.hotkey.start()
        self._capture_thread = threading.Thread(target=self.capture.start, daemon=True)
        self._capture_thread.start()
        self.tracker.start()

        print(f"\n[Main] 실행 중...")
        print(f"  {TOGGLE_HOTKEY}: 오버레이 표시/숨김")
        print(f"  Ctrl+C: 종료")
        print(f"[Main] 게임 창 대기 중: '{WINDOW_TITLE}'")

    def _run_event_loop(self):
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        self.overlay_ctrl.run(debug_ctrl=self.debug_ctrl)

    def _cleanup(self):
        if self.hotkey:
            self.hotkey.stop()
        if self.capture:
            self.capture.stop()
        if self.tracker:
            self.tracker.stop()
        if self._capture_thread and self._capture_thread.is_alive():
            self._capture_thread.join(timeout=2)
        self._release_mutex(self._mutex_handle)

    # --- Callbacks ---

    def _refresh_steam_session(self, reason: str):
        if not self.record_db:
            return
        changed, before_sid, after_sid = self.record_db.set_steam_id(get_most_recent_steam_id())
        if changed:
            self.debug_ctrl.log(f"[Main] Steam 세션 갱신 ({reason}): {before_sid} -> {after_sid}")
            self.overlay_ctrl.notify_record_updated()
        elif after_sid:
            self.debug_ctrl.log(f"[Main] Steam 세션 유지 ({reason}): {after_sid}")

    def _on_window_found(self, rect):
        self._refresh_steam_session("게임 창 발견")
        self.debug_ctrl.log(f"[Main] 게임 창 발견: {rect.width}x{rect.height} @ ({rect.left},{rect.top})")
        self.overlay_ctrl.notify_window_pos(rect.left, rect.top, rect.width, rect.height)

    def _on_window_lost(self):
        self.debug_ctrl.log("[Main] 게임 창 소실")
        self.overlay_ctrl.notify_window_lost()

    def _on_window_changed(self, rect):
        self.overlay_ctrl.notify_window_pos(rect.left, rect.top, rect.width, rect.height)

    def _on_state_changed(self, state: GameSessionState):
        self.debug_ctrl.log(f"[Main] {state}")
        self.overlay_ctrl.notify_state(state)

    def _on_screen_changed(self, is_song_select: bool):
        self.debug_ctrl.log(f"[Main] 화면 상태: {'선곡화면' if is_song_select else '기타화면'}")
        self.overlay_ctrl.notify_screen(is_song_select)

    # --- Mutex Mutility ---

    def _acquire_mutex(self) -> Optional[int]:
        if os.name != "nt":
            return 1
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        handle = kernel32.CreateMutexW(None, False, _SINGLE_INSTANCE_MUTEX_NAME)
        if not handle:
            return None
        if ctypes.get_last_error() == _ERROR_ALREADY_EXISTS:
            kernel32.CloseHandle(handle)
            return None
        return int(handle)

    def _release_mutex(self, handle: Optional[int]):
        if os.name != "nt" or not handle:
            return
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CloseHandle(handle)
