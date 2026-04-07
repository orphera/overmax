"""
애플리케이션 설정 로더.
settings.json 이 있으면 병합하고, 없으면 기본값을 사용한다.
"""

from __future__ import annotations

import copy
import json
from pathlib import Path
from typing import Any

import runtime_patch


DEFAULT_SETTINGS: dict[str, Any] = {
    "window_tracker": {
        "window_title": "DJMAX RESPECT V",
        "poll_interval_sec": 0.5,
    },
    "screen_capture": {
        "list_x_start": 0.031,
        "list_x_end": 0.167,
        "sampling_x_ratio": 0.08,
        "logo_x_start": 0.070,
        "logo_x_end": 0.215,
        "logo_y_start": 0.022,
        "logo_y_end": 0.070,
        "logo_ocr_keyword": "FREESTYLE",
        "logo_ocr_cooldown_sec": 1.0,
        "left_title_x_start": 0.045,
        "left_title_x_end": 0.245,
        "left_title_y_start": 0.225,
        "left_title_y_end": 0.282,
        "right_title_x_start": 0.365,
        "right_title_x_end": 0.685,
        "right_title_y_start": 0.495,
        "right_title_y_end": 0.560,
        "right_title_pad_px": 6,
        "left_composer_x_start": 0.045,
        "left_composer_x_end": 0.300,
        "left_composer_y_start": 0.286,
        "left_composer_y_end": 0.330,
        "highlight_hue_min": 10,
        "highlight_hue_max": 32,
        "highlight_sat_min": 130,
        "highlight_val_min": 160,
        "highlight_row_min_px": 40,
        "highlight_row_max_px": 200,
        "highlight_row_threshold": 8,
        "ocr_interval_sec": 0.35,
        "idle_sleep_sec": 0.5,
    },
    "debug_window": {
        "max_lines": 500,
        "title": "Overmax Debug Log",
    },
    "overlay": {
        "toggle_hotkey": "F9",
        "tray_tooltip": "Overmax - DJMAX Respect V 난이도 오버레이",
        "hint_label": "F9: 표시/숨김  |  드래그로 위치 이동",
        "position_file": "overlay_position.json",
    },
    "varchive": {
        "songs_api_url": "https://v-archive.net/db/v2/songs.json",
        "cache_path": "cache/songs.json",
        "cache_ttl_sec": 86400,
        "download_timeout_sec": 10,
        "fuzzy_threshold": 80,
        "button_modes": ["4B", "5B", "6B", "8B"],
        "difficulties": ["NM", "HD", "MX", "SC"],
        "diff_colors": {
            "NM": "#4A90D9",
            "HD": "#F5A623",
            "MX": "#D0021B",
            "SC": "#9B59B6",
        },
    },
}


def _merge_dict(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    for key, value in override.items():
        if isinstance(base.get(key), dict) and isinstance(value, dict):
            _merge_dict(base[key], value)
        else:
            base[key] = value
    return base


def _settings_candidates() -> list[Path]:
    data_path = runtime_patch.get_data_dir() / "settings.json"
    local_path = Path(__file__).parent / "settings.json"
    if data_path == local_path:
        return [data_path]
    return [data_path, local_path]


def _load_settings_file() -> tuple[dict[str, Any], Path | None]:
    for path in _settings_candidates():
        if not path.exists():
            continue
        try:
            with open(path, encoding="utf-8") as f:
                loaded = json.load(f)
            if isinstance(loaded, dict):
                return loaded, path
            print(f"[Settings] 잘못된 형식(객체 필요): {path}")
        except Exception as e:
            print(f"[Settings] 로드 실패 ({path}): {e}")
    return {}, None


_raw_settings, SETTINGS_PATH = _load_settings_file()
SETTINGS: dict[str, Any] = _merge_dict(copy.deepcopy(DEFAULT_SETTINGS), _raw_settings)
