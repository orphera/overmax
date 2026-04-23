"""
애플리케이션 설정 로더.
DEFAULT -> settings.json -> settings.user.json 순서로 병합하여 최종 설정을 생성한다.
save_settings() 호출 시 기본값(DEFAULT + settings.json)과 차이가 있는 항목만 settings.user.json에 저장한다.
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
        "logo_ocr_keyword": "FREESTYLE",
        "logo_ocr_cooldown_sec": 1.0,
        "freestyle_history_size": 7,
        "freestyle_on_ratio": 0.60,
        "freestyle_on_min_samples": 3,
        "freestyle_off_ratio": 0.35,
        "freestyle_off_min_samples": 7,
        "ocr_interval_sec": 0.35,
        "idle_sleep_sec": 0.5,
    },
    "mode_diff_detector": {
        "history_size": 3,
    },
    "debug_window": {
        "max_lines": 500,
        "title": "Overmax Debug Log",
    },
    "overlay": {
        "toggle_hotkey": "F3",
        "tray_tooltip": "Overmax - DJMAX Respect V 난이도 오버레이",
        "hint_label": "F3: 표시/숨김  |  드래그로 위치 이동",
        "base_opacity": 0.8,
        "scale": 1.0,
        "position": {"x": 0, "y": 0},
    },
    "jacket_matcher": {
        "db_path": "cache/image_index.db",
        "similarity_threshold": 0.6,
        "match_interval_sec": 0.8,
        "jacket_change_threshold": 2.5,
        "jacket_force_recheck_sec": 2.0,
        "log_similarity": True,
    },
    "app_update": {
        "enabled": True,
        "owner": "orphera",
        "repo": "overmax",
        "asset_name": "overmax.zip",
        "latest_release_url": "",
    },
    "varchive": {
        "auto_refresh": False,
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


def _diff_dict(base: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    """base와 current를 비교하여 차이가 있는 항목만 반환한다 (재귀)."""
    diff = {}
    for key, val in current.items():
        if key not in base:
            diff[key] = copy.deepcopy(val)
        elif isinstance(base[key], dict) and isinstance(val, dict):
            sub_diff = _diff_dict(base[key], val)
            if sub_diff:
                diff[key] = sub_diff
        elif base[key] != val:
            diff[key] = copy.deepcopy(val)
    return diff


def _get_settings_paths() -> tuple[Path, Path]:
    data_dir = runtime_patch.get_data_dir()
    settings_path = data_dir / "settings.json"
    user_settings_path = data_dir / "settings.user.json"
    return settings_path, user_settings_path


def _load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        with open(path, encoding="utf-8") as f:
            loaded = json.load(f)
        return loaded if isinstance(loaded, dict) else {}
    except Exception as e:
        print(f"[Settings] 로드 실패 ({path}): {e}")
        return {}


def _init_settings() -> tuple[dict[str, Any], dict[str, Any], Path]:
    settings_path, user_settings_path = _get_settings_paths()

    # 1. Base = DEFAULT + settings.json
    base_settings = copy.deepcopy(DEFAULT_SETTINGS)
    _merge_dict(base_settings, _load_json(settings_path))

    # 2. Final = Base + settings.user.json
    final_settings = copy.deepcopy(base_settings)
    _merge_dict(final_settings, _load_json(user_settings_path))

    return final_settings, base_settings, user_settings_path


SETTINGS, BASE_SETTINGS, USER_SETTINGS_PATH = _init_settings()


def save_settings():
    """BASE_SETTINGS와 비교하여 변경된 부분만 settings.user.json에 저장한다."""
    try:
        user_diff = _diff_dict(BASE_SETTINGS, SETTINGS)

        USER_SETTINGS_PATH.parent.mkdir(parents=True, exist_ok=True)
        with open(USER_SETTINGS_PATH, "w", encoding="utf-8") as f:
            json.dump(user_diff, f, ensure_ascii=False, indent=2)
    except Exception as e:
        print(f"[Settings] 저장 실패: {e}")
