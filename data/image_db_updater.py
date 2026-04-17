"""
image_db_updater.py - GitHub Releases 기반 image_index.db 자동 업데이트

앱 시작 시 호출. 최신 릴리즈 태그와 로컬 버전을 비교해서
다를 때만 image_index.db를 다운로드한다.

실패는 항상 조용히 넘긴다 — DB 없이도 OCR 모드로 동작하는
기존 로직을 깨지 않기 위함.
"""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Optional, Callable

import httpx

# ------------------------------------------------------------------
# 설정
# ------------------------------------------------------------------

GITHUB_API_LATEST = "https://api.github.com/repos/{owner}/{repo}/releases/latest"
ASSET_FILENAME    = "image_index.db"
VERSION_FILENAME  = "image_db_version.txt"

_DEFAULT_TIMEOUT  = 10.0
_DOWNLOAD_TIMEOUT = 60.0   # DB 파일은 크므로 넉넉히


# ------------------------------------------------------------------
# 퍼블릭 인터페이스
# ------------------------------------------------------------------

def check_and_update(
    owner: str,
    repo: str,
    db_path: Path,
    log: Optional[Callable[[str], None]] = None,
) -> bool:
    """
    최신 릴리즈를 확인하고 필요하면 image_index.db를 다운로드한다.

    Args:
        owner:   GitHub 계정/조직명
        repo:    레포지터리명
        db_path: image_index.db 저장 경로 (예: Path("cache/image_index.db"))
        log:     로그 콜백 (없으면 print 사용)

    Returns:
        True  — 업데이트 성공 또는 이미 최신
        False — 실패 (네트워크 오류, asset 없음 등)
    """
    _log = log or print

    latest_tag, download_url = _fetch_latest_release_info(owner, repo, _log)
    if not latest_tag or not download_url:
        return False

    local_tag = _read_local_version(db_path)
    if local_tag == latest_tag and db_path.exists():
        _log(f"[ImageDBUpdater] 최신 버전 유지 중: {latest_tag}")
        return True

    _log(f"[ImageDBUpdater] 업데이트 시작: {local_tag or '없음'} → {latest_tag}")
    ok = _download_db(download_url, db_path, _log)
    if ok:
        _write_local_version(db_path, latest_tag)
        _log(f"[ImageDBUpdater] 업데이트 완료: {latest_tag}")
    return ok


# ------------------------------------------------------------------
# GitHub API
# ------------------------------------------------------------------

def _fetch_latest_release_info(
    owner: str,
    repo: str,
    log: Callable[[str], None],
) -> tuple[Optional[str], Optional[str]]:
    """최신 릴리즈의 (tag_name, asset download_url) 반환. 실패 시 (None, None)."""
    url = GITHUB_API_LATEST.format(owner=owner, repo=repo)
    try:
        resp = httpx.get(url, timeout=_DEFAULT_TIMEOUT, follow_redirects=True)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        log(f"[ImageDBUpdater] 릴리즈 정보 조회 실패: {e}")
        return None, None

    tag = data.get("tag_name")
    if not tag:
        log("[ImageDBUpdater] tag_name 없음")
        return None, None

    download_url = _find_asset_url(data.get("assets", []), log)
    return tag, download_url


def _find_asset_url(
    assets: list[dict],
    log: Callable[[str], None],
) -> Optional[str]:
    """assets 목록에서 ASSET_FILENAME에 해당하는 download URL 반환."""
    for asset in assets:
        if asset.get("name") == ASSET_FILENAME:
            return asset.get("browser_download_url")
    log(f"[ImageDBUpdater] 릴리즈에서 '{ASSET_FILENAME}' asset을 찾을 수 없음")
    return None


# ------------------------------------------------------------------
# 다운로드
# ------------------------------------------------------------------

def _download_db(
    url: str,
    dest: Path,
    log: Callable[[str], None],
) -> bool:
    """url에서 dest로 DB 파일을 다운로드한다. 실패 시 False 반환."""
    tmp = dest.with_suffix(".tmp")
    dest.parent.mkdir(parents=True, exist_ok=True)
    try:
        with httpx.stream("GET", url, timeout=_DOWNLOAD_TIMEOUT, follow_redirects=True) as resp:
            resp.raise_for_status()
            total = int(resp.headers.get("content-length", 0))
            received = 0
            with open(tmp, "wb") as f:
                for chunk in resp.iter_bytes(chunk_size=1024 * 64):
                    f.write(chunk)
                    received += len(chunk)
            if total and received != total:
                raise IOError(f"크기 불일치: {received} / {total} bytes")
        shutil.move(str(tmp), str(dest))
        log(f"[ImageDBUpdater] 다운로드 완료: {received:,} bytes → {dest}")
        return True
    except Exception as e:
        log(f"[ImageDBUpdater] 다운로드 실패: {e}")
        if tmp.exists():
            tmp.unlink(missing_ok=True)
        return False


# ------------------------------------------------------------------
# 로컬 버전 파일
# ------------------------------------------------------------------

def _version_file(db_path: Path) -> Path:
    return db_path.parent / VERSION_FILENAME


def _read_local_version(db_path: Path) -> Optional[str]:
    path = _version_file(db_path)
    try:
        return path.read_text(encoding="utf-8").strip() or None
    except FileNotFoundError:
        return None


def _write_local_version(db_path: Path, tag: str):
    _version_file(db_path).write_text(tag, encoding="utf-8")
