"""
Overmax - DJMAX Respect V 비공식 난이도 오버레이
메인 진입점 — 모든 컴포넌트를 조립하고 실행

변경 사항:
  - ImageDB 초기화 추가
  - ScreenCapture에 image_db 주입
  - F10 단축키: 현재 재킷을 현재 OCR 곡명으로 등록
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
from debug_window import DebugController
from image_db import ImageDB
from settings import SETTINGS

LOCAL_SONGS_JSON = runtime_patch.get_data_dir() / "cache" / "songs.json"
WINDOW_TITLE = str(SETTINGS["window_tracker"]["window_title"])
TOGGLE_HOTKEY = str(SETTINGS["overlay"]["toggle_hotkey"])
JACKET_REGISTER_HOTKEY = str(SETTINGS["jacket_matcher"]["register_hotkey"])
_SINGLE_INSTANCE_MUTEX_NAME = "OvermaxSingleInstanceMutex"
_ERROR_ALREADY_EXISTS = 183


def _acquire_single_instance_mutex() -> Optional[int]:
    """
    Windows named mutex 기반 단일 실행 보장.
    이미 실행 중이면 None 반환.
    """
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

        # 2. ImageDB 초기화 (실패해도 계속 실행 - OCR fallback 동작)
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

        # 현재 인식된 곡명 추적 (재킷 등록 시 song_id로 사용)
        _current_song_id: dict[str, str] = {"value": ""}

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

        def on_song_changed(title: str, composer: str, song_id: Optional[int] = None):
            if song_id:
                song = db.search_by_id(song_id)
                if song:
                    title = song.get("name", title)
                    composer = song.get("composer", composer)

            _current_song_id["value"] = title
            debug_ctrl.log(f"[Main] 곡명 감지: '{title}' / composer: '{composer}'")
            controller.notify_song(title=title, composer=composer, song_id=song_id)

        def on_screen_changed(is_song_select: bool):
            debug_ctrl.log(f"[Main] 화면 상태: {'선곡화면' if is_song_select else '기타화면'}")
            controller.notify_screen(is_song_select)

        capture.on_song_changed   = on_song_changed
        capture.on_screen_changed = on_screen_changed
        capture.on_debug_log      = debug_ctrl.log
        controller._debug_log_cb  = debug_ctrl.log

        # 7. 재킷 수동 등록 단축키 (F10)
        #    Qt 이벤트 루프 밖이므로 keyboard 라이브러리 또는
        #    overlay.py의 QShortcut으로 처리.
        #    여기서는 콜백 형태로 overlay에 등록
        def on_jacket_register_hotkey():
            song_id = _current_song_id["value"]
            if not song_id:
                debug_ctrl.log("[Main] 재킷 등록 실패: 현재 곡명 없음")
                return
            debug_ctrl.log(f"[Main] 재킷 등록 시작: '{song_id}'")
            capture.trigger_jacket_register(song_id)

        # controller에 재킷 등록 콜백 주입 (overlay.py에서 QShortcut 처리)
        controller._jacket_register_cb = on_jacket_register_hotkey

        capture_thread = threading.Thread(target=capture.start, daemon=True)
        capture_thread.start()

        print(f"\n[Main] 실행 중...")
        print(f"  {TOGGLE_HOTKEY}: 오버레이 표시/숨김")
        print(f"  {JACKET_REGISTER_HOTKEY}: 현재 재킷 DB 등록")
        print(f"  Ctrl+C: 종료")
        print(f"[Main] 게임 창 대기 중: '{WINDOW_TITLE}'")

        # 8. Qt 이벤트 루프
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
