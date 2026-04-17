"""
중앙 상수 관리 모듈.
모든 설정 기반 상수와 하드코딩된 물리적 좌표/비율을 여기서 관리한다.
"""

from pathlib import Path
from settings import SETTINGS
import runtime_patch

# ------------------------------------------------------------------
# 윈도우 추적 및 기본 설정
# ------------------------------------------------------------------
REF_WIDTH = 1920
REF_HEIGHT = 1080
WINDOW_TRACKER_SETTINGS = SETTINGS["window_tracker"]
WINDOW_TITLE = str(WINDOW_TRACKER_SETTINGS["window_title"])
POLL_INTERVAL = float(WINDOW_TRACKER_SETTINGS["poll_interval_sec"])

# ------------------------------------------------------------------
# V-Archive 관련
# ------------------------------------------------------------------
VARCHIVE_SETTINGS = SETTINGS["varchive"]
SONGS_API_URL = str(VARCHIVE_SETTINGS["songs_api_url"])
CACHE_PATH = runtime_patch.get_data_dir() / str(VARCHIVE_SETTINGS["cache_path"])
CACHE_TTL = int(VARCHIVE_SETTINGS["cache_ttl_sec"])
DOWNLOAD_TIMEOUT = float(VARCHIVE_SETTINGS["download_timeout_sec"])
FUZZY_THRESHOLD = int(VARCHIVE_SETTINGS["fuzzy_threshold"])

BUTTON_MODES = list(VARCHIVE_SETTINGS["button_modes"])
DIFFICULTIES = list(VARCHIVE_SETTINGS["difficulties"])
DIFF_COLORS = dict(VARCHIVE_SETTINGS["diff_colors"])

# ------------------------------------------------------------------
# 화면 캡처 및 인식 관련
# ------------------------------------------------------------------
SCREEN_CAPTURE_SETTINGS = SETTINGS["screen_capture"]
JACKET_SETTINGS = SETTINGS["jacket_matcher"]

OCR_INTERVAL = float(SCREEN_CAPTURE_SETTINGS["ocr_interval_sec"])
IDLE_SLEEP_INTERVAL = float(SCREEN_CAPTURE_SETTINGS["idle_sleep_sec"])

# 로고(FREESTYLE) ROI (비율)
LOGO_X_START = float(SCREEN_CAPTURE_SETTINGS["logo_x_start"])
LOGO_X_END = float(SCREEN_CAPTURE_SETTINGS["logo_x_end"])
LOGO_Y_START = float(SCREEN_CAPTURE_SETTINGS["logo_y_start"])
LOGO_Y_END = float(SCREEN_CAPTURE_SETTINGS["logo_y_end"])

LOGO_OCR_KEYWORD = str(SCREEN_CAPTURE_SETTINGS["logo_ocr_keyword"]).upper()
LOGO_OCR_COOLDOWN_SEC = float(SCREEN_CAPTURE_SETTINGS["logo_ocr_cooldown_sec"])

# 로고 인식 히스토리/판정
FREESTYLE_HISTORY_SIZE = int(SCREEN_CAPTURE_SETTINGS["freestyle_history_size"])
FREESTYLE_ON_RATIO = float(SCREEN_CAPTURE_SETTINGS["freestyle_on_ratio"])
FREESTYLE_ON_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS["freestyle_on_min_samples"])
FREESTYLE_OFF_RATIO = float(SCREEN_CAPTURE_SETTINGS["freestyle_off_ratio"])
FREESTYLE_OFF_MIN_SAMPLES = int(SCREEN_CAPTURE_SETTINGS["freestyle_off_min_samples"])

# 재킷 ROI (비율)
JACKET_X_START = float(JACKET_SETTINGS["jacket_x_start"])
JACKET_X_END = float(JACKET_SETTINGS["jacket_x_end"])
JACKET_Y_START = float(JACKET_SETTINGS["jacket_y_start"])
JACKET_Y_END = float(JACKET_SETTINGS["jacket_y_end"])

# 재킷 매칭
JACKET_MATCH_INTERVAL = float(JACKET_SETTINGS["match_interval_sec"])
JACKET_SIMILARITY_LOG = bool(JACKET_SETTINGS["log_similarity"])
JACKET_CHANGE_THRESHOLD = float(JACKET_SETTINGS["jacket_change_threshold"])
JACKET_FORCE_RECHECK_SEC = float(JACKET_SETTINGS["jacket_force_recheck_sec"])

# 상태 판정
_MODE_DIFF_SETTINGS = SETTINGS.get("mode_diff_detector", {})
MODE_DIFF_HISTORY = int(_MODE_DIFF_SETTINGS.get("history_size", 3))

# Rate OCR (1920x1080 기준 픽셀 좌표)
RATE_ROI = (176, 583, 270, 605)
RATE_OCR_INTERVAL = 1.5

# ------------------------------------------------------------------
# UI 내비게이션 및 상세 인식 영역 (1920x1080 기준 픽셀 좌표)
# ------------------------------------------------------------------

# 버튼 모드 감지 영역
BTN_MODE_ROI = (80, 130, 85, 135)

# 버튼 모드 대표색 (BGR)
BTN_COLORS: dict[str, list[tuple[int, int, int]]] = {
    "4B": [(0x55, 0x4F, 0x2D), (0x5A, 0x47, 0x0C)],   # #2D4F55 / #0C475A
    "5B": [(0xC6, 0xA9, 0x44)],                         # #44A9C6
    "6B": [(0x30, 0x94, 0xED)],                         # #ED9430
    "8B": [(0x31, 0x14, 0x1D)],                         # #1D1431
}
BTN_MODE_MAX_DIST = 60   # 이 이상이면 인식 실패로 간주

# 난이도 패널 ROI (NM 기준, 비율)
DIFF_PANEL_ROI = (98, 488, 208, 516)

# 난이도별 X 오프셋 (비율)
DIFF_X_OFFSETS: dict[str, float] = {
    "NM": 0,
    "HD": 120,
    "MX": 240,
    "SC": 360,
}

# 난이도 감지 임계값
DIFF_MIN_BRIGHTNESS   = 45.0   # 이하이면 UI 전환 중으로 간주
DIFF_CONFIDENT_MARGIN = 15.0   # 1위 패널이 2위보다 이 이상 밝아야 confident

# ------------------------------------------------------------------
# 오버레이 제어
# ------------------------------------------------------------------
OVERLAY_SETTINGS = SETTINGS["overlay"]
TOGGLE_HOTKEY = str(OVERLAY_SETTINGS["toggle_hotkey"])
TRAY_TOOLTIP = str(OVERLAY_SETTINGS["tray_tooltip"])
OVERLAY_POSITION_FILE = str(OVERLAY_SETTINGS["position_file"])
RECORD_DB_PATH = "cache/record.db"

# ------------------------------------------------------------------
# 추천 시스템
# ------------------------------------------------------------------
SC_GROUP = {"SC"}
NHM_GROUP = {"NM", "HD", "MX"}
