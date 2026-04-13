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

from varchive import VArchiveDB
from window_tracker import WindowTracker
from screen_capture import ScreenCapture
from overlay import OverlayController
from global_hotkey import GlobalHotkey
from debug_window import DebugController
from image_db import ImageDB
from settings import SETTINGS

LOCAL_SONGS_JSON = runtime_patch.get_data_dir() / "cache" / "songs.json"
WINDOW_TITLE = str(SETTINGS["window_tracker"]["window_title"])
TOGGLE_HOTKEY = str(SETTINGS["overlay"]["toggle_hotkey"])
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

        # 3. 디버그 컨트롤러
        debug_ctrl = DebugController()

        # 4. 오버레이 컨트롤러
        controller = OverlayController(db)

        # 5. 창 추적기
        tracker = WindowTracker()

        def on_window_found(rect):
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
        capture = ScreenCapture(tracker, image_db=image_db)

        def on_song_changed(song_id: int):
            song = db.search_by_id(song_id)
            if not song:
                debug_ctrl.log(f"[Main] 곡 정보를 찾을 수 없습니다: {song_id}")
                return

            title = song.get("name", "")
            composer = song.get("composer", "")

            debug_ctrl.log(f"[Main] 곡명 감지: '{title}' / composer: '{composer}'")
            controller.notify_song(title=title, composer=composer, song_id=song_id)

        def on_screen_changed(is_song_select: bool):
            debug_ctrl.log(f"[Main] 화면 상태: {'선곡화면' if is_song_select else '기타화면'}")
            controller.notify_screen(is_song_select)

        def on_mode_diff_changed(mode: str, diff: str):
            debug_ctrl.log(f"[Main] 버튼 모드/난이도: {mode} / {diff}")
            controller.notify_mode_diff(mode, diff)

        capture.on_song_changed      = on_song_changed
        capture.on_screen_changed    = on_screen_changed
        capture.on_debug_log         = debug_ctrl.log
        capture.on_mode_diff_changed = on_mode_diff_changed
        controller._debug_log_cb     = debug_ctrl.log
        
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
            capture.stop()
            tracker.stop()
    finally:
        _release_single_instance_mutex(mutex_handle)


if __name__ == "__main__":
    main()
