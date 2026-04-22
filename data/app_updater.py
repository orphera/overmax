"""
app_updater.py - GitHub Releases 기반 앱 자동 패치

앱 시작 시 최신 릴리즈를 확인하고, 신규 버전이 있으면 overmax.zip을
다운로드/압축해제한 뒤 별도 워커 프로세스로 파일 교체를 수행한다.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable, Optional

import httpx

GITHUB_API_LATEST = "https://api.github.com/repos/{owner}/{repo}/releases/latest"

_DEFAULT_TIMEOUT = 8.0
_DOWNLOAD_TIMEOUT = 120.0
_UPDATE_WAIT_TIMEOUT = 120.0
_MANIFEST_NAME = "release_manifest.json"
_RESULT_FILENAME = "update_result.txt"
_APPLIED_TAG_FILENAME = "app_update_applied_tag.txt"
_WORKER_EXE_NAME = "overmax_updater_worker.exe"
_NO_CACHE_HEADERS = {
    "Cache-Control": "no-cache",
    "Pragma": "no-cache",
}


class AppUpdateError(RuntimeError):
    """앱 자동패치 중 치명적인 오류."""


class _UpdateStatusReporter:
    def __init__(self):
        self._app = None
        self._window = None
        self._label = None
        self._available = False

    def start(self, message: str):
        if not self._ensure_window():
            return
        self.update(message)
        self._window.show()
        self._window.raise_()
        self._window.activateWindow()
        self.pump(120)

    def update(self, message: str):
        if not self._available:
            return
        self._label.setText(message)
        self._window.adjustSize()
        self.pump(80)

    def close(self):
        if not self._available:
            return
        try:
            self._window.close()
            self.pump(80)
        except Exception:
            pass

    def pump(self, millis: int = 30):
        if not self._available:
            return
        deadline = time.time() + max(0, millis) / 1000.0
        while time.time() < deadline:
            self._app.processEvents()
            time.sleep(0.01)

    def _ensure_window(self) -> bool:
        if self._available:
            return True
        if os.name != "nt":
            return False
        try:
            from PyQt6.QtCore import Qt
            from PyQt6.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget
        except Exception:
            return False

        self._app = QApplication.instance() or QApplication([])
        self._window = QWidget()
        self._window.setWindowTitle("Overmax Update")
        self._window.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, True)
        self._window.setWindowFlag(Qt.WindowType.WindowCloseButtonHint, False)
        layout = QVBoxLayout(self._window)
        layout.setContentsMargins(18, 14, 18, 14)
        self._label = QLabel("")
        self._label.setWordWrap(True)
        layout.addWidget(self._label)
        self._window.setMinimumWidth(360)
        self._available = True
        return True


def check_and_apply_update(
    owner: str,
    repo: str,
    asset_name: str,
    current_version: str,
    app_dir: Path,
    latest_release_url: Optional[str] = None,
    fail_on_update_error: bool = False,
    log: Optional[Callable[[str], None]] = None,
    ask_before_update: Optional[Callable[[str, str], bool]] = None,
) -> bool:
    """최신 릴리즈 확인 후 업데이트를 시작한다."""
    _log = log or print
    latest_tag, asset_url, manifest_url = _fetch_latest_release_info(
        owner=owner,
        repo=repo,
        asset_name=asset_name,
        latest_release_url=latest_release_url,
        log=_log,
    )
    if not latest_tag or not asset_url:
        return True

    if not _is_newer_version(latest_tag, current_version):
        _log(f"[AppUpdater] 최신 버전 유지 중: {current_version}")
        return True

    if _should_skip_repeated_tag(app_dir, latest_tag, current_version):
        _log(
            "[AppUpdater] 동일 태그 업데이트가 이미 적용된 상태로 판단되어 재시도를 건너뜁니다: "
            f"{latest_tag} (현재 v{current_version})"
        )
        return True

    _log(f"[AppUpdater] 새 버전 감지: {current_version} -> {latest_tag}")
    if ask_before_update and not ask_before_update(current_version, latest_tag):
        _log("[AppUpdater] 사용자가 이번 실행의 자동 패치를 취소했습니다.")
        return True

    zip_path, stage_dir = _prepare_update_paths(app_dir, asset_name)
    if not _download_and_verify(asset_url, manifest_url, asset_name, zip_path, _log):
        return _handle_update_failure(fail_on_update_error, "자동 패치 파일 다운로드/검증에 실패했습니다.", _log)

    if not _extract_zip(zip_path, stage_dir, _log):
        return _handle_update_failure(fail_on_update_error, "자동 패치 압축 해제에 실패했습니다.", _log)

    payload_dir = _resolve_payload_dir(stage_dir)
    if payload_dir is None:
        return _handle_update_failure(fail_on_update_error, "자동 패치 결과물에서 overmax.exe를 찾지 못했습니다.", _log)

    if not _launch_update_worker(app_dir, payload_dir, current_version, latest_tag, _log):
        return _handle_update_failure(fail_on_update_error, "자동 패치 워커 실행에 실패했습니다.", _log)

    _log("[AppUpdater] 업데이트 워커를 시작했습니다. 현재 앱을 종료합니다.")
    return False


def consume_update_result(app_dir: Path) -> Optional[dict[str, str]]:
    result = _read_update_result(_result_path(app_dir))
    if not result:
        return None
    if result.get("status") != "started":
        _result_path(app_dir).unlink(missing_ok=True)
    return result


def peek_update_result(app_dir: Path) -> Optional[dict[str, str]]:
    return _read_update_result(_result_path(app_dir))


def _read_update_result(path: Path) -> Optional[dict[str, str]]:
    if not path.exists():
        return None
    try:
        raw = path.read_text(encoding="utf-8")
    except Exception:
        path.unlink(missing_ok=True)
        return None

    result: dict[str, str] = {}
    for line in raw.splitlines():
        line = line.strip()
        if not line or "=" not in line:
            continue
        key, value = line.split("=", 1)
        result[key.strip()] = value.strip()

    return result or None


def cleanup_update_artifacts(app_dir: Path):
    stage_dir = app_dir / "cache" / "update" / "stage"
    try:
        if stage_dir.exists():
            shutil.rmtree(stage_dir)
    except Exception:
        pass


def run_update_worker(argv: list[str], log: Optional[Callable[[str], None]] = None) -> int:
    _log = log or print
    args = _parse_worker_args(argv)
    app_dir = Path(args.app_dir).resolve()
    payload_dir = Path(args.payload_dir).resolve()
    result_path = _result_path(app_dir)
    status = _UpdateStatusReporter()
    status.start("업데이트를 시작합니다.")
    try:
        _write_result(result_path, status="started", from_ver=args.from_version, to_ver=args.to_version)
        status.update("Overmax 종료를 기다리는 중입니다...")
        if not _wait_for_process_exit(args.parent_pid, _UPDATE_WAIT_TIMEOUT, on_tick=status.pump):
            _write_result(result_path, status="failed", from_ver=args.from_version, to_ver=args.to_version, reason="wait_timeout")
            status.update("업데이트 실패: 앱 종료 대기 시간이 초과되었습니다.")
            status.pump(1200)
            return 1

        status.update("업데이트 파일을 적용하는 중입니다...")
        try:
            _apply_payload(app_dir, payload_dir)
        except Exception as e:
            _log(f"[AppUpdaterWorker] 복사 실패: {e}")
            _write_result(result_path, status="failed", from_ver=args.from_version, to_ver=args.to_version, reason="copy_failed")
            status.update("업데이트 실패: 파일 복사 중 오류가 발생했습니다.")
            status.pump(1200)
            return 1

        _write_result(result_path, status="success", from_ver=args.from_version, to_ver=args.to_version)
        _write_applied_tag(app_dir, args.to_version)
        status.update(
            f"업데이트 완료\n\n{args.from_version} -> {args.to_version}\n\n잠시 후 Overmax를 다시 실행합니다..."
        )
        status.pump(900)
        if not _restart_app(app_dir):
            _write_result(result_path, status="failed", from_ver=args.from_version, to_ver=args.to_version, reason="restart_failed")
            status.update("업데이트 완료 후 재실행에 실패했습니다.")
            status.pump(1200)
            return 1
        return 0
    finally:
        status.close()


def _parse_worker_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--update-worker", action="store_true")
    parser.add_argument("--parent-pid", type=int, required=True)
    parser.add_argument("--app-dir", required=True)
    parser.add_argument("--payload-dir", required=True)
    parser.add_argument("--from-version", required=True)
    parser.add_argument("--to-version", required=True)
    return parser.parse_args(argv)


def _apply_payload(app_dir: Path, payload_dir: Path):
    backup = app_dir / "cache" / "update" / "settings.pre_update.json"
    source_settings = app_dir / "settings.json"
    backup.parent.mkdir(parents=True, exist_ok=True)
    if source_settings.exists():
        shutil.copy2(source_settings, backup)

    _copy_tree(payload_dir, app_dir)

    if backup.exists():
        shutil.copy2(backup, source_settings)


def _copy_tree(src_root: Path, dst_root: Path):
    for src in src_root.rglob("*"):
        if src.is_file() and src.name == _WORKER_EXE_NAME:
            continue
        rel = src.relative_to(src_root)
        dst = dst_root / rel
        if src.is_dir():
            dst.mkdir(parents=True, exist_ok=True)
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)


def _restart_app(app_dir: Path) -> bool:
    exe = app_dir / "overmax.exe"
    try:
        flags = _windows_detached_flags()
        subprocess.Popen([str(exe)], close_fds=True, creationflags=flags)
        return True
    except Exception:
        return False


def _wait_for_process_exit(
    pid: int,
    timeout_sec: float,
    on_tick: Optional[Callable[[int], None]] = None,
) -> bool:
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        if on_tick:
            try:
                on_tick(50)
            except Exception:
                pass
        if not _is_process_running(pid):
            return True
        time.sleep(1.0)
    return False


def _is_process_running(pid: int) -> bool:
    try:
        if os.name == "nt":
            cmd = ["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"]
            out = subprocess.run(cmd, capture_output=True, text=True, check=False)
            return str(pid) in out.stdout and "No tasks" not in out.stdout
        os.kill(pid, 0)
        return True
    except Exception:
        return False


def _launch_update_worker(
    app_dir: Path,
    payload_dir: Path,
    current_version: str,
    latest_tag: str,
    log: Callable[[str], None],
) -> bool:
    try:
        base = _build_worker_base(app_dir)
    except Exception as e:
        log(f"[AppUpdater] 워커 베이스 준비 실패: {e}")
        return False

    cmd = _build_worker_command(base, app_dir, payload_dir, current_version, latest_tag)
    try:
        subprocess.Popen(
            cmd,
            close_fds=True,
            creationflags=_windows_detached_flags(),
        )
        return True
    except Exception as e:
        log(f"[AppUpdater] 워커 실행 실패: {e}")
        return False


def _build_worker_base(app_dir: Path) -> list[str]:
    if not getattr(sys, "frozen", False):
        main_path = Path(__file__).resolve().parents[1] / "main.py"
        return [str(Path(sys.executable).resolve()), str(main_path)]

    source_exe = Path(sys.executable).resolve()
    worker_dir = app_dir / "cache" / "update" / "stage"
    worker_dir.mkdir(parents=True, exist_ok=True)
    worker_exe = worker_dir / _WORKER_EXE_NAME
    shutil.copy2(source_exe, worker_exe)
    return [str(worker_exe)]


def _build_worker_command(
    base: list[str],
    app_dir: Path,
    payload_dir: Path,
    current_version: str,
    latest_tag: str,
) -> list[str]:
    return base + [
        "--update-worker",
        "--parent-pid",
        str(os.getpid()),
        "--app-dir",
        str(app_dir.resolve()),
        "--payload-dir",
        str(payload_dir.resolve()),
        "--from-version",
        f"v{current_version}",
        "--to-version",
        latest_tag,
    ]


def _windows_detached_flags() -> int:
    if os.name != "nt":
        return 0
    create_no_window = 0x08000000
    detached_process = 0x00000008
    return create_no_window | detached_process


def _prepare_update_paths(app_dir: Path, asset_name: str) -> tuple[Path, Path]:
    root = app_dir / "cache" / "update"
    return root / asset_name, root / "stage"


def _result_path(app_dir: Path) -> Path:
    return app_dir / "cache" / "update" / _RESULT_FILENAME


def _applied_tag_path(app_dir: Path) -> Path:
    return app_dir / "cache" / "update" / _APPLIED_TAG_FILENAME


def _read_applied_tag(app_dir: Path) -> Optional[str]:
    path = _applied_tag_path(app_dir)
    try:
        value = path.read_text(encoding="utf-8").strip()
        return value or None
    except Exception:
        return None


def _write_applied_tag(app_dir: Path, tag: str):
    path = _applied_tag_path(app_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(tag.strip(), encoding="utf-8")


def _should_skip_repeated_tag(app_dir: Path, latest_tag: str, current_version: str) -> bool:
    applied = _read_applied_tag(app_dir)
    if not applied:
        return False
    if applied != latest_tag:
        return False
    return _is_newer_version(latest_tag, current_version)


def is_newer_version(remote_tag: str, local_version: str) -> bool:
    return _is_newer_version(remote_tag, local_version)


def _write_result(
    path: Path,
    status: str,
    from_ver: str,
    to_ver: str,
    reason: str = "",
):
    lines = [
        f"status={status}",
        f"from={from_ver}",
        f"to={to_ver}",
    ]
    if reason:
        lines.append(f"reason={reason}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _handle_update_failure(strict: bool, message: str, log: Callable[[str], None]) -> bool:
    log(f"[AppUpdater] {message}")
    if strict:
        raise AppUpdateError(message)
    return True


def _download_and_verify(
    asset_url: str,
    manifest_url: Optional[str],
    asset_name: str,
    zip_path: Path,
    log: Callable[[str], None],
) -> bool:
    if not _download_file(asset_url, zip_path, log):
        return False
    if not manifest_url:
        return True
    try:
        _verify_with_manifest(manifest_url, asset_name, zip_path, log)
        return True
    except Exception as e:
        log(f"[AppUpdater] 해시 검증 실패 (업데이트 중단): {e}")
        return False


def _fetch_latest_release_info(
    owner: str,
    repo: str,
    asset_name: str,
    latest_release_url: Optional[str],
    log: Callable[[str], None],
) -> tuple[Optional[str], Optional[str], Optional[str]]:
    url = _build_latest_release_url(owner, repo, latest_release_url)
    try:
        resp = httpx.get(url, timeout=_DEFAULT_TIMEOUT, follow_redirects=True, headers=_NO_CACHE_HEADERS)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        log(f"[AppUpdater] 릴리즈 정보 조회 실패: {e}")
        return None, None, None

    tag = data.get("tag_name")
    if not tag:
        log("[AppUpdater] tag_name 없음")
        return None, None, None

    asset_url = None
    manifest_url = None
    for asset in data.get("assets", []):
        name = asset.get("name")
        if name == asset_name:
            asset_url = asset.get("browser_download_url")
        elif name == _MANIFEST_NAME:
            manifest_url = asset.get("browser_download_url")

    if not asset_url:
        log(f"[AppUpdater] 릴리즈에서 '{asset_name}' asset을 찾을 수 없음")
        return None, None, None

    return tag, asset_url, manifest_url


def _build_latest_release_url(owner: str, repo: str, override: Optional[str]) -> str:
    if override:
        return override
    return GITHUB_API_LATEST.format(owner=owner, repo=repo)


def _is_newer_version(remote_tag: str, local_version: str) -> bool:
    remote = _parse_version(remote_tag)
    local = _parse_version(local_version)
    if remote is None or local is None:
        return remote_tag.strip().lower() != f"v{local_version.strip().lower()}"
    return remote > local


def _parse_version(version_text: str) -> Optional[tuple[int, ...]]:
    cleaned = version_text.strip().lower()
    if cleaned.startswith("v"):
        cleaned = cleaned[1:]
    parts = cleaned.split(".")
    values: list[int] = []
    for part in parts:
        if not part.isdigit():
            return None
        values.append(int(part))
    return tuple(values)


def _download_file(url: str, dest: Path, log: Callable[[str], None]) -> bool:
    tmp = dest.with_suffix(dest.suffix + ".tmp")
    dest.parent.mkdir(parents=True, exist_ok=True)
    try:
        with httpx.stream("GET", url, timeout=_DOWNLOAD_TIMEOUT, follow_redirects=True, headers=_NO_CACHE_HEADERS) as resp:
            resp.raise_for_status()
            with open(tmp, "wb") as f:
                for chunk in resp.iter_bytes(chunk_size=1024 * 256):
                    f.write(chunk)
        shutil.move(str(tmp), str(dest))
        log(f"[AppUpdater] 다운로드 완료: {dest.name}")
        return True
    except Exception as e:
        log(f"[AppUpdater] 다운로드 실패: {e}")
        tmp.unlink(missing_ok=True)
        return False


def _verify_with_manifest(
    manifest_url: str,
    asset_name: str,
    zip_path: Path,
    log: Callable[[str], None],
):
    try:
        resp = httpx.get(manifest_url, timeout=_DEFAULT_TIMEOUT, follow_redirects=True, headers=_NO_CACHE_HEADERS)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        log(f"[AppUpdater] 매니페스트 조회 실패 (검증 생략): {e}")
        return

    expected = _extract_expected_sha256(data, asset_name)
    if not expected:
        return

    actual = _sha256_of_file(zip_path)
    if actual != expected:
        raise RuntimeError("release_manifest.json sha256 불일치")
    log("[AppUpdater] 해시 검증 완료")


def _extract_expected_sha256(manifest: dict, asset_name: str) -> Optional[str]:
    assets = manifest.get("assets")
    if not isinstance(assets, list):
        return None
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        if asset.get("name") != asset_name:
            continue
        sha = asset.get("sha256")
        if isinstance(sha, str) and sha:
            return sha.lower()
    return None


def _sha256_of_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest().lower()


def _extract_zip(zip_path: Path, stage_dir: Path, log: Callable[[str], None]) -> bool:
    try:
        if stage_dir.exists():
            shutil.rmtree(stage_dir)
        shutil.unpack_archive(str(zip_path), str(stage_dir))
        return True
    except Exception as e:
        log(f"[AppUpdater] 압축 해제 실패: {e}")
        return False


def _resolve_payload_dir(stage_dir: Path) -> Optional[Path]:
    if (stage_dir / "overmax.exe").exists():
        return stage_dir

    children = [p for p in stage_dir.iterdir() if p.is_dir()]
    for child in children:
        if (child / "overmax.exe").exists():
            return child

    for child in children:
        nested = child / "overmax"
        if (nested / "overmax.exe").exists():
            return nested

    return None
