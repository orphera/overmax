//! GitHub Releases app update (paths and filenames aligned with Python `app_updater.py`).

pub mod paths;
pub mod release;
pub mod result_io;
pub mod version;
pub mod worker;

use std::path::Path;

use serde_json::Value;

use paths::{applied_tag_path, update_root};
use release::{download_and_verify, extract_zip, fetch_latest_release, resolve_payload_dir};
use result_io::{cleanup_artifacts, consume_result, peek_result};
use version::is_newer_version;
use worker::spawn_update_worker;

/// `settings.json` / merged `app_update` section defaults match Python `core/app.py`.
#[derive(Debug, Clone)]
pub struct AppUpdateConfig {
    pub enabled: bool,
    pub owner: String,
    pub repo: String,
    pub asset_name: String,
    pub latest_release_url: Option<String>,
}

impl Default for AppUpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            owner: "orphera".into(),
            repo: "overmax".into(),
            asset_name: "overmax.zip".into(),
            latest_release_url: None,
        }
    }
}

impl AppUpdateConfig {
    pub fn from_merged_settings(v: &Value) -> Self {
        let mut c = Self::default();
        let Some(u) = v.get("app_update") else {
            return c;
        };
        c.enabled = u.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
        c.owner = u
            .get("owner")
            .and_then(|x| x.as_str())
            .unwrap_or("orphera")
            .to_string();
        c.repo = u
            .get("repo")
            .and_then(|x| x.as_str())
            .unwrap_or("overmax")
            .to_string();
        c.asset_name = u
            .get("asset_name")
            .and_then(|x| x.as_str())
            .unwrap_or("overmax.zip")
            .to_string();
        c.latest_release_url = u
            .get("latest_release_url")
            .and_then(|x| x.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(String::from);
        if let Ok(ov) = std::env::var("OVERMAX_UPDATE_LATEST_URL") {
            let t = ov.trim();
            if !t.is_empty() {
                c.latest_release_url = Some(t.to_string());
            }
        }
        c
    }
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Main executable to restart after update (default `overmax-rs.exe`).
pub fn main_exe_name() -> String {
    std::env::var("OVERMAX_MAIN_EXE").unwrap_or_else(|_| "overmax-rs.exe".into())
}

fn skip_auto_update_by_policy() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    std::env::var("OVERMAX_SKIP_APP_UPDATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// If another update is in progress, show message and return `false` (do not start GUI).
pub fn notify_previous_update(app_dir: &Path) -> Result<bool, String> {
    if let Some(p) = peek_result(app_dir) {
        if p.get("status").map(|s| s.as_str()) == Some("started") {
            show_message_mb_ok(
                "Overmax Update",
                "다른 업데이트 작업이 진행 중입니다.\n\n업데이트 완료 후 Overmax가 자동으로 시작됩니다.",
            );
            return Ok(false);
        }
    }
    if let Some(r) = consume_result(app_dir) {
        cleanup_artifacts(app_dir);
        handle_consumed_result(r)?;
    } else {
        cleanup_artifacts(app_dir);
    }
    Ok(true)
}

fn handle_consumed_result(r: std::collections::HashMap<String, String>) -> Result<(), String> {
    let status = r.get("status").map(String::as_str).unwrap_or("");
    let from_ver = r.get("from").cloned().unwrap_or_else(|| "unknown".into());
    let to_ver = r.get("to").cloned().unwrap_or_else(|| "unknown".into());
    if status == "success" {
        if is_newer_version(&to_ver, app_version()) {
            show_message_mb_ok(
                "Overmax Update Error",
                &format!(
                    "자동 패치를 적용했지만 현재 실행 버전이 갱신되지 않았습니다.\n\n\
                     현재: v{}\n목표: {}\n\n\
                     동일 태그에 대한 자동 재시도는 건너뜁니다. 릴리즈 패키지를 확인해 주세요.",
                    app_version(),
                    to_ver
                ),
            );
        } else {
            eprintln!("[Main] 자동 패치 완료: {from_ver} -> {to_ver}");
        }
    } else {
        let reason = r.get("reason").cloned().unwrap_or_else(|| "unknown".into());
        show_message_mb_ok(
            "Overmax Update Error",
            &format!(
                "자동 패치가 완료되지 않았습니다.\n\n사유: {reason}\n시도 버전: {from_ver} -> {to_ver}"
            ),
        );
    }
    Ok(())
}

/// Returns `Ok(false)` if the app should exit (worker spawned).
pub fn check_and_apply_update_blocking(
    app_dir: &Path,
    cfg: &AppUpdateConfig,
) -> Result<bool, String> {
    if skip_auto_update_by_policy() {
        eprintln!("[AppUpdater] 개발/스킵 모드에서는 자동 패치를 건너뜁니다.");
        return Ok(true);
    }
    if !cfg.enabled {
        return Ok(true);
    }
    let release = fetch_latest_release(cfg).map_err(|e| e.to_string())?;
    let Some(tag) = release.tag else {
        return Ok(true);
    };
    let Some(url) = release.asset_url else {
        return Ok(true);
    };
    if !is_newer_version(&tag, app_version()) {
        eprintln!("[AppUpdater] 최신 버전 유지 중: {}", app_version());
        return Ok(true);
    }
    if should_skip_repeated_tag(app_dir, &tag, app_version()) {
        eprintln!(
            "[AppUpdater] 동일 태그 업데이트가 이미 적용된 상태로 판단되어 재시도를 건너뜁니다: {tag} (현재 v{})",
            app_version()
        );
        return Ok(true);
    }
    if !ask_update_confirm(app_version(), &tag) {
        eprintln!("[AppUpdater] 사용자가 이번 실행의 자동 패치를 취소했습니다.");
        return Ok(true);
    }
    eprintln!("[AppUpdater] 새 버전 감지: {} -> {tag}", app_version());
    let zip_path = update_root(app_dir).join(&cfg.asset_name);
    let stage_dir = update_root(app_dir).join("stage");
    download_and_verify(&url, release.manifest_url.as_deref(), &cfg.asset_name, &zip_path)
        .map_err(|e| e.to_string())?;
    extract_zip(&zip_path, &stage_dir).map_err(|e| e.to_string())?;
    let Some(payload) = resolve_payload_dir(&stage_dir) else {
        return Err("자동 패치 결과물에서 실행 파일을 찾지 못했습니다.".into());
    };
    if !spawn_update_worker(app_dir, &payload, app_version(), &tag) {
        return Err("자동 패치 워커 실행에 실패했습니다.".into());
    }
    eprintln!("[AppUpdater] 업데이트 워커를 시작했습니다. 현재 앱을 종료합니다.");
    Ok(false)
}

fn should_skip_repeated_tag(app_dir: &Path, latest_tag: &str, current_version: &str) -> bool {
    let Some(applied) = read_applied_tag(app_dir) else {
        return false;
    };
    if applied != latest_tag {
        return false;
    }
    is_newer_version(latest_tag, current_version)
}

fn read_applied_tag(app_dir: &Path) -> Option<String> {
    let p = applied_tag_path(app_dir);
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn show_message_mb_ok(title: &str, msg: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    };

    let title_w: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let msg_w: Vec<u16> = OsStr::new(msg).encode_wide().chain(Some(0)).collect();
    let icon = if title.contains("Error") {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | icon,
        );
    }
}

fn ask_update_confirm(current: &str, latest_tag: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
    };

    let title = "Overmax Update";
    let msg = format!(
        "새 앱 업데이트가 있습니다.\n\n현재 버전: v{current}\n최신 버전: {latest_tag}\n\n지금 업데이트를 진행할까요?"
    );
    let title_w: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let msg_w: Vec<u16> = OsStr::new(&msg).encode_wide().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            msg_w.as_ptr(),
            title_w.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}
