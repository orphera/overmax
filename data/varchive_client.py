"""
V-Archive 유저 기록 API 클라이언트
https://github.com/djmax-in/openapi/wiki/유저-기록-조회-API-V2
"""

import json
from pathlib import Path
from typing import Optional

import httpx

from constants import CACHE_PATH


class VArchiveRecordClient:
    BASE_URL = "https://v-archive.net/api/v2/archive/{nick}/button/{btn}"

    def __init__(self, cache_dir: str = "cache/varchive"):
        self.cache_dir = Path(cache_dir)
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def fetch_records(self, v_id: str, button: int) -> Optional[dict]:
        """
        V-Archive API에서 특정 버튼 모드의 유저 기록을 가져온다.
        button: 4, 5, 6, 8
        """
        url = self.BASE_URL.format(nick=v_id, btn=button)
        try:
            print(f"[VArchiveClient] Fetching: {url}")
            resp = httpx.get(url, timeout=10.0)
            resp.raise_for_status()
            return resp.json()
        except Exception as e:
            print(f"[VArchiveClient] API 요청 실패 ({url}): {e}")
            return None

    def save_to_cache(self, steam_id: str, v_id: str, button: int, data: dict):
        """기록 데이터를 로컬 파일로 캐시한다."""
        user_dir = self.cache_dir / steam_id
        user_dir.mkdir(parents=True, exist_ok=True)

        cache_file = user_dir / f"{button}.json"
        
        # 메타데이터 포함해서 저장
        cache_data = {
            "v_id": v_id,
            "button": button,
            "records": data.get("records", []),
            "updated_at": data.get("user", {}).get("updated_at") # 또는 현재 시간
        }

        with open(cache_file, "w", encoding="utf-8") as f:
            json.dump(cache_data, f, ensure_ascii=False, indent=2)
        
        print(f"[VArchiveClient] 캐시 저장 완료: {cache_file}")

    def load_cached_records(self, steam_id: str, button: int) -> list[dict]:
        """캐시된 기록을 로드한다."""
        cache_file = self.cache_dir / steam_id / f"{button}.json"
        if not cache_file.exists():
            return []

        try:
            with open(cache_file, "r", encoding="utf-8") as f:
                data = json.load(f)
            return data.get("records", [])
        except Exception as e:
            print(f"[VArchiveClient] 캐시 로드 실패 ({cache_file}): {e}")
            return []
