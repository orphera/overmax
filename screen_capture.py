"""
screen_capture.py - 화면 캡처 및 OCR 모듈 (재킷 매칭 우선 버전)

곡 감지 우선순위:
  1. 재킷 이미지 매칭 (image_db.py)
  2. OCR 기반 곡명/작곡가 추출 (fallback)

추가:
  - 버튼 모드(4B/5B/6B/8B) 감지
  - 선택된 난이도(NM/HD/MX/SC) 감지
  - on_mode_diff_changed 콜백
  - Rate OCR 자동 수집 → RecordDB 저장
"""

import time
import threading
import asyncio
import re
import difflib
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
from mode_diff_detector import detect_mode_and_difficulty, get_difficulty_roi

# ------------------------------------------------------------------
# 설정 상수 (비율 기반)
# ------------------------------------------------------------------
SCREEN_CAPTURE_SETTINGS = SETTINGS["screen_capture"]
JACKET_SETTINGS = SETTINGS["jacket_matcher"]

OCR_INTERVAL = float(SCREEN_CAPTURE_SETTINGS["ocr_interval_sec"])
IDLE_SLEEP_INTERVAL = float(SCREEN_CAPTURE_SETTINGS["idle_sleep_sec"])

# 선곡화면 로고(FREESTYLE) 감지 영역
LOGO_X_START = float(SCREEN_CAPTURE_SETTINGS["logo_x_start"])
LOGO_X_END   = float(SCREEN_CAPTURE_SETTINGS["logo_x_end"])
LOGO_Y_START = float(SCREEN_CAPTURE_SETTINGS["logo_y_start"])
LOGO_Y_END   = float(SCREEN_CAPTURE_SETTINGS["logo_y_end"])

LOGO_OCR_KEYWORD = str(SCREEN_CAPTURE_SETTINGS["logo_ocr_keyword"]).upper()
LOGO_OCR_COOLDOWN_SEC = float(SCREEN_CAPTURE_SETTINGS["logo_ocr_cooldown_sec"])
FREESTYLE_HISTORY_SIZE = int(SCREEN_CAPTURE_SETTINGS["freestyle_history_size"])
FREESTYLE_MAJORITY_RATIO = float(SCREEN_CAPTURE_SETTINGS["freestyle_majority_ratio"])
FREESTYLE_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS["freestyle_min_samples"])
FREESTYLE_ON_RATIO = float(SCREEN_CAPTURE_SETTINGS["freestyle_on_ratio"])
FREESTYLE_ON_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS["freestyle_on_min_samples"])
FREESTYLE_OFF_RATIO = float(SCREEN_CAPTURE_SETTINGS["freestyle_off_ratio"])
FREESTYLE_OFF_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS["freestyle_off_min_samples"])

# 재킷 ROI
JACKET_X_START = float(JACKET_SETTINGS["jacket_x_start"])
JACKET_X_END   = float(JACKET_SETTINGS["jacket_x_end"])
JACKET_Y_START = float(JACKET_SETTINGS["jacket_y_start"])
JACKET_Y_END   = float(JACKET_SETTINGS["jacket_y_end"])

# 재킷 매칭 관련
JACKET_MATCH_INTERVAL = float(JACKET_SETTINGS["match_interval_sec"])
JACKET_SIMILARITY_LOG = bool(JACKET_SETTINGS["log_similarity"])
JACKET_CHANGE_THRESHOLD = float(JACKET_SETTINGS["jacket_change_threshold"])
JACKET_FORCE_RECHECK_SEC = float(JACKET_SETTINGS["jacket_force_recheck_sec"])

# 모드/난이도 감지 관련
_MODE_DIFF_SETTINGS = SETTINGS.get("mode_diff_detector", {})
MODE_DIFF_INTERVAL = float(_MODE_DIFF_SETTINGS.get("interval_sec", 0.15))
MODE_DIFF_HISTORY  = int(_MODE_DIFF_SETTINGS.get("history_size", 3))

# Rate OCR 관련 (1920x1080 기준 픽셀 좌표)
_RATE_X1, _RATE_Y1 = 176, 583
_RATE_X2, _RATE_Y2 = 270, 605   # 오른쪽 여유 확보 (100.00% 대응)
RATE_OCR_INTERVAL  = 1.5        # 같은 패턴 재OCR 최소 간격 (초)


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

        # 콜백
        self.on_song_changed:      Optional[Callable[[int], None]]            = None
        self.on_screen_changed:    Optional[Callable[[bool], None]]           = None
        self.on_debug_log:         Optional[Callable[[str], None]]            = None
        self.on_mode_diff_changed: Optional[Callable[[str, str], None]]       = None
        self.on_record_updated:    Optional[Callable[[], None]]               = None

        # asyncio
        self._loop: Optional[asyncio.AbstractEventLoop] = None

        # 로고 OCR 캐시
        self._last_logo_ocr_ts = 0.0
        self._last_logo_ocr_ok = False
        self._freestyle_history = deque(maxlen=max(1, FREESTYLE_HISTORY_SIZE))

        # 재킷 매칭 상태
        self._last_jacket_ts = 0.0
        self._last_jacket_thumb: Optional[np.ndarray] = None
        self._last_jacket_match_ts = 0.0
        self._last_jacket_matched = False

        # 모드/난이도 감지 상태
        self._last_mode_diff_ts = 0.0
        self._last_mode: Optional[str] = None
        self._last_diff: Optional[str] = None
        self._mode_history: deque[Optional[str]] = deque(maxlen=MODE_DIFF_HISTORY)
        self._diff_history: deque[Optional[str]] = deque(maxlen=MODE_DIFF_HISTORY)
        self._diff_verified = False
        self._verified_diff: Optional[str] = None

        # Rate OCR 상태
        self._last_rate_ocr_ts: float = 0.0
        self._last_rate_key:    str   = ""   # "song_id:mode:diff"

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
        # 1. 선곡화면 감지
        is_song_select, is_leaving = await self._detect_song_select(sct, rect)

        if is_song_select != self._is_song_select:
            self._is_song_select = is_song_select
            self.log(f"화면 변경: {'선곡화면' if is_song_select else '기타화면'}")
            if self.on_screen_changed:
                self.on_screen_changed(is_song_select)

        if not is_song_select:
            self._last_jacket_matched = False
            self._mode_history.clear()
            self._diff_history.clear()
            return

        if is_leaving:
            self.log("선곡 판정 하락 중 - 인식 skip")
            return

        # 2. 전체 화면 스냅샷
        full_region = {
            "top":    rect.top,
            "left":   rect.left,
            "width":  rect.width,
            "height": rect.height,
        }
        full_frame = np.array(sct.grab(full_region))  # BGRA

        now = time.time()

        # 3. 재킷 ROI
        h, w = full_frame.shape[:2]
        jx1 = int(w * JACKET_X_START)
        jy1 = int(h * JACKET_Y_START)
        jx2 = int(w * JACKET_X_END)
        jy2 = int(h * JACKET_Y_END)
        jacket_img = full_frame[jy1:jy2, jx1:jx2]

        # 4. 재킷 매칭
        jacket_matched = (
            self._last_jacket_matched
            and self.image_db is not None
            and self.image_db.is_ready
            and self.image_db.song_count > 0
        )
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
                        # 곡이 바뀌면 Rate 키 초기화 (새 곡에서 즉시 OCR 가능하게)
                        self._last_rate_key = ""
                        if str(song_id).isdigit():
                            if self.on_song_changed:
                                self.on_song_changed(int(song_id))
                            jacket_matched = True
                            self._last_jacket_matched = True
                        else:
                            jacket_matched = False
                            self._last_jacket_matched = False
                    else:
                        jacket_matched = True
                        self._last_jacket_matched = True
                else:
                    jacket_matched = False
                    self._last_jacket_matched = False

        # 5. 버튼 모드 / 난이도 감지
        raw_mode, raw_diff = (None, None)
        if jacket_matched:
            # 매 프레임 감지하여 히스토리 업데이트
            raw_mode, raw_diff = self._update_mode_diff(full_frame)

            # 6. Rate OCR (song_id + mode + diff 모두 확정된 경우)
            # 현재 프레임의 raw_mode/diff가 stable한 last_mode/diff와 일치할 때만 수행
            await self._try_record_rate(full_frame, now, raw_mode, raw_diff)

        # 7. OCR fallback
        if not jacket_matched:
            await self._ocr_fallback(sct, rect)

    # ------------------------------------------------------------------
    # 버튼 모드 / 난이도 감지
    # ------------------------------------------------------------------

    def _update_mode_diff(self, full_frame: np.ndarray) -> tuple[Optional[str], Optional[str]]:
        raw_mode, raw_diff = detect_mode_and_difficulty(full_frame)

        self._mode_history.append(raw_mode)
        self._diff_history.append(raw_diff)

        if (
            len(self._mode_history) < MODE_DIFF_HISTORY
            or len(self._diff_history) < MODE_DIFF_HISTORY
        ):
            return raw_mode, raw_diff

        stable_mode = self._majority(self._mode_history)
        stable_diff = self._majority(self._diff_history)

        if stable_mode != self._last_mode or stable_diff != self._last_diff:
            self.log(
                f"모드/난이도 변경 감지: old=({self._last_mode},{self._last_diff}) "
                f"new=({stable_mode},{stable_diff})"
            )
            self._last_mode = stable_mode
            self._last_diff = stable_diff
            
            # 난이도가 바뀌면 검증 상태 초기화
            self._diff_verified = False
            self._verified_diff = None
            
            # 모드/난이도가 바뀌면 Rate 키 초기화 (새 조합 즉시 OCR 가능하게)
            self._last_rate_key = ""
            if self.on_mode_diff_changed and stable_mode and stable_diff:
                self.on_mode_diff_changed(stable_mode, stable_diff, False) # 초기 상태는 미검증
        
        # 난이도 검증 트리거 (미검증 상태이고 stable_diff가 있는 경우)
        if stable_diff and not self._diff_verified:
            # CPU/OCR 부하를 위해 별도 비동기 태스크로 실행
            asyncio.create_task(self._verify_difficulty_async(full_frame, stable_diff))

        return raw_mode, raw_diff

    async def _verify_difficulty_async(self, full_frame: np.ndarray, diff: str):
        """밝기 기반으로 감지된 난이도가 실제 맞는지 OCR로 검증 (Fuzzy matching + Fallback)"""
        if self._diff_verified and self._verified_diff == diff:
            return

        # ROI 추출
        rx1, ry1, rx2, ry2 = get_difficulty_roi(diff)
        h, w = full_frame.shape[:2]
        x1, y1 = int(rx1 * w), int(ry1 * h)
        x2, y2 = int(rx2 * w), int(ry2 * h)
        roi_bgra = full_frame[y1:y2, x1:x2]

        # 1차 시도
        text = await self._ocr_windows(roi_bgra)
        is_ok, match_info = self._check_difficulty_keywords(text, diff)

        # 2차 시도 (1차 실패 시 반전 처리하여 재시도)
        if not is_ok:
            text_alt = await self._ocr_windows(roi_bgra, force_invert=True)
            is_ok, match_info = self._check_difficulty_keywords(text_alt, diff)
            if is_ok:
                text = text_alt

        if is_ok:
            if not self._diff_verified:
                self.log(f"난이도 검증 완료: {diff} (OCR: '{text}', match={match_info})")
            self._diff_verified = True
            self._verified_diff = diff
            
            # 오버레이에 검증 완료 알림
            if self.on_mode_diff_changed:
                self.on_mode_diff_changed(self._last_mode, diff, True)
        else:
            # 검증 실패 시 로그 (실제 OCR 결과를 보여주어 디버깅 용이하게 함)
            if self._verified_diff != f"FAILED_{diff}_{text}":
                self.log(f"난이도 검증 실패: {diff} (인식된 문자: '{text}') - 검증될 때까지 저장 안 함")
                self._verified_diff = f"FAILED_{diff}_{text}"
                self._diff_verified = False
                
                # 검증 실패 상태 알림 (이미 False였겠지만 명시적으로 보냄)
                if self.on_mode_diff_changed:
                    self.on_mode_diff_changed(self._last_mode, diff, False)

    def _check_difficulty_keywords(self, text: str, expected_diff: str) -> tuple[bool, str]:
        """OCR 텍스트와 기대 난이도 간의 키워드 매칭 (부분 일치 및 유사도 검사)"""
        normalized = text.upper().strip().replace(" ", "")
        if not normalized:
            return False, ""

        # 각 난이도별 허용 키워드
        keywords_map = {
            "NM": ["NORMAL", "NORM"],
            "HD": ["HARD"],
            "MX": ["MAX", "MAXIMUM", "MAXIMUN"],
            "SC": ["SC", "5C", "8C", "BC", "CC", "OC", "S.C"] # SC 오인식 변종 대폭 추가
        }
        
        target_keywords = keywords_map.get(expected_diff, [])
        
        # 1. 단순 포함 여부 확인
        for k in target_keywords:
            if k in normalized:
                return True, f"'{k}'(exact)"

        # 2. 유사도 기반 퍼지 매칭 (difflib 사용)
        import difflib
        for k in target_keywords:
            ratio = difflib.SequenceMatcher(None, k, normalized).ratio()
            if ratio >= 0.7:
                return True, f"'{k}'(sim={ratio:.2f})"
            
            # 긴 단어(NORMAL, MAXIMUM)의 경우 텍스트 내 특정 단어와 유사도 체크
            if len(normalized) >= 3:
                for word in normalized.split():
                    w_ratio = difflib.SequenceMatcher(None, k, word).ratio()
                    if w_ratio >= 0.7:
                        return True, f"'{k}'(sim={w_ratio:.2f} in '{word}')"

        return False, ""

    @staticmethod
    def _majority(history: deque) -> Optional[str]:
        counts: dict[str, int] = {}
        for v in history:
            if v is not None:
                counts[v] = counts.get(v, 0) + 1
        if not counts:
            return None
        return max(counts, key=lambda k: counts[k])

    # ------------------------------------------------------------------
    # Rate OCR + RecordDB 저장
    # ------------------------------------------------------------------

    async def _try_record_rate(self, full_frame: np.ndarray, now: float, raw_mode: Optional[str], raw_diff: Optional[str]):
        """
        현재 확정된 song_id / mode / diff 조합의 Rate를 OCR로 읽어 RecordDB에 저장.
        - 세 값 중 하나라도 없으면 skip
        - 현재 프레임의 raw_mode/diff가 stable한 값과 다르면 skip (전환 중인 프레임 방지)
        - 같은 조합은 RATE_OCR_INTERVAL 이내 재시도 안 함
        """
        song_key = self._last_song_key
        mode     = self._last_mode
        diff     = self._last_diff

        if not song_key.startswith("jacket::") or not mode or not diff:
            return

        try:
            song_id = int(song_key.split("::")[1])
        except (IndexError, ValueError):
            return

        # 동기화 및 검증 체크
        if raw_mode != mode or raw_diff != diff or not self._diff_verified:
            # 잦은 로그 방지 (선택이 안정될 때까지 skip)
            return

        rate_key = f"{song_id}:{mode}:{diff}"
        if rate_key == self._last_rate_key and (now - self._last_rate_ocr_ts) < RATE_OCR_INTERVAL:
            return

        self._last_rate_ocr_ts = now
        self._last_rate_key    = rate_key

        # Rate 영역 크롭 (해상도 대응)
        h, w = full_frame.shape[:2]
        sx, sy = w / 1920.0, h / 1080.0
        x1 = int(_RATE_X1 * sx)
        y1 = int(_RATE_Y1 * sy)
        x2 = int(_RATE_X2 * sx)
        y2 = int(_RATE_Y2 * sy)
        roi_bgra = full_frame[y1:y2, x1:x2]

        text = await self._ocr_windows(roi_bgra)
        rate = self._parse_rate(text)

        if rate is None:
            if not text:
                self.log(f"Rate OCR 빈 결과 ({song_id} {mode}/{diff}) - 이진화 재시도")
                text = await self._ocr_windows(roi_bgra, force_invert=True)
                rate = self._parse_rate(text)
            if rate is None:
                self.log(f"Rate OCR 파싱 실패: '{text}' ({song_id} {mode}/{diff})")
                return

        self.log(f"Rate OCR: {song_id} {mode}/{diff} = {rate:.2f}% (raw='{text}')")

        if rate == 0.0:
            self.log(f"Rate 0.00% - 미플레이로 간주, 저장 skip")
            return

        if self.record_db is not None and self.record_db.is_ready:
            if self.record_db.upsert(song_id, mode, diff, rate):
                if self.on_record_updated:
                    self.on_record_updated()

    @staticmethod
    def _parse_rate(text: str) -> Optional[float]:
        """OCR 결과에서 Rate(%) 수치 추출."""
        text = text.strip()
        # 정상 케이스: 숫자.숫자%  ex) 99.04%, 100.00%, 0.00%
        m = re.search(r"(\d{1,3})\s*[.,]\s*(\d{2})\s*%?", text)
        if m:
            val = float(f"{m.group(1)}.{m.group(2)}")
            if 0.0 <= val <= 100.0:
                return val
        # 소수점 없이 붙은 경우 ex) 9904 → 99.04
        m = re.search(r"\b(\d{4,5})\b", text)
        if m:
            raw = m.group(1)
            val = float(f"{raw[:2]}.{raw[2:]}") if len(raw) == 4 else float(f"{raw[:3]}.{raw[3:]}")
            if 0.0 <= val <= 100.0:
                return val
        return None

    # ------------------------------------------------------------------
    # OCR fallback (현재 비활성)
    # ------------------------------------------------------------------

    async def _ocr_fallback(self, sct, rect: WindowRect):
        return

    # ------------------------------------------------------------------
    # ROI 헬퍼
    # ------------------------------------------------------------------

    def _region_from_ratio(self, rect, x_start, x_end, y_start, y_end) -> dict:
        return {
            "top":    rect.top  + int(rect.height * y_start),
            "left":   rect.left + int(rect.width  * x_start),
            "width":  max(1, int(rect.width  * (x_end - x_start))),
            "height": max(1, int(rect.height * (y_end - y_start))),
        }

    # ------------------------------------------------------------------
    # 텍스트 정규화 (OCR fallback용, 현재 미사용)
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
        return re.sub(r"\s+", " ", lines[0]).strip()

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
        left_score  = self._score_title(left_title,  prefer_right=False)
        right_score = self._score_title(right_title, prefer_right=True)
        return right_title if right_score >= left_score else left_title

    # ------------------------------------------------------------------
    # 선곡화면 감지
    # ------------------------------------------------------------------

    async def _detect_song_select(self, sct, rect: WindowRect) -> tuple[bool, bool]:
        logo_now = await self._detect_freestyle_logo(sct, rect)
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

        self.log(
            f"선곡판정 버퍼: hit={hit_count}/{sample_count} "
            f"(ratio={ratio:.2f}) -> {'선곡' if is_song_select else '기타'}"
            + (f" [이탈중]" if is_leaving else "")
        )
        return is_song_select, is_leaving

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
            text       = await self._ocr_windows(logo_img)
            normalized = re.sub(r"[^A-Z0-9]", "", text.upper())
            keyword    = re.sub(r"[^A-Z0-9]", "", LOGO_OCR_KEYWORD.upper())
            is_detected = False

            if keyword and normalized:
                if keyword in normalized:
                    is_detected = True
                else:
                    min_partial_len = min(6, len(keyword))
                    for i in range(0, len(keyword) - min_partial_len + 1):
                        part = keyword[i : i + min_partial_len]
                        if part and part in normalized:
                            is_detected = True
                            break
                    if not is_detected:
                        ratio = difflib.SequenceMatcher(None, keyword, normalized).ratio()
                        is_detected = ratio >= 0.72

            self._last_logo_ocr_ok = is_detected
            self._last_logo_ocr_ts = now
            self.log(f"로고 OCR: '{text}' (norm='{normalized}') -> {self._last_logo_ocr_ok}")
        return self._last_logo_ocr_ok

    # ------------------------------------------------------------------
    # Windows OCR
    # ------------------------------------------------------------------

    async def _ocr_windows(self, img_bgra: np.ndarray, force_invert: bool = False) -> str:
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
            # force_invert=True면 자동 판단과 반대 방향으로 이진화
            normal_is_dark = bg_mean < 128
            use_invert = normal_is_dark if force_invert else not normal_is_dark
            if not use_invert:
                _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY | cv2.THRESH_OTSU)
            else:
                _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY_INV | cv2.THRESH_OTSU)

            # 여백(Padding) 추가: Windows OCR은 글자가 가장자리에 붙어 있으면 인식률이 급감함
            padding = 10
            thresh = cv2.copyMakeBorder(
                thresh, padding, padding, padding, padding,
                cv2.BORDER_CONSTANT, value=0 # 검은색 배경 추가
            )

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
            
            # 명시적 리소스 해제 권장
            stream.close()
            
            return result.text.strip()
        except Exception as e:
            self.log(f"OCR 실행 오류: {e}")
            return ""
