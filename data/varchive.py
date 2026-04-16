"""
V-Archive 데이터 관리
songs.json을 로컬 캐시로 사용하거나 API에서 최신 데이터를 가져옴
"""

import json
import re
import time
from pathlib import Path
from typing import Optional

from constants import (
    SONGS_API_URL,
    CACHE_PATH,
    CACHE_TTL,
    DOWNLOAD_TIMEOUT,
    FUZZY_THRESHOLD,
    BUTTON_MODES,
    DIFFICULTIES,
    DIFF_COLORS,
)

try:
    import httpx
    HTTPX_AVAILABLE = True
except ImportError:
    HTTPX_AVAILABLE = False

try:
    from rapidfuzz import process, fuzz
    RAPIDFUZZ_AVAILABLE = True
except ImportError:
    RAPIDFUZZ_AVAILABLE = False
    import difflib


class VArchiveDB:
    def __init__(self):
        self.songs: list[dict] = []
        self._title_map: dict[str, list[dict]] = {}  # 곡명 소문자 → song list

    # ------------------------------------------------------------------
    # 로드 / 캐시
    # ------------------------------------------------------------------

    def load(self, local_path: Optional[str] = None):
        """
        1) local_path 지정 시 그 파일 우선 사용
        2) 캐시가 유효하면 캐시 사용
        3) 둘 다 없으면 API에서 다운로드
        """
        if local_path and Path(local_path).exists():
            self._load_file(local_path)
            return

        if self._cache_valid():
            self._load_file(CACHE_PATH)
            return

        self._download_and_cache()

    def _load_file(self, path):
        with open(path, encoding="utf-8") as f:
            self.songs = json.load(f)
        self._build_index()
        print(f"[VArchive] {len(self.songs)}곡 로드 완료 ({path})")

    def _cache_valid(self) -> bool:
        if not CACHE_PATH.exists():
            return False
        age = time.time() - CACHE_PATH.stat().st_mtime
        return age < CACHE_TTL

    def _download_and_cache(self):
        if not HTTPX_AVAILABLE:
            raise ImportError("httpx 미설치 - 'pip install httpx' 후 재시도")
        print("[VArchive] API에서 최신 데이터 다운로드 중...")
        try:
            resp = httpx.get(SONGS_API_URL, timeout=DOWNLOAD_TIMEOUT)
            resp.raise_for_status()
            CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
            CACHE_PATH.write_bytes(resp.content)
            self.songs = resp.json()
            self._build_index()
            print(f"[VArchive] {len(self.songs)}곡 다운로드 완료")
        except Exception as e:
            print(f"[VArchive] 다운로드 실패: {e}")
            raise

    def _build_index(self):
        """곡명 소문자 인덱스 구축 (동명이곡 대응)"""
        self._title_map = {}
        for song in self.songs:
            key = song["name"].lower().strip()
            self._title_map.setdefault(key, []).append(song)

    # ------------------------------------------------------------------
    # 검색
    # ------------------------------------------------------------------

    def _normalize_text(self, value: str) -> str:
        value = value.lower().strip()
        return re.sub(r"\s+", "", value)

    def _pick_by_composer(self, songs: list[dict], composer: str) -> Optional[dict]:
        if not songs:
            return None
        if not composer:
            return songs[0]

        query = self._normalize_text(composer)
        if not query:
            return songs[0]

        best_song = None
        best_score = -1.0
        for song in songs:
            song_comp = str(song.get("composer", ""))
            comp_norm = self._normalize_text(song_comp)
            score = 0.0
            if query == comp_norm:
                score = 200.0
            elif query and comp_norm and (query in comp_norm or comp_norm in query):
                score = 150.0
            elif RAPIDFUZZ_AVAILABLE:
                score = float(fuzz.WRatio(query, comp_norm))
            else:
                import difflib
                score = float(difflib.SequenceMatcher(None, query, comp_norm).ratio() * 100)

            if score > best_score:
                best_score = score
                best_song = song

        return best_song if best_song is not None else songs[0]

    def find_exact(self, title: str, composer: str = "") -> Optional[dict]:
        """정확한 곡명 검색 (대소문자 무시), 동명이곡은 composer로 분기"""
        songs = self._title_map.get(title.lower().strip())
        if not songs:
            return None
        return self._pick_by_composer(songs, composer)

    def find_fuzzy(
        self,
        title: str,
        composer: str = "",
        threshold: int = FUZZY_THRESHOLD,
    ) -> Optional[dict]:
        """
        퍼지 검색 - OCR 오인식 대응
        threshold: 0~100, 높을수록 엄격
        """
        if not self._title_map:
            return None

        candidates = list(self._title_map.keys())
        query = title.lower().strip()

        if RAPIDFUZZ_AVAILABLE:
            result = process.extractOne(
                query, candidates, scorer=fuzz.WRatio, score_cutoff=threshold
            )
            if result:
                matched_key, score, _ = result
                print(f"[VArchive] 퍼지매칭: '{title}' → '{matched_key}' (점수: {score})")
                return self._pick_by_composer(self._title_map[matched_key], composer)
        else:
            # difflib fallback
            matches = difflib.get_close_matches(query, candidates, n=1, cutoff=threshold / 100)
            if matches:
                print(f"[VArchive] 퍼지매칭(difflib): '{title}' → '{matches[0]}'")
                return self._pick_by_composer(self._title_map[matches[0]], composer)

        return None

    def search_by_id(self, song_id: int) -> Optional[dict]:
        """ID로 곡 검색"""
        for song in self.songs:
            if int(song.get("title")) == song_id:
                return song
        return None

    def search(self, title: str, composer: str = "") -> Optional[dict]:
        """정확 검색 → 퍼지 검색 순으로 시도 (composer로 동명이곡 분기)"""
        return (
            self.find_exact(title, composer=composer)
            or self.find_fuzzy(title, composer=composer, threshold=FUZZY_THRESHOLD)
        )

    # ------------------------------------------------------------------
    # 패턴 정보 포맷
    # ------------------------------------------------------------------

    def get_patterns(self, song: dict, button_mode: str) -> Optional[dict]:
        """특정 버튼 모드의 패턴 반환"""
        return song["patterns"].get(button_mode)

    def format_pattern_info(self, song: dict, button_mode: str) -> list[dict]:
        """
        오버레이 표시용 패턴 정보 리스트 반환
        반환 형식:
        [
            {
                "diff": "SC",
                "level": 15,
                "floor": 152,
                "floorName": "15.2",  # 없으면 None
                "rating": 185,        # 없으면 None
                "color": "#9B59B6"
            },
            ...
        ]
        """
        patterns = self.get_patterns(song, button_mode)
        if not patterns:
            return []

        result = []
        for diff in DIFFICULTIES:
            if diff not in patterns:
                continue
            info = patterns[diff]
            result.append({
                "diff": diff,
                "level": info.get("level"),
                "floor": info.get("floor"),
                "floorName": info.get("floorName"),
                "rating": info.get("rating"),
                "color": DIFF_COLORS.get(diff, "#FFFFFF"),
            })
        return result


# ------------------------------------------------------------------
# 간단한 테스트
# ------------------------------------------------------------------
if __name__ == "__main__":
    db = VArchiveDB()
    db.load(local_path="cache/songs.json")

    # 정확 검색
    song = db.search("Kamui")
    if song:
        print(f"\n곡명: {song['name']} / 작곡가: {song['composer']}")
        for mode in BUTTON_MODES:
            patterns = db.format_pattern_info(song, mode)
            if patterns:
                print(f"  [{mode}]")
                for p in patterns:
                    floor_str = f" → {p['floorName']}" if p['floorName'] else ""
                    print(f"    {p['diff']}: Lv.{p['level']}{floor_str}")
