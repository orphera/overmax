# Local Auto-Update Test

## 1) Build release artifacts

```bat
build.bat
```

Expected files:
- `dist/overmax.zip`
- `dist/release_manifest.json`

## 2) Create mock release feed

```bat
python scripts/prepare_update_test_feed.py
```

Optional:
- `--version 0.1.99` to force a specific target version
- `--port 9000` to use a different local port

## 3) Start local feed server

```bat
scripts\start_update_test_server.bat
```

Keep this terminal open.

## 4) Run app with local update endpoint

Open a new terminal:

```bat
set OVERMAX_UPDATE_LATEST_URL=http://127.0.0.1:8765/repos/orphera/overmax/releases/latest
cd dist\overmax
overmax.exe
```

## 5) Expected behavior

- App logs show a newer version detected.
- `cache/update/` receives downloaded zip and extracted stage files.
- App exits and updater script copies files.
- App restarts automatically.

## Troubleshooting

- If no update is detected, ensure mock `tag_name` is higher than `core/version.py`.
- If update fails, inspect console log lines starting with `[AppUpdater]`.
- If server cannot start on `8765`, regenerate with another port and match env var.
