#!/usr/bin/env bash
set -euo pipefail

server_args="-screen 0 1280x720x24 +extension Composite +extension MIT-SHM -nolisten tcp"

xvfb-run --auto-servernum --server-args="$server_args" bash -euo pipefail -c '
    openbox --sm-disable &
    wm_pid=$!
    trap '\''kill "$wm_pid" 2>/dev/null || true'\'' EXIT

    cargo test -p overmax-engine --lib \
        capture::window_tracker::linux::tests::x11_ewmh_snapshot_lifecycle \
        --locked -- --ignored --exact --nocapture
    cargo test -p overmax-engine --lib \
        capture::capture_engine::linux::tests::xcomposite_shm_lifecycle \
        --locked -- --ignored --exact --nocapture
'

xvfb-run --auto-servernum \
    --server-args="-screen 0 1280x720x24 -extension Composite -nolisten tcp" \
    cargo test -p overmax-engine --lib \
        capture::capture_engine::linux::tests::unavailable_capture_backend_is_deferred_to_set_target \
        --locked -- --ignored --exact --nocapture
