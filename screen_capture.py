"""
화면 캡처 + 선곡화면 감지 + 곡명 OCR

감지 전략:
1. 하단 힌트바 앵커 픽셀로 선곡화면 여부 판별
2. 오른쪽 리스트에서 주황-보라 하이라이트 행 Y좌표 탐색
3. 해당 행의 곡명 텍스트만 OCR
"""

import time
import threading
import numpy as np
from typing import Optional, Callable
from window_tracker import WindowTracker, WindowRect

try:
    import mss
    import cv2
    MSS_AVAILABLE = True
except ImportError:
    print("[ScreenCapture] mss/cv2 없음 - 더미 모드")
    MSS_AVAILABLE = False

try:
    import easyocr
    reader = easyocr.Reader(["ko", "en"], gpu=False, verbose=False)
    OCR_AVAILABLE = True
    print("[ScreenCapture] EasyOCR 초기화 완료")
except ImportError:
    print("[ScreenCapture] easyocr 없음 - OCR 비활성화")
    OCR_AVAILABLE = False


# ------------------------------------------------------------------
# 비율 상수 (1920x1080 기준으로 측정, 비율로 저장)
# ------------------------------------------------------------------

# 선곡화면 앵커: 하단 힌트바 "Esc 메뉴" 근처 흰색 텍스트 영역
ANCHOR_REGION = (0.88, 0.972, 0.99, 0.995)

# 앵커 판별: 해당 영역의 평균 밝기가 임계값 이상이면 선곡화면
ANCHOR_BRIGHTNESS_THRESHOLD = 150

# 곡 리스트 영역 (하이라이트 행 탐색 범위)
LIST_REGION = (0.188, 0.08, 0.595, 0.955)

# 하이라이트 색상 범위 (HSV)
# 주황(~#E8873A): H=20~35, S=150~255, V=180~255
# 보라(~#9B59B6): H=260~290 → OpenCV HSV에서 H는 절반이라 130~145
HIGHLIGHT_HUE_RANGES = [
    (15, 35),    # 주황 계열
    (130, 150),  # 보라 계열
]
HIGHLIGHT_SAT_MIN = 100
HIGHLIGHT_VAL_MIN = 150

# 하이라이트 행에서 곡명 텍스트 X 범위
TITLE_X_RATIO = (0.215, 0.475)

# OCR 폴링 주기
OCR_INTERVAL = 0.4  # 초


class ScreenCapture:
    def __init__(self, tracker: WindowTracker):
        self.tracker = tracker
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_title = ""
        self._is_song_select = False

        # 콜백
        self.on_song_changed: Optional[Callable[[str], None]] = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None  # True=선곡화면 진입

    def start(self):
        self._running = True
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()
        print("[ScreenCapture] 시작됨")

    def stop(self):
        self._running = False

    # ------------------------------------------------------------------
    # 메인 루프
    # ------------------------------------------------------------------

    def _loop(self):
        if not MSS_AVAILABLE:
            print("[ScreenCapture] mss 없음, 더미 루프 실행")
            while self._running:
                time.sleep(OCR_INTERVAL)
            return

        with mss.mss() as sct:
            while self._running:
                rect = self.tracker.rect
                if rect is None:
                    time.sleep(0.5)
                    continue

                # 게임이 포커스가 아니면 처리 안 함
                if not self.tracker.is_foreground():
                    time.sleep(0.5)
                    continue

                try:
                    self._process_frame(sct, rect)
                except Exception as e:
                    print(f"[ScreenCapture] 프레임 처리 오류: {e}")

                time.sleep(OCR_INTERVAL)

    def _process_frame(self, sct, rect: WindowRect):
        # 1. 선곡화면 앵커 체크 (가벼운 연산 먼저)
        is_song_select = self._check_anchor(sct, rect)

        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            print(f"[ScreenCapture] 화면 전환: {'선곡화면' if is_song_select else '기타'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if not is_song_select:
            return

        # 2. 하이라이트 행 탐색 + 곡명 OCR
        title = self._detect_selected_song(sct, rect)
        if title and title != self._last_title:
            self._last_title = title
            print(f"[ScreenCapture] 곡 변경 감지: {title}")
            if self.on_song_changed:
                self.on_song_changed(title)

    # ------------------------------------------------------------------
    # 선곡화면 앵커 체크
    # ------------------------------------------------------------------

    def _check_anchor(self, sct, rect: WindowRect) -> bool:
        region = rect.region(*ANCHOR_REGION)
        img = np.array(sct.grab(region))
        gray = cv2.cvtColor(img, cv2.COLOR_BGRA2GRAY)
        brightness = gray.mean()
        return brightness > ANCHOR_BRIGHTNESS_THRESHOLD

    # ------------------------------------------------------------------
    # 하이라이트 행 탐색
    # ------------------------------------------------------------------

    def _detect_selected_song(self, sct, rect: WindowRect) -> Optional[str]:
        # 리스트 전체 영역 캡처
        region = rect.region(*LIST_REGION)
        img = np.array(sct.grab(region))
        bgr = cv2.cvtColor(img, cv2.COLOR_BGRA2BGR)
        hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)

        # 하이라이트 색상 마스크 생성
        mask = np.zeros(hsv.shape[:2], dtype=np.uint8)
        for h_min, h_max in HIGHLIGHT_HUE_RANGES:
            m = cv2.inRange(
                hsv,
                (h_min, HIGHLIGHT_SAT_MIN, HIGHLIGHT_VAL_MIN),
                (h_max, 255, 255),
            )
            mask = cv2.bitwise_or(mask, m)

        # 행별로 하이라이트 픽셀 수 집계
        row_sums = mask.sum(axis=1)
        img_w = img.shape[1]
        threshold = img_w * 0.3  # 행 너비의 30% 이상이 하이라이트색이면 해당 행

        highlight_rows = np.where(row_sums > threshold)[0]
        if len(highlight_rows) == 0:
            return None

        # 하이라이트 행의 중앙 Y
        row_mid = int(highlight_rows.mean())
        row_h = max(len(highlight_rows), 30)

        # 곡명 텍스트 영역 크롭
        list_region = rect.region(*LIST_REGION)
        title_x1 = list_region["left"] + int(list_region["width"] * (TITLE_X_RATIO[0] - LIST_REGION[0]) / (LIST_REGION[2] - LIST_REGION[0]))
        title_x2 = list_region["left"] + int(list_region["width"] * (TITLE_X_RATIO[1] - LIST_REGION[0]) / (LIST_REGION[2] - LIST_REGION[0]))

        y1 = max(0, row_mid - row_h // 2)
        y2 = min(img.shape[0], row_mid + row_h // 2)
        title_crop = bgr[y1:y2, :int(img_w * 0.75)]

        return self._ocr_title(title_crop)

    # ------------------------------------------------------------------
    # OCR
    # ------------------------------------------------------------------

    def _ocr_title(self, img_bgr: np.ndarray) -> Optional[str]:
        if not OCR_AVAILABLE:
            return None

        # 대비 향상
        gray = cv2.cvtColor(img_bgr, cv2.COLOR_BGR2GRAY)
        _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)

        results = reader.readtext(thresh, detail=0, paragraph=True)
        if not results:
            return None

        # 가장 긴 텍스트를 곡명으로 선택
        title = max(results, key=len).strip()
        return title if title else None


if __name__ == "__main__":
    from window_tracker import WindowTracker

    tracker = WindowTracker()
    tracker.start()

    capture = ScreenCapture(tracker)
    capture.on_song_changed = lambda t: print(f">>> 곡: {t}")
    capture.on_screen_changed = lambda s: print(f">>> 선곡화면: {s}")
    capture.start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        capture.stop()
        tracker.stop()
