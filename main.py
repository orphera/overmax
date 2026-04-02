"""
Overmax - DJMAX Respect V 비공식 난이도 오버레이
메인 진입점 - 모든 컴포넌트를 조립하고 실행
"""

import sys
import os
import threading
from pathlib import Path

# PyInstaller 패키징 환경 대응 패치 (반드시 다른 import보다 먼저)
import runtime_patch

# 프로젝트 루트를 path에 추가
sys.path.insert(0, str(Path(__file__).parent))

from varchive import VArchiveDB
from window_tracker import WindowTracker
from screen_capture import ScreenCapture
from overlay import OverlayController

# songs.json 위치
# - 개발: 프로젝트 폴더/cache/songs.json
# - 패키징: exe 옆 cache/songs.json
LOCAL_SONGS_JSON = runtime_patch.get_data_dir() / "cache" / "songs.json"


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

    # 2. 오버레이 컨트롤러 생성
    controller = OverlayController(db)

    # 3. 창 추적기 시작
    tracker = WindowTracker()
    tracker.on_found(lambda rect: controller.notify_window_pos(
        rect.left, rect.top, rect.width, rect.height
    ))
    tracker.start()

    # 4. 화면 캡처 + 감지 시작 (별도 스레드)
    capture = ScreenCapture(tracker)

    def on_song_changed(title: str):
        controller.notify_song(title)

    def on_screen_changed(is_song_select: bool):
        controller.notify_screen(is_song_select)

    capture.on_song_changed = on_song_changed
    capture.on_screen_changed = on_screen_changed

    capture_thread = threading.Thread(target=capture.start, daemon=True)
    capture_thread.start()

    print("\n[Main] 실행 중... (F9: 오버레이 표시/숨김, Ctrl+C: 종료)")
    print(f"[Main] 게임 창 대기 중: 'DJMAX RESPECT V'")

    # 5. Qt 이벤트 루프 (메인 스레드 점유)
    try:
        controller.run()
    except KeyboardInterrupt:
        print("\n[Main] 종료 중...")
    finally:
        capture.stop()
        tracker.stop()


if __name__ == "__main__":
    main()
