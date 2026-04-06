"""
Overmax - DJMAX Respect V 비공식 난이도 오버레이
메인 진입점 — 모든 컴포넌트를 조립하고 실행
"""

import sys
import threading
from pathlib import Path

# PyInstaller 패키징 환경 대응 패치 (반드시 다른 import보다 먼저)
import runtime_patch

sys.path.insert(0, str(Path(__file__).parent))

from varchive import VArchiveDB
from window_tracker import WindowTracker
from screen_capture import ScreenCapture
from overlay import OverlayController
from debug_window import DebugController
from settings import SETTINGS

LOCAL_SONGS_JSON = runtime_patch.get_data_dir() / "cache" / "songs.json"
WINDOW_TITLE = str(SETTINGS.get("window_tracker", {}).get("window_title", "DJMAX RESPECT V"))
TOGGLE_HOTKEY = str(SETTINGS.get("overlay", {}).get("toggle_hotkey", "F9"))


def main():
    print("=" * 50)
    print("  Overmax - DJMAX Respect V 난이도 오버레이")
    print("  V-Archive 데이터 기반")
    print("=" * 50)

    # 1. DB 로드
    db = VArchiveDB()
    local = str(LOCAL_SONGS_JSON) if LOCAL_SONGS_JSON.exists() else None
    try:
        db.load(local_path=local)
    except Exception as e:
        print(f"[Main] DB 로드 실패: {e}")
        print("  songs.json을 cache/ 폴더에 넣거나 인터넷 연결을 확인하세요.")
        sys.exit(1)

    # 2. 디버그 컨트롤러 생성 (Qt App 전에 signals 준비)
    debug_ctrl = DebugController()

    # 3. 오버레이 컨트롤러 생성
    controller = OverlayController(db)

    # 4. 창 추적기 시작
    tracker = WindowTracker()

    def on_window_found(rect):
        debug_ctrl.log(
            f"[Main] 게임 창 발견: {rect.width}x{rect.height} @ ({rect.left},{rect.top})"
        )
        controller.notify_window_pos(rect.left, rect.top, rect.width, rect.height)

    def on_window_lost():
        debug_ctrl.log("[Main] 게임 창 소실")

    tracker.on_found(on_window_found)
    tracker.on_lost(on_window_lost)
    tracker.start()

    # 5. 화면 캡처 + OCR
    capture = ScreenCapture(tracker)

    def on_song_changed(title: str):
        debug_ctrl.log(f"[Main] 곡명 감지: '{title}'")
        controller.notify_song(title)

    def on_screen_changed(is_song_select: bool):
        debug_ctrl.log(f"[Main] 화면 상태: {'선곡화면' if is_song_select else '기타화면'}")
        controller.notify_screen(is_song_select)

    capture.on_song_changed  = on_song_changed
    capture.on_screen_changed = on_screen_changed
    # ScreenCapture 자체 로그도 debug 창으로
    capture.on_debug_log = debug_ctrl.log

    # OverlayController 로그도 연결
    controller._debug_log_cb = debug_ctrl.log

    capture_thread = threading.Thread(target=capture.start, daemon=True)
    capture_thread.start()

    print(f"\n[Main] 실행 중... ({TOGGLE_HOTKEY}: 오버레이 표시/숨김, Ctrl+C: 종료)")
    print(f"[Main] 게임 창 대기 중: '{WINDOW_TITLE}'")

    # 6. Qt 이벤트 루프 (메인 스레드)
    #    OverlayController.run() 안에서 QApplication 생성 후 디버그 창도 띄움
    try:
        controller.run(debug_ctrl=debug_ctrl)
    except KeyboardInterrupt:
        print("\n[Main] 종료 중...")
    finally:
        capture.stop()
        tracker.stop()


if __name__ == "__main__":
    main()
