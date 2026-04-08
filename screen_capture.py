"""
screen_capture.py - 화면 캡처 및 OCR 모듈 (재킷 매칭 우선 버전)

곡 감지 우선순위:
  1. 재킷 이미지 매칭 (image_db.py)
  2. OCR 기반 곡명/작곡가 추출 (fallback)

단축키 F10 (기본): 현재 선택된 재킷을 DB에 수동 등록
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
from image_db import ImageDB

# ------------------------------------------------------------------
# 설정 상수 (비율 기반)
# ------------------------------------------------------------------
SCREEN_CAPTURE_SETTINGS = SETTINGS.get("screen_capture", {})
JACKET_SETTINGS = SETTINGS.get("jacket_matcher", {})

OCR_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("ocr_interval_sec", 0.35))
IDLE_SLEEP_INTERVAL = float(SCREEN_CAPTURE_SETTINGS.get("idle_sleep_sec", 0.5))

# 선곡화면 로고(FREESTYLE) 감지 영역
LOGO_X_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_start", 0.028))
LOGO_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("logo_x_end", 0.210))
LOGO_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_start", 0.015))
LOGO_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("logo_y_end", 0.090))

LOGO_OCR_KEYWORD = str(SCREEN_CAPTURE_SETTINGS.get("logo_ocr_keyword", "FREESTYLE")).upper()
LOGO_OCR_COOLDOWN_SEC = float(SCREEN_CAPTURE_SETTINGS.get("logo_ocr_cooldown_sec", 1.0))
FREESTYLE_HISTORY_SIZE = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_history_size", 7))
FREESTYLE_MAJORITY_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_majority_ratio", 0.60))
FREESTYLE_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_min_samples", 3))
FREESTYLE_ON_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_on_ratio", FREESTYLE_MAJORITY_RATIO))
FREESTYLE_ON_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_on_min_samples", FREESTYLE_MIN_SAMPLES))
FREESTYLE_OFF_RATIO = float(SCREEN_CAPTURE_SETTINGS.get("freestyle_off_ratio", 0.35))
FREESTYLE_OFF_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS.get("freestyle_off_min_samples", FREESTYLE_HISTORY_SIZE))

# OCR ROI
LEFT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_start", 0.028))
LEFT_TITLE_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_title_x_end", 0.265))
LEFT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_start", 0.180))
LEFT_TITLE_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_title_y_end", 0.305))

RIGHT_TITLE_X_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_start", 0.325))
RIGHT_TITLE_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("right_title_x_end", 0.660))
RIGHT_TITLE_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_start", 0.535))
RIGHT_TITLE_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("right_title_y_end", 0.602))
RIGHT_TITLE_PAD_PX = int(SCREEN_CAPTURE_SETTINGS.get("right_title_pad_px", 6))

LEFT_COMPOSER_X_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_start", 0.028))
LEFT_COMPOSER_X_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_x_end", 0.300))
LEFT_COMPOSER_Y_START = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_start", 0.245))
LEFT_COMPOSER_Y_END   = float(SCREEN_CAPTURE_SETTINGS.get("left_composer_y_end", 0.285))

# 재킷 ROI
JACKET_X_START = float(JACKET_SETTINGS.get("jacket_x_start", 0.370))
JACKET_X_END   = float(JACKET_SETTINGS.get("jacket_x_end",   0.401))
JACKET_Y_START = float(JACKET_SETTINGS.get("jacket_y_start", 0.494))
JACKET_Y_END   = float(JACKET_SETTINGS.get("jacket_y_end",   0.549))

# 재킷 매칭 관련
JACKET_MATCH_INTERVAL = float(JACKET_SETTINGS.get("match_interval_sec", 0.5))
JACKET_REGISTER_HOTKEY = str(JACKET_SETTINGS.get("register_hotkey", "F10"))
JACKET_SIMILARITY_LOG = bool(JACKET_SETTINGS.get("log_similarity", True))
JACKET_CHANGE_THRESHOLD = float(JACKET_SETTINGS.get("jacket_change_threshold", 2.5))
JACKET_FORCE_RECHECK_SEC = float(JACKET_SETTINGS.get("jacket_force_recheck_sec", 2.0))


class ScreenCapture:
    def __init__(self, tracker: WindowTracker, image_db: Optional[ImageDB] = None):
        self.tracker = tracker
        self.image_db = image_db  # None이면 재킷 매칭 비활성

        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_song_key = ""
        self._is_song_select = False

        # 콜백
        self.on_song_changed:   Optional[Callable[[str, str], None]]  = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None
        self.on_debug_log:      Optional[Callable[[str], None]]  = None
        # 재킷 등록 요청 시 곡명 조회용 (overlay controller에서 연결)
        self.on_jacket_register_request: Optional[Callable[[np.ndarray], None]] = None

        # asyncio
        self._loop: Optional[asyncio.AbstractEventLoop] = None

        # 로고 OCR 캐시
        self._last_logo_ocr_ts = 0.0
        self._last_logo_ocr_ok = False
        self._freestyle_history = deque(maxlen=max(1, FREESTYLE_HISTORY_SIZE))

        # 재킷 매칭 상태
        self._last_jacket_ts = 0.0
        self._last_jacket_img: Optional[np.ndarray] = None
        self._last_jacket_thumb: Optional[np.ndarray] = None
        self._last_jacket_match_ts = 0.0

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
    # 로그
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
        jacket_status = "활성" if (self.image_db and self.image_db.is_ready) else "비활성"
        self.log(f"시작됨 (Windows OCR: {WINDOWS_OCR_AVAILABLE}, 재킷 매칭: {jacket_status})")

    def stop(self):
        self._running = False

    def _loop_entry(self):
        self._loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self._loop)
        try:
            self._loop.run_until_complete(self._async_loop())
        finally:
            self._loop.close()

    # ------------------------------------------------------------------
    # 재킷 수동 등록 (외부에서 호출 - 단축키 트리거)
    # ------------------------------------------------------------------

    def trigger_jacket_register(self, song_id: str):
        """
        현재 캡처된 재킷 이미지를 song_id로 등록.
        단축키 핸들러에서 song_id를 얻어와 호출.
        """
        if self.image_db is None:
            self.log("재킷 DB 없음 - 등록 불가")
            return
        if self._last_jacket_img is None:
            self.log("저장된 재킷 이미지 없음 - 선곡화면에서 먼저 실행하세요")
            return
        if not song_id:
            self.log("곡명 없음 - 등록 취소")
            return

        img = self._last_jacket_img.copy()
        ok = self.image_db.register(song_id, img)
        if ok:
            self.log(f"재킷 등록 완료: '{song_id}'")
        else:
            self.log(f"재킷 등록 실패: '{song_id}'")

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

        # 2. 재킷 이미지 캡처 (항상 최신 유지 - 등록 트리거 대응)
        jacket_region = self._region_from_ratio(
            rect,
            JACKET_X_START, JACKET_X_END,
            JACKET_Y_START, JACKET_Y_END,
        )
        jacket_img = np.array(sct.grab(jacket_region))
        self._last_jacket_img = jacket_img

        # 3. 재킷 매칭 시도 (주기 제한)
        now = time.time()
        jacket_matched = False
        if (
            self.image_db is not None
            and self.image_db.is_ready
            and self.image_db.song_count > 0
            and now - self._last_jacket_ts >= JACKET_MATCH_INTERVAL
        ):
            self._last_jacket_ts = now
            thumb = cv2.resize(
                cv2.cvtColor(jacket_img, cv2.COLOR_BGRA2GRAY),
                (32, 32),
                interpolation=cv2.INTER_AREA,
            )
            image_changed = True
            if self._last_jacket_thumb is not None:
                diff = np.abs(thumb.astype(np.float32) - self._last_jacket_thumb.astype(np.float32))
                image_changed = float(np.mean(diff)) >= JACKET_CHANGE_THRESHOLD
            force_recheck = (now - self._last_jacket_match_ts) >= JACKET_FORCE_RECHECK_SEC

            if image_changed or force_recheck:
                self._last_jacket_thumb = thumb
                self._last_jacket_match_ts = now
                result = self.image_db.search(jacket_img)
                if result:
                    song_id, score = result
                    if JACKET_SIMILARITY_LOG:
                        self.log(f"재킷 매칭: '{song_id}' (유사도 {score:.4f})")
                    song_key = f"jacket::{song_id}"
                    if song_key != self._last_song_key:
                        self._last_song_key = song_key
                        # numeric song_id면 DB id lookup, 아니면 OCR fallback 유지
                        if str(song_id).isdigit():
                            if self.on_song_changed:
                                self.on_song_changed("", "", song_id=int(song_id))
                            jacket_matched = True
                        else:
                            self.log(
                                f"재킷 매칭 결과가 숫자 song_id가 아님: '{song_id}' -> OCR fallback"
                            )
                    else:
                        jacket_matched = True
                else:
                    if JACKET_SIMILARITY_LOG:
                        self.log("재킷 매칭 실패 → OCR fallback")

        # 4. OCR fallback (재킷 매칭 실패 시)
        if not jacket_matched:
            await self._ocr_fallback(sct, rect)

    async def _ocr_fallback(self, sct, rect: WindowRect):
        """OCR로 곡명/작곡가 추출 (재킷 매칭 실패 시 사용)"""
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

        song_key = f"ocr::{title}::{composer}".strip("::")
        if title and song_key != self._last_song_key:
            self._last_song_key = song_key
            if self.on_song_changed:
                self.on_song_changed(title, composer)

    # ------------------------------------------------------------------
    # ROI 헬퍼
    # ------------------------------------------------------------------

    def _region_from_ratio(
        self,
        rect: WindowRect,
        x_start: float, x_end: float,
        y_start: float, y_end: float,
    ) -> dict:
        return {
            "top":    rect.top  + int(rect.height * y_start),
            "left":   rect.left + int(rect.width  * x_start),
            "width":  max(1, int(rect.width  * (x_end - x_start))),
            "height": max(1, int(rect.height * (y_end - y_start))),
        }

    # ------------------------------------------------------------------
    # 텍스트 정규화
    # ------------------------------------------------------------------

    def _normalize_title_text(self, raw: str) -> str:
        if not raw:
            return ""
        lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        if not lines:
            return ""
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
        has_alnum_or_cjk = bool(re.search(
            r"[0-9A-Za-z\u3131-\u318E\uAC00-\uD7A3\u3040-\u30FF\u4E00-\u9FFF]", title
        ))
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
    # 선곡화면 감지
    # ------------------------------------------------------------------

    async def _detect_song_select(self, sct, rect: WindowRect) -> bool:
        logo_now = await self._detect_freestyle_logo(sct, rect)
        self._freestyle_history.append(logo_now)
        sample_count = len(self._freestyle_history)
        hit_count = sum(1 for v in self._freestyle_history if v)
        ratio = (hit_count / sample_count) if sample_count > 0 else 0.0

        if self._is_song_select:
            should_turn_off = (
                sample_count >= max(1, FREESTYLE_OFF_MIN_SAMPLES)
                and ratio <= FREESTYLE_OFF_RATIO
            )
            is_logo_majority = not should_turn_off
        else:
            is_logo_majority = (
                sample_count >= max(1, FREESTYLE_ON_MIN_SAMPLES)
                and ratio >= FREESTYLE_ON_RATIO
            )

        self.log(
            f"선곡판정 버퍼: hit={hit_count}/{sample_count} "
            f"(ratio={ratio:.2f}) -> {'선곡' if is_logo_majority else '기타'}"
        )
        return is_logo_majority

    async def _detect_freestyle_logo(self, sct, rect: WindowRect) -> bool:
        logo_region = {
            "top":    rect.top  + int(rect.height * LOGO_Y_START),
            "left":   rect.left + int(rect.width  * LOGO_X_START),
            "width":  max(1, int(rect.width  * (LOGO_X_END - LOGO_X_START))),
            "height": max(1, int(rect.height * (LOGO_Y_END - LOGO_Y_START))),
        }
        logo_img = np.array(sct.grab(logo_region))
        now = time.time()
        if now - self._last_logo_ocr_ts >= LOGO_OCR_COOLDOWN_SEC:
            text = await self._ocr_windows(logo_img)
            normalized = text.upper().replace("\n", " ").replace(" ", "")
            self._last_logo_ocr_ok = LOGO_OCR_KEYWORD.replace(" ", "") in normalized
            self._last_logo_ocr_ts = now
            self.log(f"로고 OCR: '{text}' -> {self._last_logo_ocr_ok}")
        return self._last_logo_ocr_ok

    # ------------------------------------------------------------------
    # Windows OCR
    # ------------------------------------------------------------------

    async def _ocr_windows(self, img_bgra: np.ndarray) -> str:
        if not WINDOWS_OCR_AVAILABLE or self.ocr_engine is None:
            return ""
        try:
            h, w = img_bgra.shape[:2]
            if w == 0 or h == 0:
                return ""

            scale = 3
            upscaled = cv2.resize(
                img_bgra, (w * scale, h * scale),
                interpolation=cv2.INTER_CUBIC,
            )
            gray = cv2.cvtColor(upscaled, cv2.COLOR_BGRA2GRAY)

            bg_mean = float(gray.mean())
            if bg_mean < 128:
                _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY | cv2.THRESH_OTSU)
            else:
                _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY_INV | cv2.THRESH_OTSU)

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
