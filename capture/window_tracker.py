"""
DJMAX Respect V 창 위치/크기 추적
- GetClientRect로 타이틀바/테두리 제외한 실제 게임 영역만 추적
- 비율 기반 좌표 계산 지원
"""

import time
import threading
from dataclasses import dataclass
from typing import Optional, Callable

try:
    import win32gui
    import win32con
    import win32api
except ImportError:
    print("[WindowTracker] pywin32 없음 - 더미 모드로 실행")
    win32gui = None

from constants import WINDOW_TITLE, POLL_INTERVAL


@dataclass
class WindowRect:
    left: int
    top: int
    width: int
    height: int

    def abs(self, rx: float, ry: float) -> tuple[int, int]:
        """비율 좌표 → 절대 좌표"""
        return (
            self.left + int(self.width * rx),
            self.top + int(self.height * ry),
        )

    def abs_rect(self, rx1: float, ry1: float, rx2: float, ry2: float) -> tuple[int, int, int, int]:
        """비율 rect → 절대 rect (left, top, right, bottom)"""
        return (
            self.left + int(self.width * rx1),
            self.top + int(self.height * ry1),
            self.left + int(self.width * rx2),
            self.top + int(self.height * ry2),
        )

    def region(self, rx1: float, ry1: float, rx2: float, ry2: float) -> dict:
        """mss 캡처용 region dict 반환"""
        l, t, r, b = self.abs_rect(rx1, ry1, rx2, ry2)
        return {"left": l, "top": t, "width": r - l, "height": b - t}


class WindowTracker:
    def __init__(self):
        self._rect: Optional[WindowRect] = None
        self._hwnd: Optional[int] = None
        self._lock = threading.Lock()
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._on_found: Optional[Callable] = None
        self._on_lost: Optional[Callable] = None
        self._on_changed: Optional[Callable] = None

    @property
    def rect(self) -> Optional[WindowRect]:
        with self._lock:
            return self._rect

    @property
    def is_found(self) -> bool:
        return self._rect is not None

    def on_found(self, callback: Callable):
        """창 발견 시 콜백"""
        self._on_found = callback

    def on_lost(self, callback: Callable):
        """창 소실 시 콜백"""
        self._on_lost = callback

    def on_changed(self, callback: Callable):
        """창 위치/크기 변경 시 콜백"""
        self._on_changed = callback

    def start(self):
        self._running = True
        self._thread = threading.Thread(target=self._poll_loop, daemon=True)
        self._thread.start()
        print("[WindowTracker] 시작됨")

    def stop(self):
        self._running = False
        if self._thread:
            self._thread.join(timeout=2)

    def _poll_loop(self):
        was_found = False
        prev_rect_tuple: Optional[tuple[int, int, int, int]] = None
        while self._running:
            rect = self._get_game_rect()
            with self._lock:
                self._rect = rect

            is_found = rect is not None
            if is_found and not was_found:
                print(f"[WindowTracker] 게임 창 발견: {rect.width}x{rect.height} @ ({rect.left},{rect.top})")
                if self._on_found:
                    self._on_found(rect)
                prev_rect_tuple = (rect.left, rect.top, rect.width, rect.height)
            elif not is_found and was_found:
                print("[WindowTracker] 게임 창 소실")
                if self._on_lost:
                    self._on_lost()
                prev_rect_tuple = None
            elif is_found and was_found:
                current_tuple = (rect.left, rect.top, rect.width, rect.height)
                if current_tuple != prev_rect_tuple:
                    if self._on_changed:
                        self._on_changed(rect)
                    prev_rect_tuple = current_tuple
            was_found = is_found

            time.sleep(POLL_INTERVAL)

    def _get_game_rect(self) -> Optional[WindowRect]:
        if win32gui is None:
            # 더미: 테스트용
            return WindowRect(0, 0, 1920, 1080)

        try:
            hwnd = win32gui.FindWindow(None, WINDOW_TITLE)
            if not hwnd:
                return None

            # 클라이언트 영역 (타이틀바/테두리 제외)
            client_rect = win32gui.GetClientRect(hwnd)
            left, top = win32gui.ClientToScreen(hwnd, (0, 0))
            width = client_rect[2]
            height = client_rect[3]

            if width <= 0 or height <= 0:
                return None

            return WindowRect(left=left, top=top, width=width, height=height)

        except Exception as e:
            print(f"[WindowTracker] 오류: {e}")
            return None

    def is_foreground(self) -> bool:
        """게임 창이 현재 포커스(맨 앞)인지 확인"""
        if win32gui is None:
            return True
        try:
            foreground = win32gui.GetForegroundWindow()
            hwnd = win32gui.FindWindow(None, WINDOW_TITLE)
            return foreground == hwnd
        except Exception:
            return False


if __name__ == "__main__":
    tracker = WindowTracker()
    tracker.start()

    try:
        while True:
            rect = tracker.rect
            if rect:
                print(f"창: {rect.width}x{rect.height} @ ({rect.left},{rect.top}), 포커스: {tracker.is_foreground()}")
            else:
                print("게임 창 없음")
            time.sleep(1)
    except KeyboardInterrupt:
        tracker.stop()
