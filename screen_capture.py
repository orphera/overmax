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
import re
from collections import deque
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

OCR_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("ocr_interval_sec", 0.35))   # 초
IDLE_SLEEP_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("idle_sleep_sec", 0.5))

# 선곡화면 로고(좌상단 FREESTYLE) 감지 영역
LOGO_X_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_start", 0.028))
LOGO_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_end", 0.210))
LOGO_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_start", 0.015))
LOGO_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_end", 0.090))

# 로고 OCR 판정
LOGO_OCR_KEYWORD = str(SCREEN_CAPTURE_SETTINGS.get("logo_ocr_keyword", "FREESTYLE")).upper()
LOGO_OCR_COOLDOWN_SEC = float(SCREEN_CAPTURE_SETTINGS.get("logo_ocr_cooldown_sec", 1.0))
FREESTYLE_HISTORY_SIZE = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_history_size", 7))
FREESTYLE_MAJORITY_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_majority_ratio", 0.60))
FREESTYLE_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_min_samples", 3))
FREESTYLE_ON_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_on_ratio", FREESTYLE_MAJORITY_RATIO))
FREESTYLE_ON_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_on_min_samples", FREESTYLE_MIN_SAMPLES))
FREESTYLE_OFF_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_off_ratio", 0.35))
FREESTYLE_OFF_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_off_min_samples", FREESTYLE_HISTORY_SIZE))

# 좌측 패널 곡명 OCR ROI
LEFT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_start", 0.028))
LEFT_TITLE_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_end", 0.265))
LEFT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_start", 0.180))
LEFT_TITLE_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_end", 0.305))

# 우측 선곡창 곡명 OCR ROI (하이라이트 y를 못 찾을 때 fallback)
RIGHT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_start", 0.325))
RIGHT_TITLE_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_end", 0.660))
RIGHT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_start", 0.535))
RIGHT_TITLE_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_end", 0.602))

# 하이라이트 y를 찾았을 때 우측 ROI에 추가할 여유(padding px)
RIGHT_TITLE_PAD_PX = int(SCREEN_CAPTURE_SETTINGS.get("right_title_pad_px", 6))

# 좌측 패널 작곡가 OCR ROI
LEFT_COMPOSER_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_start", 0.028))
LEFT_COMPOSER_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_end", 0.300))
LEFT_COMPOSER_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_start", 0.245))
LEFT_COMPOSER_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_end", 0.285))


class ScreenCapture:
    def __init__(self, tracker: WindowTracker):
        self.tracker = tracker
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_song_key = ""
        self._is_song_select = False

        # 콜백
        self.on_song_changed:   Optional[Callable[[str, str], None]]  = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None
        self.on_debug_log:      Optional[Callable[[str], None]]  = None

        # 스레드 내 asyncio 이벤트 루프 (한 번만 생성)
        self._loop: Optional[asyncio.AbstractEventLoop] = None

        # 로고 OCR 결과 캐시 (매 프레임 OCR 과부하 방지)
        self._last_logo_ocr_ts = 0.0
        self._last_logo_ocr_ok = False
        self._freestyle_history = deque(maxlen=max(1, FREESTYLE_HISTORY_SIZE))

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
        is_song_select = await self._detect_song_select(sct, rect)

        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            self.log(f"화면 변경: {'선곡화면' if is_song_select else '기타화면'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if not is_song_select:
            return

        left_region = self._region_from_ratio(
            rect,
            LEFT_TITLE_X_START, LEFT_TITLE_X_END,
            LEFT_TITLE_Y_START, LEFT_TITLE_Y_END,
        )
        left_raw = await self._ocr_windows(np.array(sct.grab(left_region)))
        left_title = self._normalize_title_text(left_raw)

        right_region = self._region_from_ratio(
            rect,
            RIGHT_TITLE_X_START, RIGHT_TITLE_X_END,
            RIGHT_TITLE_Y_START, RIGHT_TITLE_Y_END,
        )
        right_raw = await self._ocr_windows(np.array(sct.grab(right_region)))
        right_title = self._normalize_title_text(right_raw)

        composer_region = self._region_from_ratio(
            rect,
            LEFT_COMPOSER_X_START, LEFT_COMPOSER_X_END,
            LEFT_COMPOSER_Y_START, LEFT_COMPOSER_Y_END,
        )
        composer_raw = await self._ocr_windows(np.array(sct.grab(composer_region)))
        composer = self._normalize_composer_text(composer_raw)

        title = self._choose_title(left_title, right_title)
        self.log(
            f"OCR 후보: left='{left_title}' / right='{right_title}' / "
            f"composer='{composer}' -> 선택='{title}'"
        )

        song_key = f"{title}::{composer}".strip(":")
        if title and song_key != self._last_song_key:
            self._last_song_key = song_key
            if self.on_song_changed:
                self.on_song_changed(title, composer)

    def _region_from_ratio(
        self,
        rect: WindowRect,
        x_start: float, x_end: float,
        y_start: float, y_end: float,
    ) -> dict:
        return {
            "top": rect.top + int(rect.height * y_start),
            "left": rect.left + int(rect.width * x_start),
            "width": max(1, int(rect.width * (x_end - x_start))),
            "height": max(1, int(rect.height * (y_end - y_start))),
        }

    def _normalize_title_text(self, raw: str) -> str:
        if not raw:
            return ""
        lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        if not lines:
            return ""
        # 첫 줄(곡명)을 우선 사용하되, 너무 짧으면 다음 줄 보조
        title = lines[0]
        if len(title) < 2 and len(lines) > 1:
            title = lines[1]
        title = re.sub(r"\s+", " ", title).strip()
        return title

    def _normalize_composer_text(self, raw: str) -> str:
        if not raw:
            return ""
        lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        if not lines:
            return ""
        composer = re.sub(r"\s+", " ", lines[0]).strip()
        return composer

    def _score_title(self, title: str, prefer_right: bool) -> int:
        if not title:
            return -999
        has_alnum_or_cjk = bool(re.search(r"[0-9A-Za-z\u3131-\u318E\uAC00-\uD7A3\u3040-\u30FF\u4E00-\u9FFF]", title))
        score = len(title) * 2
        if has_alnum_or_cjk:
            score += 8
        if prefer_right:
            score += 6
        return score

    def _choose_title(self, left_title: str, right_title: str) -> str:
        left_score = self._score_title(left_title, prefer_right=False)
        right_score = self._score_title(right_title, prefer_right=True)
        return right_title if right_score >= left_score else left_title

    # ------------------------------------------------------------------
    # 선곡화면 감지 (주황 클러스터 방식)
    # ------------------------------------------------------------------

    async def _detect_song_select(self, sct, rect: WindowRect):
        """
        1) 좌상단 FREESTYLE 로고 특징으로 선곡화면 여부를 1차 판정
        2) 고정 OCR ROI 사용

        Returns:
            is_song_select
        """
        logo_now = await self._detect_freestyle_logo(sct, rect)
        self._freestyle_history.append(logo_now)
        sample_count = len(self._freestyle_history)
        hit_count = sum(1 for v in self._freestyle_history if v)
        ratio = (hit_count / sample_count) if sample_count > 0 else 0.0
        if self._is_song_select:
            # 느리게 OFF: 충분한 샘플이 쌓이고 ratio가 낮을 때만 해제
            should_turn_off = (
                sample_count >= max(1, FREESTYLE_OFF_MIN_SAMPLES)
                and ratio <= FREESTYLE_OFF_RATIO
            )
            is_logo_majority = not should_turn_off
        else:
            # 빠르게 ON: 비교적 적은 샘플에서도 ratio가 높으면 진입
            is_logo_majority = (
                sample_count >= max(1, FREESTYLE_ON_MIN_SAMPLES)
                and ratio >= FREESTYLE_ON_RATIO
            )
        self.log(
            f"선곡판정 버퍼: hit={hit_count}/{sample_count} "
            f"(ratio={ratio:.2f}, on>={FREESTYLE_ON_RATIO:.2f}/{FREESTYLE_ON_MIN_SAMPLES}, "
            f"off<={FREESTYLE_OFF_RATIO:.2f}/{FREESTYLE_OFF_MIN_SAMPLES}) -> "
            f"{'선곡' if is_logo_majority else '기타'}"
        )

        if not is_logo_majority:
            return False

        return True

    async def _detect_freestyle_logo(self, sct, rect: WindowRect) -> bool:
        """
        좌상단 로고 영역 OCR 결과에 FREESTYLE 키워드가 포함되는지 판정.
        """
        logo_region = {
            "top": rect.top + int(rect.height * LOGO_Y_START),
            "left": rect.left + int(rect.width * LOGO_X_START),
            "width": max(1, int(rect.width * (LOGO_X_END - LOGO_X_START))),
            "height": max(1, int(rect.height * (LOGO_Y_END - LOGO_Y_START))),
        }
        logo_img = np.array(sct.grab(logo_region))  # BGRA
        now = time.time()
        if now - self._last_logo_ocr_ts >= LOGO_OCR_COOLDOWN_SEC:
            text = await self._ocr_windows(logo_img)
            normalized = text.upper().replace("\n", " ").replace(" ", "")
            self._last_logo_ocr_ok = LOGO_OCR_KEYWORD.replace(" ", "") in normalized
            self._last_logo_ocr_ts = now
            self.log(f"로고 OCR: '{text}' -> {self._last_logo_ocr_ok}")

        return self._last_logo_ocr_ok

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

            stream = streams.InMemoryRandomAccessStream()
            data_writer = streams.DataWriter(stream)
            data_writer.write_bytes(encoded.tobytes())
            await data_writer.store_async()
            data_writer.detach_stream()
            stream.seek(0)

            decoder = await imaging.BitmapDecoder.create_async(stream)
            software_bitmap = await decoder.get_software_bitmap_async()

            result = await self.ocr_engine.recognize_async(software_bitmap)
            return result.text.strip()

        except Exception as e:
            self.log(f"OCR 실행 오류: {e}")
            return ""
