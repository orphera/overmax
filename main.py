"""
Overmax - DJMAX Respect V 비공식 난이도 오버레이
메인 진입점 — 모든 컴포넌트를 조립하고 실행

변경 사항:
  - ImageDB 초기화 추가
  - ScreenCapture에 image_db 주입
  - on_mode_diff_changed 콜백 연결
"""

import sys
import threading
import ctypes
import os
from pathlib import Path
from typing import Optional

import runtime_patch

sys.path.insert(0, str(Path(__file__).parent))

from data.varchive import VArchiveDB
from capture.window_tracker import WindowTracker
from capture.screen_capture import ScreenCapture
from overlay import OverlayController
from core.global_hotkey import GlobalHotkey
from overlay.debug_window import DebugController
from detection.image_db import ImageDB
from data.record_db import RecordDB
from settings import SETTINGS
from data.steam_session import get_most_recent_steam_id
from core.game_state import GameSessionState

from constants import (
    WINDOW_TITLE,
    TOGGLE_HOTKEY,
    CACHE_PATH as LOCAL_SONGS_JSON,
    RECORD_DB_PATH,
)
_SINGLE_INSTANCE_MUTEX_NAME = "OvermaxSingleInstanceMutex"
_ERROR_ALREADY_EXISTS = 183


def _acquire_single_instance_mutex() -> Optional[int]:
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


def _release_single_instance_mutex(handle: Optional[int]):
    if os.name != "nt":
        return
    if not handle:
        return
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CloseHandle(handle)


def main():
    mutex_handle = _acquire_single_instance_mutex()
    if mutex_handle is None:
        print("[Main] 이미 Overmax가 실행 중입니다. 기존 인스턴스를 종료한 뒤 다시 실행하세요.")
        return

    print("=" * 50)
    print("  Overmax - DJMAX Respect V 난이도 오버레이")
    print("  V-Archive 데이터 기반")
    print("=" * 50)

    try:
        # 1. VArchive DB 로드
        db = VArchiveDB()
        local = str(LOCAL_SONGS_JSON) if LOCAL_SONGS_JSON.exists() else None
        try:
            db.load(local_path=local)
        except Exception as e:
            print(f"[Main] DB 로드 실패: {e}")
            print("  songs.json을 cache/ 폴더에 넣거나 인터넷 연결을 확인하세요.")
            sys.exit(1)

        # 2. ImageDB 초기화
        image_cfg = SETTINGS["jacket_matcher"]
        image_db = ImageDB(
            db_path=str(image_cfg["db_path"]),
            similarity_threshold=float(image_cfg["similarity_threshold"]),
        )
        image_ok = image_db.initialize()
        if image_ok:
            image_db.load()
            print(f"[Main] ImageDB 준비 완료: {image_db.song_count}곡 등록됨")
        else:
            print("[Main] ImageDB 초기화 실패 - OCR 전용 모드로 실행")
            image_db = None

        # 2-1. RecordDB 초기화
        from constants import RECORD_DB_PATH
        record_db = RecordDB(db_path=RECORD_DB_PATH)
        if record_db.initialize():
            initial_steam_id = get_most_recent_steam_id()
            changed, before_sid, after_sid = record_db.set_steam_id(initial_steam_id)
            if changed:
                print(f"[Main] Steam 세션 갱신: {before_sid} -> {after_sid}")
            else:
                print(f"[Main] Steam 세션 유지: {after_sid}")
            stats = record_db.stats()
            print(
                f"[Main] RecordDB 준비 완료: {stats.get('total', 0)}개 레코드 "
                f"(steam_id={stats.get('steam_id', 'unknown')})"
            )
        else:
            print("[Main] RecordDB 초기화 실패 - 기록 수집 비활성")
            record_db = None

        # 3. 디버그 컨트롤러
        debug_ctrl = DebugController()


        # 4. 오버레이 컨트롤러
        controller = OverlayController(db, record_db)

        # 5. 창 추적기
        tracker = WindowTracker()

        def refresh_steam_session(reason: str):
            if not record_db:
                return
            steam_id = get_most_recent_steam_id()
            changed, before_sid, after_sid = record_db.set_steam_id(steam_id)
            if changed:
                debug_ctrl.log(f"[Main] Steam 세션 갱신 ({reason}): {before_sid} -> {after_sid}")
                controller.notify_record_updated()
            else:
                debug_ctrl.log(f"[Main] Steam 세션 유지 ({reason}): {after_sid}")

        def on_window_found(rect):
            refresh_steam_session("게임 창 발견")
            debug_ctrl.log(
                f"[Main] 게임 창 발견: {rect.width}x{rect.height} @ ({rect.left},{rect.top})"
            )
            controller.notify_window_pos(rect.left, rect.top, rect.width, rect.height)

        def on_window_lost():
            debug_ctrl.log("[Main] 게임 창 소실")
            controller.notify_window_lost()

        def on_window_changed(rect):
            controller.notify_window_pos(rect.left, rect.top, rect.width, rect.height)

        tracker.on_found(on_window_found)
        tracker.on_lost(on_window_lost)
        tracker.on_changed(on_window_changed)
        tracker.start()

        # 6. 화면 캡처 + OCR
        capture = ScreenCapture(tracker, image_db=image_db, record_db=record_db)

        def on_state_changed(state: GameSessionState):
            debug_ctrl.log(f"[Main] {state}")
            controller.notify_state(state)

        def on_screen_changed(is_song_select: bool):
            debug_ctrl.log(f"[Main] 화면 상태: {'선곡화면' if is_song_select else '기타화면'}")
            controller.notify_screen(is_song_select)

        capture.on_state_changed     = on_state_changed
        capture.on_screen_changed    = on_screen_changed
        capture.on_debug_log         = debug_ctrl.log
        controller._debug_log_cb     = debug_ctrl.log

        if record_db:
            capture.on_record_updated = controller.notify_record_updated
        
        hotkey = GlobalHotkey()

        # 표시/숨김 단축키
        hotkey.register(TOGGLE_HOTKEY, controller.toggle_visibility)  # 오버레이 토글
        # hotkey.register("F8", debug_ctrl.toggle_window)  # 디버그 창도 가능
        hotkey.start()

        capture_thread = threading.Thread(target=capture.start, daemon=True)
        capture_thread.start()

        print(f"\n[Main] 실행 중...")
        print(f"  {TOGGLE_HOTKEY}: 오버레이 표시/숨김")
        print(f"  Ctrl+C: 종료")
        print(f"[Main] 게임 창 대기 중: '{WINDOW_TITLE}'")

        # 7. Qt 이벤트 루프
        try:
            controller.run(debug_ctrl=debug_ctrl)
        except KeyboardInterrupt:
            print("\n[Main] 종료 중...")
        finally:
            hotkey.stop()
            capture.stop()
            tracker.stop()
            if capture_thread.is_alive():
                capture_thread.join(timeout=2)
    finally:
        _release_single_instance_mutex(mutex_handle)


if __name__ == "__main__":
    main()
