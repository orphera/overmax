# coding: utf-8
"""
Google Sheets 메타 정보를 가져와 캐시하는 모듈.

4B/5B/6B/8B 탭의 패턴 메타(황배 여부, 비고, 보조 키 여부)를 파싱하여
곡명/버튼모드/난이도 키로 조회할 수 있게 저장합니다.
"""

import csv
import io
import json
import re
import time
from pathlib import Path
from urllib.request import urlopen

SHEET_ID = "1ks1dwJyNjkAXYtQ_6UZIeNOCGOmhf2jMbakpTcJm9rw"
SHEET_GIDS = {
    "4B": "979055934",
    "5B": "112529029",
    "6B": "2010625608",
    "8B": "1833696991",
}

# 캐시 파일 경로와 유지 시간(초)
CACHE_PATH = Path("cache/pattern_meta.json")
CACHE_TTL = 60 * 60 * 24  # 1일


def _norm(value: str) -> str:
    """공백 제거 후 소문자로 정규화"""
    return re.sub(r"\s+", "", str(value or "").lower().strip())


def _csv_url(gid: str) -> str:
    """지정된 gid의 CSV export URL 생성"""
    return (
        f"https://docs.google.com/spreadsheets/d/{SHEET_ID}/gviz/tq"
        f"?tqx=out:csv&gid={gid}"
    )


class PatternSheetMeta:
    """
    Google Sheets에서 패턴 메타 정보를 로드하여 저장/조회하는 클래스.
    """

    def __init__(self):
        self.items: dict[str, dict] = {}

    def load(self, force: bool = False) -> None:
        """
        메타 정보를 로드한다.
        force가 False이면 캐시가 유효할 때 캐시를 사용한다.
        """
        if not force and CACHE_PATH.exists():
            try:
                age = time.time() - CACHE_PATH.stat().st_mtime
                if age < CACHE_TTL:
                    self.items = json.loads(CACHE_PATH.read_text(encoding="utf-8"))
                    return
            except Exception:
                # 캐시가 손상되었을 수 있으므로 무시하고 새로 다운로드
                pass

        items: dict[str, dict] = {}
        for mode, gid in SHEET_GIDS.items():
            try:
                with urlopen(_csv_url(gid), timeout=10) as resp:
                    text = resp.read().decode("utf-8-sig")
            except Exception as e:
                print(f"[PatternSheetMeta] 시트 다운로드 실패({mode}): {e}")
                continue

            rows = csv.DictReader(io.StringIO(text))
            for row in rows:
                title = row.get("곡명", "") or row.get("Title", "")
                diff = row.get("난이도", "") or row.get("Diff", "")
                if not title or not diff:
                    continue

                key = f"{mode}|{_norm(title)}|{_norm(diff)}"
                meta = {
                    "gold": str(row.get("황배 여부", "") or row.get("황배여부", "")).strip(),
                    "note": str(row.get("비고", "") or row.get("Note", "")).strip(),
                }

                # 5B 탭만 보조 키 컬럼이 존재
                if mode == "5B":
                    meta["assist_key"] = str(row.get("보조 키 여부", "") or row.get("보조키여부", "")).strip()

                items[key] = meta

        # 캐시 파일 저장
        try:
            CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
            CACHE_PATH.write_text(json.dumps(items, ensure_ascii=False, indent=2), encoding="utf-8")
        except Exception as e:
            print(f"[PatternSheetMeta] 캐시 저장 실패: {e}")

        self.items = items

    def get(self, song_name: str, mode: str, diff: str) -> dict:
        """
        곡명/버튼모드/난이도 키에 해당하는 메타 정보를 반환한다.
        """
        return self.items.get(f"{mode}|{_norm(song_name)}|{_norm(diff)}", {})