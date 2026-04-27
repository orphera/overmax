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
    LOGO_OCR_KEYWORD,
    LOGO_OCR_COOLDOWN_SEC,
    FREESTYLE_HISTORY_SIZE,
    FREESTYLE_ON_RATIO,
    FREESTYLE_ON_MIN_SAMPLES,
    FREESTYLE_OFF_RATIO,
    FREESTYLE_OFF_MIN_SAMPLES,
    JACKET_MATCH_INTERVAL,
    JACKET_SIMILARITY_LOG,
    JACKET_CHANGE_THRESHOLD,
    JACKET_FORCE_RECHECK_SEC,
    MODE_DIFF_HISTORY,
    RATE_OCR_INTERVAL,
)
from capture.roi_manager import ROIManager
from capture.window_tracker import WindowTracker, WindowRect
from detection.image_db import ImageDB
from detection.play_state import PlayStateDetector
from detection.ocr import OcrDetector
from core.game_state import GameSessionState
from capture.helpers import (
    crop_roi,
    has_thumbnail_changed,
    make_thumbnail,
)


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
        self.roiman = ROIManager()

        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_song_key = ""
        self._is_song_select = False

        self._init_callbacks()
        self._init_runtime_state()

        self.ocr_detector = OcrDetector(self.log)
        self.play_state_detector = PlayStateDetector(self.ocr_detector, history_size=MODE_DIFF_HISTORY)

    def _init_callbacks(self):
        self.on_state_changed: Optional[Callable[[GameSessionState], None]] = None
        self.on_screen_changed: Optional[Callable[[bool], None]] = None
        self.on_confidence_changed: Optional[Callable[[float], None]] = None
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

        self._last_emitted_state: Optional[GameSessionState] = None
        self._recorded_states: set = set()
        self._last_is_leaving = False
        self._last_logo_detected_val: Optional[bool] = None


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
        ocr_status = "활성" if self.ocr_detector.engine.is_available else "비활성"
        jacket_status = "활성" if (self.image_db and self.image_db.is_ready) else "비활성"
        record_status = "활성" if (self.record_db and self.record_db.is_ready) else "비활성"
        self.log(
            f"시작됨 (OCR: {ocr_status}, "
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
        self.roiman.update_window_size(rect.width, rect.height)
        full_frame = np.array(
            sct.grab({"top": rect.top, "left": rect.left, "width": rect.width, "height": rect.height})
        )
        now = time.time()

        is_song_select, is_leaving, confidence = await self._detect_song_select(full_frame)
        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            self.log(f"화면 변경: {'선곡화면' if is_song_select else '기타화면'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if self.on_confidence_changed:
            self.on_confidence_changed(confidence)

        if not is_song_select:
            self._reset_on_screen_exit()
            return

        if is_leaving:
            return

        self._update_song_id_from_jacket(full_frame, now)
        song_id = self._current_song_id
        
        state = await self.play_state_detector.detect(full_frame, self.roiman, song_id)
        self._emit_state_if_changed(state)
        
        # 새로운 유효 기록 수집 시도
        if state.is_valid and state.rate is not None:
            self._try_record_result(state)

    def _reset_on_screen_exit(self):
        self._current_song_id = None
        self._last_emitted_state = None
        self._recorded_states.clear()
        self.play_state_detector.reset()

    def _update_song_id_from_jacket(self, full_frame: np.ndarray, now: float):
        if not self._should_match_jacket(now):
            return

        self._last_jacket_ts = now
        jacket_roi = self.roiman.get_roi("jacket")
        jacket_img = crop_roi(full_frame, jacket_roi)
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

    def _emit_state_if_changed(self, state: GameSessionState):
        if state == self._last_emitted_state:
            return
        self._last_emitted_state = state
        if state.is_stable:
            self.log(f"상태 확정: {state}")
        if self.on_state_changed:
            self.on_state_changed(state)

    def _try_record_result(self, state: GameSessionState):
        """안정화된 상태와 Rate가 준비되었을 때 1회 기록 저장"""
        current_key = (state.song_id, state.mode, state.diff)
        if current_key in self._recorded_states:
            return

        success = False
        if self.record_db is not None and self.record_db.is_ready and state.rate > 0.0:
            self.log(f"기록 저장: {state.song_id}, {state.mode}, {state.diff}, {state.rate}%, MaxCombo: {state.is_max_combo}")
            success = self.record_db.upsert(
                state.song_id, 
                state.mode, 
                state.diff, 
                state.rate, 
                state.is_max_combo
            )
        
        if success:
            self._recorded_states.add(current_key)
            if self.on_record_updated:
                self.on_record_updated()

    # ------------------------------------------------------------------
    # 선곡화면 감지
    # ------------------------------------------------------------------

    async def _detect_song_select(self, full_frame: np.ndarray) -> tuple[bool, bool, float]:
        """선곡화면 여부와 이탈 여부, 신뢰도(0.0~1.0)를 반환.
        신뢰도는 히스테리시스 버퍼의 hit 비율로 정의된다.
        """
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

        # 이탈 중이면 신뢰도를 감소 방향으로 보정 (부드러운 fade-out 효과)
        confidence = ratio * (0.5 if is_leaving else 1.0)
        return is_song_select, is_leaving, confidence

    async def _detect_freestyle_logo(self, full_frame: np.ndarray) -> bool:
        logo_roi = self.roiman.get_roi("logo")
        logo_img = crop_roi(full_frame, logo_roi)
        now = time.time()
        if now - self._last_logo_ocr_ts >= LOGO_OCR_COOLDOWN_SEC:
            is_detected, text, normalized = await self.ocr_detector.detect_logo(logo_img)
            self._last_logo_ocr_ok = is_detected
            self._last_logo_ocr_ts = now
            if is_detected != self._last_logo_detected_val:
                self.log(f"로고 OCR: '{text}' (norm='{normalized}') -> {is_detected}")
                self._last_logo_detected_val = is_detected
        return self._last_logo_ocr_ok


