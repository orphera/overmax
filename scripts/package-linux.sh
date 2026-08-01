#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
[[ $(uname -m) == x86_64 ]] || { echo "Linux bundle supports x86_64 only" >&2; exit 1; }

cargo build --release --locked -p overmax-app --bin overmax-rs

stage=dist/overmax
archive=dist/overmax-linux-x86_64.tar.gz
rm -rf "$stage"
rm -f "$archive"
install -Dm755 target/release/overmax-rs "$stage/overmax"
install -Dm644 settings.json "$stage/settings.json"
install -Dm644 README.md "$stage/README.md"
mkdir "$stage/cache"
tar -czf "$archive" -C dist overmax
sha256sum "$archive" > "$archive.sha256"

echo "Created $archive"
