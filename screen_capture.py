"""
screen_capture.py - 화면 캡처 및 OCR 모듈 (Windows OCR 버전)

주요 수정 사항 (2026-04-06):
  - SAMPLING_X_RATIO: 0.20 -> 0.08 (실제 곡 리스트 x=0.03~0.17)
  - TITLE_X_START/END: 0.22/0.50 -> 0.031/0.167 (실제 텍스트 범위)
  - 선곡화면 감지: 앵커포인트(색상) -> 주황 클러스터 방식 (실제 동작)
  - _ocr_windows: 클래스 바깥에 잘못 정의된 것 수정 (메서드로)
  - asyncio: 스레드당 이벤트 루프 1개만 생성 (매 호출마다 run() 금지)
  - 디버그 로그 콜백 추가
"""

import time
import threading
import asyncio
import numpy as np
from typing import Optional, Callable
import mss
import cv2
from settings import SETTINGS

try:
    import winrt.windows.media.ocr as ocr
    import winrt.windows.graphics.imaging as imaging
    import winrt.windows.storage.streams as streams
    WINDOWS_OCR_AVAILABLE = True
except ImportError:
    WINDOWS_OCR_AVAILABLE = False

from window_tracker import WindowTracker, WindowRect

# ------------------------------------------------------------------
# 설정 상수 (비율 기반) — 1920x1080 실측값
# ------------------------------------------------------------------
SCREEN_CAPTURE_SETTINGS = SETTINGS.get("screen_capture", {})

# 곡 리스트 영역 x 범위 (왼쪽 패널)
LIST_X_START = float(SCREEN_CAPTURE_SETTINGS.get("list_x_start", 0.031))   # x=60/1920
LIST_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("list_x_end", 0.167))   # x=320/1920

# 수직 샘플링 X (리스트 중앙)
SAMPLING_X_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("sampling_x_ratio", 0.08))

# OCR 할 제목 영역 (하이라이트 행의 x 범위) — 리스트와 동일
TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("title_x_start", 0.031))
TITLE_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("title_x_end", 0.167))

# 하이라이트 행 감지 (HSV 주황)
HIGHLIGHT_HUE_MIN  = int(SCREEN_CAPTURE_SETTINGS.get("highlight_hue_min", 10))
HIGHLIGHT_HUE_MAX  = int(SCREEN_CAPTURE_SETTINGS.get("highlight_hue_max", 32))
HIGHLIGHT_SAT_MIN  = int(SCREEN_CAPTURE_SETTINGS.get("highlight_sat_min", 130))
HIGHLIGHT_VAL_MIN  = int(SCREEN_CAPTURE_SETTINGS.get("highlight_val_min", 160))

# 하이라이트 행으로 인정하는 연속 픽셀 높이 (px)
HIGHLIGHT_ROW_MIN_PX = int(SCREEN_CAPTURE_SETTINGS.get("highlight_row_min_px", 40))
HIGHLIGHT_ROW_MAX_PX = int(SCREEN_CAPTURE_SETTINGS.get("highlight_row_max_px", 200))

# 각 행에서 주황 픽셀 최소 개수
HIGHLIGHT_ROW_THRESHOLD = int(SCREEN_CAPTURE_SETTINGS.get("highlight_row_threshold", 8))

OCR_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("ocr_interval_sec", 0.35))   # 초
IDLE_SLEEP_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("idle_sleep_sec", 0.5))


class ScreenCapture:
    def __init__(self, tracker: WindowTracker):
        self.tracker = tracker
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_title = ""
        self._is_song_select = False

        # 콜백
        self.on_song_changed:   Optional[Callable[[str], None]]  = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None
        self.on_debug_log:      Optional[Callable[[str], None]]  = None

        # 스레드 내 asyncio 이벤트 루프 (한 번만 생성)
        self._loop: Optional[asyncio.AbstractEventLoop] = None

        # Windows OCR 엔진
        self.ocr_engine = None
        if WINDOWS_OCR_AVAILABLE:
            try:
                supported_langs = ocr.OcrEngine.available_recognizer_languages
                target_lang = next(
                    (l for l in supported_langs if "ko" in l.language_tag.lower()), None
                )
                if not target_lang and len(supported_langs) > 0:
                    target_lang = supported_langs[0]
                if target_lang:
                    self.ocr_engine = ocr.OcrEngine.try_create_from_language(target_lang)
                    self.log(f"OCR 엔진 언어: {target_lang.language_tag}")
                else:
                    self.ocr_engine = ocr.OcrEngine.try_create_from_user_profile_languages()
                    self.log("OCR 엔진: user profile 언어 사용")
            except Exception as e:
                self.log(f"OCR 엔진 초기화 실패: {e}")

    # ------------------------------------------------------------------
    # 로그 헬퍼
    # ------------------------------------------------------------------

    def log(self, msg: str):
        full = f"[ScreenCapture] {msg}"
        print(full)
        if self.on_debug_log:
            self.on_debug_log(full)

    # ------------------------------------------------------------------
    # 시작 / 종료
    # ------------------------------------------------------------------

    def start(self):
        self._running = True
        self._thread = threading.Thread(target=self._loop_entry, daemon=True)
        self._thread.start()
        self.log(f"시작됨 (Windows OCR: {WINDOWS_OCR_AVAILABLE})")

    def stop(self):
        self._running = False

    def _loop_entry(self):
        """백그라운드 스레드 진입점 — asyncio 루프를 여기서 한 번만 생성"""
        self._loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self._loop)
        try:
            self._loop.run_until_complete(self._async_loop())
        finally:
            self._loop.close()

    # ------------------------------------------------------------------
    # 메인 루프
    # ------------------------------------------------------------------

    async def _async_loop(self):
        with mss.mss() as sct:
            while self._running:
                rect = self.tracker.rect
                if rect is None or not self.tracker.is_foreground():
                    await asyncio.sleep(IDLE_SLEEP_INTERVAL)
                    continue
                try:
                    await self._process_frame(sct, rect)
                except Exception as e:
                    self.log(f"프레임 처리 오류: {e}")
                await asyncio.sleep(OCR_INTERVAL)

    # ------------------------------------------------------------------
    # 프레임 처리
    # ------------------------------------------------------------------

    async def _process_frame(self, sct, rect: WindowRect):
        # 1. 선곡화면 감지
        is_song_select, y_range = self._detect_song_select(sct, rect)

        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            self.log(f"화면 변경: {'선곡화면' if is_song_select else '기타화면'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if not is_song_select or y_range is None:
            return

        y_start, y_end = y_range
        self.log(
            f"하이라이트 행: y={y_start}~{y_end} "
            f"(비율 {y_start/rect.height:.3f}~{y_end/rect.height:.3f})"
        )

        # 2. 제목 영역 캡처
        title_region = {
            "top":    rect.top  + y_start,
            "left":   rect.left + int(rect.width * TITLE_X_START),
            "width":  int(rect.width * (TITLE_X_END - TITLE_X_START)),
            "height": max(1, y_end - y_start),
        }
        img_bgra = np.array(sct.grab(title_region))

        # 3. OCR
        title = await self._ocr_windows(img_bgra)
        self.log(f"OCR 원문: '{title}'")

        if title and title != self._last_title:
            self._last_title = title
            if self.on_song_changed:
                self.on_song_changed(title)

    # ------------------------------------------------------------------
    # 선곡화면 감지 (주황 클러스터 방식)
    # ------------------------------------------------------------------

    def _detect_song_select(self, sct, rect: WindowRect):
        """
        곡 리스트 열(LIST_X_START~LIST_X_END)을 수직으로 스캔.
        주황색 픽셀(선택 행 배경)이 연속으로 40~200px 나타나면 선곡화면.

        Returns:
            (is_song_select, y_range)  — y_range는 rect 기준 절대 y
        """
        scan_region = {
            "top":    rect.top,
            "left":   rect.left + int(rect.width * LIST_X_START),
            "width":  max(1, int(rect.width * (LIST_X_END - LIST_X_START))),
            "height": rect.height,
        }
        col_img = np.array(sct.grab(scan_region))   # (H, W, 4) BGRA

        bgr = cv2.cvtColor(col_img, cv2.COLOR_BGRA2BGR)
        hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)

        orange_mask = cv2.inRange(
            hsv,
            (HIGHLIGHT_HUE_MIN, HIGHLIGHT_SAT_MIN, HIGHLIGHT_VAL_MIN),
            (HIGHLIGHT_HUE_MAX, 255, 255),
        )

        row_counts = np.sum(orange_mask > 0, axis=1)
        highlight_rows = np.where(row_counts > HIGHLIGHT_ROW_THRESHOLD)[0]

        if len(highlight_rows) < HIGHLIGHT_ROW_MIN_PX:
            return False, None

        # 연속 클러스터 탐색
        clusters = []
        start = int(highlight_rows[0])
        prev  = int(highlight_rows[0])
        for r in highlight_rows[1:]:
            r = int(r)
            if r - prev > 5:
                clusters.append((start, prev))
                start = r
            prev = r
        clusters.append((start, prev))

        valid = [
            (s, e) for s, e in clusters
            if HIGHLIGHT_ROW_MIN_PX <= (e - s) <= HIGHLIGHT_ROW_MAX_PX
        ]
        if not valid:
            return False, None

        best_s, best_e = max(valid, key=lambda x: x[1] - x[0])
        return True, (best_s, best_e)

    # ------------------------------------------------------------------
    # Windows OCR (async 메서드)
    # ------------------------------------------------------------------

    async def _ocr_windows(self, img_bgra: np.ndarray) -> str:
        """Windows.Media.Ocr 엔진을 사용한 고속 인식"""
        if not WINDOWS_OCR_AVAILABLE or self.ocr_engine is None:
            return ""

        try:
            h, w = img_bgra.shape[:2]
            if w == 0 or h == 0:
                return ""

            # 업스케일 (3x) — 작은 텍스트 인식률 향상
            scale = 3
            upscaled = cv2.resize(
                img_bgra, (w * scale, h * scale),
                interpolation=cv2.INTER_CUBIC,
            )
            gray = cv2.cvtColor(upscaled, cv2.COLOR_BGRA2GRAY)

            # OTSU 이진화
            bg_mean = float(gray.mean())
            if bg_mean < 128:
                _, thresh = cv2.threshold(
                    gray, 0, 255, cv2.THRESH_BINARY | cv2.THRESH_OTSU
                )
            else:
                _, thresh = cv2.threshold(
                    gray, 0, 255, cv2.THRESH_BINARY_INV | cv2.THRESH_OTSU
                )

            # BMP -> Windows InMemoryRandomAccessStream
            success, encoded = cv2.imencode(".bmp", thresh)
            if not success:
                return ""

            data_writer = streams.DataWriter()
            data_writer.write_bytes(encoded.tobytes())

            stream = streams.InMemoryRandomAccessStream()
            await data_writer.store_async(stream)
            data_writer.detach_stream()
            stream.seek(0)

            decoder = await imaging.BitmapDecoder.create_async(stream)
            software_bitmap = await decoder.get_software_bitmap_async()

            result = await self.ocr_engine.recognize_async(software_bitmap)
            return result.text.strip()

        except Exception as e:
            self.log(f"OCR 실행 오류: {e}")
            return ""
