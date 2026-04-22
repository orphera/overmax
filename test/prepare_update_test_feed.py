"""
로컬 자동패치 테스트용 feed 생성 스크립트.

필수 입력:
- dist/overmax.zip
- dist/release_manifest.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--owner", default="orphera")
    parser.add_argument("--repo", default="overmax")
    parser.add_argument("--version", default="")
    parser.add_argument("--feed-dir", default="cache/update_test_feed")
    return parser.parse_args()


def read_app_version(version_file: Path) -> str:
    namespace: dict = {}
    code = version_file.read_text(encoding="utf-8")
    exec(code, namespace)
    version = str(namespace.get("APP_VERSION", "")).strip()
    if not version:
        raise RuntimeError("APP_VERSION을 읽을 수 없습니다.")
    return version


def next_patch_version(version: str) -> str:
    chunks = version.split(".")
    if len(chunks) < 3 or not all(c.isdigit() for c in chunks[:3]):
        raise RuntimeError(f"버전 형식이 잘못되었습니다: {version}")
    major, minor, patch = int(chunks[0]), int(chunks[1]), int(chunks[2])
    return f"{major}.{minor}.{patch + 1}"


def sha256_of(path: Path) -> str:
    hasher = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest().lower()


def main():
    args = parse_args()
    root = Path(__file__).resolve().parents[1]
    dist_dir = root / "dist"
    source_zip = dist_dir / "overmax.zip"
    source_manifest = dist_dir / "release_manifest.json"

    if not source_zip.exists() or not source_manifest.exists():
        raise RuntimeError("dist/overmax.zip 또는 dist/release_manifest.json 이 없습니다. 먼저 build.bat를 실행하세요.")

    current = read_app_version(root / "core" / "version.py")
    target_version = args.version.strip() or next_patch_version(current)
    feed_root = (root / args.feed_dir).resolve()

    if feed_root.exists():
        shutil.rmtree(feed_root)

    assets_dir = feed_root / "assets"
    api_dir = feed_root / "repos" / args.owner / args.repo / "releases"
    assets_dir.mkdir(parents=True, exist_ok=True)
    api_dir.mkdir(parents=True, exist_ok=True)

    zip_target = assets_dir / "overmax.zip"
    manifest_target = assets_dir / "release_manifest.json"
    shutil.copy2(source_zip, zip_target)
    shutil.copy2(source_manifest, manifest_target)

    base_url = f"http://{args.host}:{args.port}"
    release_data = {
        "tag_name": f"v{target_version}",
        "assets": [
            {
                "name": "overmax.zip",
                "browser_download_url": f"{base_url}/assets/overmax.zip",
            },
            {
                "name": "release_manifest.json",
                "browser_download_url": f"{base_url}/assets/release_manifest.json",
            },
        ],
    }
    (api_dir / "latest").write_text(json.dumps(release_data, ensure_ascii=False, indent=2), encoding="utf-8")

    print("[LocalUpdateTest] feed 생성 완료")
    print(f"  feed_dir: {feed_root}")
    print(f"  latest tag: v{target_version}")
    print(f"  overmax.zip sha256: {sha256_of(zip_target)}")
    print("  아래 환경변수를 설정한 뒤 앱을 실행하세요:")
    print(f"    set OVERMAX_UPDATE_LATEST_URL={base_url}/repos/{args.owner}/{args.repo}/releases/latest")


if __name__ == "__main__":
    main()
