"""
screen_capture.py - 화면 캡처 및 OCR 모듈

동작 방식:
  매 프레임 (OCR_INTERVAL 마다):
  1. FREESTYLE 로고 OCR → 선곡화면 감지
  2. 재킷 이미지 매칭 → song_id
  3. 밝기 비교 → mode, diff  (OCR 없음)
  4. (song_id, mode, diff) 3종이 같은 프레임에서 N회 연속 동일 + 밝기 신뢰 → 안정  
  5. 안정 상태 확정 시 verified=True 콜백, 이후 쿨다운마다 Rate OCR 반복 수집
"""

import time
import threading
import asyncio
from collections import deque
import numpy as np
from typing import Optional, Callable
import mss
import cv2
from constants import (
    OCR_INTERVAL,
    IDLE_SLEEP_INTERVAL,
    LOGO_X_START,
    LOGO_X_END,
    LOGO_Y_START,
    LOGO_Y_END,
    LOGO_OCR_KEYWORD,
    LOGO_OCR_COOLDOWN_SEC,
    FREESTYLE_HISTORY_SIZE,
    FREESTYLE_ON_RATIO,
    FREESTYLE_ON_MIN_SAMPLES,
    FREESTYLE_OFF_RATIO,
    FREESTYLE_OFF_MIN_SAMPLES,
    JACKET_X_START,
    JACKET_X_END,
    JACKET_Y_START,
    JACKET_Y_END,
    JACKET_MATCH_INTERVAL,
    JACKET_SIMILARITY_LOG,
    JACKET_CHANGE_THRESHOLD,
    JACKET_FORCE_RECHECK_SEC,
    MODE_DIFF_HISTORY,
    RATE_X1,
    RATE_Y1,
    RATE_X2,
    RATE_Y2,
    RATE_OCR_INTERVAL,
)

try:
    import winrt.windows.media.ocr as ocr
    import winrt.windows.graphics.imaging as imaging
    import winrt.windows.storage.streams as streams
    WINDOWS_OCR_AVAILABLE = True
except ImportError:
    WINDOWS_OCR_AVAILABLE = False

from capture.window_tracker import WindowTracker, WindowRect
from detection.image_db import ImageDB
from detection.mode_diff import detect_mode_and_difficulty
from core.game_state import GameSessionState
from capture.helpers import (
    build_ratio_region,
    crop_ratio_region,
    has_thumbnail_changed,
    is_logo_keyword_match,
    make_rate_roi,
    make_thumbnail,
    normalize_alnum,
    parse_rate_text,
    preprocess_for_ocr,
)

# (Constants moved to constants.py)


class ScreenCapture:
    def __init__(
        self,
        tracker: WindowTracker,
        image_db: Optional[ImageDB] = None,
        record_db=None,   # RecordDB | None
    ):
        self.tracker = tracker
        self.image_db = image_db
        self.record_db = record_db

        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_song_key = ""
        self._is_song_select = False

        self._init_callbacks()
        self._init_runtime_state()
        self.ocr_engine = self._create_ocr_engine()

    def _init_callbacks(self):
        self.on_state_changed: Optional[Callable[[GameSessionState], None]] = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None
        self.on_debug_log: Optional[Callable[[str], None]] = None
        self.on_record_updated: Optional[Callable[[], None]] = None

    def _init_runtime_state(self):
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self._last_logo_ocr_ts = 0.0
        self._last_logo_ocr_ok = False
        self._freestyle_history = deque(maxlen=max(1, FREESTYLE_HISTORY_SIZE))

        self._current_song_id: Optional[int] = None
        self._last_jacket_ts = 0.0
        self._last_jacket_thumb: Optional[np.ndarray] = None
        self._last_jacket_match_ts = 0.0

        self._state_history: deque = deque(maxlen=max(1, MODE_DIFF_HISTORY))
        self._last_emitted_state: Optional[GameSessionState] = None
        self._recorded_states: set = set()
        self._last_rate_ocr_ts = 0.0
        self._last_is_leaving = False
        self._last_logo_detected_val: Optional[bool] = None

    def _create_ocr_engine(self):
        if not WINDOWS_OCR_AVAILABLE:
            return None
        try:
            supported_langs = ocr.OcrEngine.available_recognizer_languages
            target_lang = next((lang for lang in supported_langs if "ko" in lang.language_tag.lower()), None)
            if target_lang is None and supported_langs:
                target_lang = supported_langs[0]

            if target_lang:
                self.log(f"OCR 엔진 언어: {target_lang.language_tag}")
                return ocr.OcrEngine.try_create_from_language(target_lang)

            self.log("OCR 엔진: user profile 언어 사용")
            return ocr.OcrEngine.try_create_from_user_profile_languages()
        except Exception as exc:
            self.log(f"OCR 엔진 초기화 실패: {exc}")
            return None

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
        record_status = "활성" if (self.record_db and self.record_db.is_ready) else "비활성"
        self.log(
            f"시작됨 (Windows OCR: {WINDOWS_OCR_AVAILABLE}, "
            f"재킷 매칭: {jacket_status}, 기록 수집: {record_status})"
        )

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
        full_frame = np.array(
            sct.grab({"top": rect.top, "left": rect.left, "width": rect.width, "height": rect.height})
        )
        now = time.time()

        is_song_select, is_leaving = await self._detect_song_select(full_frame)
        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            self.log(f"화면 변경: {'선곡화면' if is_song_select else '기타화면'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if not is_song_select:
            self._reset_on_screen_exit()
            return

        if is_leaving:
            self.log("선곡 판정 하락 중 - 인식 skip")
            return

        self._update_song_id_from_jacket(full_frame, now)
        mode, diff, is_confident = detect_mode_and_difficulty(full_frame)
        song_id = self._current_song_id
        current = (song_id, mode, diff)
        is_stable = self._update_stability(current, is_confident)
        state = GameSessionState(
            song_id=song_id,
            mode=mode,
            diff=diff,
            is_stable=is_stable,
        )

        self._emit_state_if_changed(state)
        await self._try_record_rate(full_frame, current, is_stable, now)

    def _reset_on_screen_exit(self):
        self._state_history.clear()
        self._current_song_id = None
        self._last_emitted_state = None
        self._recorded_states.clear()

    def _update_song_id_from_jacket(self, full_frame: np.ndarray, now: float):
        if not self._should_match_jacket(now):
            return

        self._last_jacket_ts = now
        jacket_img = crop_ratio_region(full_frame, JACKET_X_START, JACKET_X_END, JACKET_Y_START, JACKET_Y_END)
        thumb = make_thumbnail(jacket_img)
        image_changed = has_thumbnail_changed(thumb, self._last_jacket_thumb, JACKET_CHANGE_THRESHOLD)
        force_recheck = (now - self._last_jacket_match_ts) >= JACKET_FORCE_RECHECK_SEC
        if not (image_changed or force_recheck):
            return

        self._last_jacket_thumb = thumb
        self._last_jacket_match_ts = now
        if image_changed:
            self._current_song_id = None
        self._current_song_id = self._search_song_id_from_jacket(jacket_img)

    def _should_match_jacket(self, now: float) -> bool:
        return (
            self.image_db is not None
            and self.image_db.is_ready
            and self.image_db.song_count > 0
            and now - self._last_jacket_ts >= JACKET_MATCH_INTERVAL
        )

    def _search_song_id_from_jacket(self, jacket_img: np.ndarray) -> Optional[int]:
        result = self.image_db.search(jacket_img)
        if not result:
            return None
        sid, score = result
        if JACKET_SIMILARITY_LOG:
            self.log(f"재킷 매칭: '{sid}' (유사도 {score:.4f})")
        return int(sid) if str(sid).isdigit() else None

    def _update_stability(self, current: tuple, is_confident: bool) -> bool:
        valid = all(current) and is_confident
        self._state_history.append(current if valid else None)
        history = list(self._state_history)
        return (
            len(history) == self._state_history.maxlen
            and len(set(history)) == 1
            and history[0] is not None
        )

    def _emit_state_if_changed(self, state: GameSessionState):
        if state == self._last_emitted_state:
            return
        self._last_emitted_state = state
        if state.is_stable:
            self.log(f"상태 확정: {state}")
            self._last_rate_ocr_ts = 0.0
        else:
            self.log(f"상태 감지: {state}")
        if self.on_state_changed:
            self.on_state_changed(state)

    async def _try_record_rate(self, full_frame: np.ndarray, current: tuple, is_stable: bool, now: float):
        if not is_stable or current in self._recorded_states:
            return
        if now - self._last_rate_ocr_ts < RATE_OCR_INTERVAL:
            return
        self._last_rate_ocr_ts = now
        song_id, mode, diff = current
        success = await self._do_record_rate(full_frame, song_id, mode, diff)
        if success:
            self._recorded_states.add(current)

    # ------------------------------------------------------------------
    # Rate OCR + RecordDB 저장
    # ------------------------------------------------------------------

    async def _do_record_rate(
        self,
        full_frame: np.ndarray,
        song_id: int,
        mode: str,
        diff: str,
    ) -> bool:
        """
        Rate 영역 OCR 수행 후 RecordDB에 저장.
        반환: True = 성공 (recorded_states에 추가 가능), False = 실패 (재시도 예정)
        """
        roi_bgra = make_rate_roi(full_frame, RATE_X1, RATE_Y1, RATE_X2, RATE_Y2)
        text = await self._ocr_windows(roi_bgra)
        rate = parse_rate_text(text)

        if rate is None and not text:
            self.log(f"Rate OCR 빈 결과 ({song_id} {mode}/{diff}) - 이진화 재시도")
            text = await self._ocr_windows(roi_bgra, force_invert=True)
            rate = parse_rate_text(text)
        if rate is None:
            self.log(f"Rate OCR 파싱 실패: '{text}' ({song_id} {mode}/{diff})")
            return False

        self.log(f"Rate OCR: {song_id} {mode}/{diff} = {rate:.2f}% (raw='{text}')")

        if rate == 0.0:
            self.log("Rate 0.00% - 미플레이로 간주, 저장 skip")
            return True

        if self.record_db is not None and self.record_db.is_ready and self.record_db.upsert(song_id, mode, diff, rate):
            if self.on_record_updated:
                self.on_record_updated()

        return True

    # ------------------------------------------------------------------
    # ROI 헬퍼
    # ------------------------------------------------------------------

    def _region_from_ratio(self, rect, x_start, x_end, y_start, y_end) -> dict:
        return build_ratio_region(rect, x_start, x_end, y_start, y_end)

    def _parse_rate(self, text: str) -> Optional[float]:
        return parse_rate_text(text)

    # ------------------------------------------------------------------
    # 선곡화면 감지
    # ------------------------------------------------------------------

    async def _detect_song_select(self, full_frame: np.ndarray) -> tuple[bool, bool]:
        logo_now = await self._detect_freestyle_logo(full_frame)
        self._freestyle_history.append(logo_now)
        sample_count = len(self._freestyle_history)
        hit_count    = sum(1 for v in self._freestyle_history if v)
        ratio        = (hit_count / sample_count) if sample_count > 0 else 0.0

        if self._is_song_select:
            should_turn_off = (
                sample_count >= max(1, FREESTYLE_OFF_MIN_SAMPLES)
                and ratio <= FREESTYLE_OFF_RATIO
            )
            is_song_select = not should_turn_off
        else:
            is_song_select = (
                sample_count >= max(1, FREESTYLE_ON_MIN_SAMPLES)
                and ratio >= FREESTYLE_ON_RATIO
            )

        is_leaving = False
        if is_song_select and sample_count >= 4:
            half = sample_count // 2
            history_list = list(self._freestyle_history)
            first_half_ratio  = sum(history_list[:half]) / half
            second_half_ratio = sum(history_list[half:]) / (sample_count - half)
            if second_half_ratio < first_half_ratio:
                is_leaving = True

        if is_song_select != self._is_song_select or is_leaving != self._last_is_leaving:
            self.log(
                f"선곡판정 버퍼: hit={hit_count}/{sample_count} "
                f"(ratio={ratio:.2f}) -> {'선곡' if is_song_select else '기타화면'}"
                + (f" [이탈중]" if is_leaving else "")
            )
            self._last_is_leaving = is_leaving
        return is_song_select, is_leaving

    async def _detect_freestyle_logo(self, full_frame: np.ndarray) -> bool:
        logo_img = crop_ratio_region(full_frame, LOGO_X_START, LOGO_X_END, LOGO_Y_START, LOGO_Y_END)
        now = time.time()
        if now - self._last_logo_ocr_ts >= LOGO_OCR_COOLDOWN_SEC:
            text = await self._ocr_windows(logo_img)
            normalized = normalize_alnum(text)
            keyword = normalize_alnum(LOGO_OCR_KEYWORD)
            is_detected = is_logo_keyword_match(keyword, normalized)
            self._last_logo_ocr_ok = is_detected
            self._last_logo_ocr_ts = now
            if is_detected != self._last_logo_detected_val:
                self.log(f"로고 OCR: '{text}' (norm='{normalized}') -> {is_detected}")
                self._last_logo_detected_val = is_detected
        return self._last_logo_ocr_ok

    # ------------------------------------------------------------------
    # Windows OCR
    # ------------------------------------------------------------------

    async def _ocr_windows(self, img_bgra: np.ndarray, force_invert: bool = False) -> str:
        if not WINDOWS_OCR_AVAILABLE or self.ocr_engine is None:
            return ""
        try:
            thresh = preprocess_for_ocr(img_bgra, force_invert=force_invert)
            if thresh is None:
                return ""

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

            stream.close()

            return result.text.strip()
        except Exception as e:
            self.log(f"OCR 실행 오류: {e}")
            return ""
