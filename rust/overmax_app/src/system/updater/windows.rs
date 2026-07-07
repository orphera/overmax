use std::path::Path;

use super::version::is_newer_version;
use super::{app_version, main_exe_name, AppUpdateConfig};

/// Clean up any leftover artifacts from previous custom updater runs, if any.
pub fn notify_previous_update(_app_dir: &Path) -> Result<bool, String> {
    Ok(true)
}

/// Returns `Ok(false)` if the app has been updated and should exit.
pub fn check_and_apply_update_blocking(
    _app_dir: &Path,
    cfg: &AppUpdateConfig,
) -> Result<bool, String> {
    if skip_auto_update_by_policy() {
        eprintln!("[AppUpdater] 개발/스킵 모드에서는 자동 패치를 건너뜁니다.");
        return Ok(true);
    }
    if !cfg.enabled {
        return Ok(true);
    }

    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(&cfg.owner)
        .repo_name(&cfg.repo)
        .bin_name(main_exe_name().as_str())
        .target("")
        .identifier(&cfg.asset_name)
        .current_version(app_version())
        .no_confirm(true)
        .show_download_progress(false);

    let updater = match builder.build() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[AppUpdater] 업데이터 구성 실패: {}", e);
            return Ok(true);
        }
    };

    let latest_release = match updater.get_latest_release() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[AppUpdater] 업데이트 확인 실패: {}", e);
            return Ok(true);
        }
    };

    if !is_newer_version(&latest_release.version, app_version()) {
        eprintln!("[AppUpdater] 최신 버전 유지 중: {}", app_version());
        return Ok(true);
    }

    if !ask_update_confirm(app_version(), &latest_release.version) {
        eprintln!("[AppUpdater] 사용자가 이번 실행의 자동 패치를 취소했습니다.");
        return Ok(true);
    }

    eprintln!(
        "[AppUpdater] 새 버전 감지: {} -> {}. 업데이트 진행...",
        app_version(),
        latest_release.version
    );

    let status = match updater.update() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[AppUpdater] 업데이트 실패: {}", e);
            show_message_mb_ok(
                "Overmax Update Error",
                &format!("자동 패치가 완료되지 않았습니다.\n\n사유: {}", e),
            );
            return Ok(true);
        }
    };

    let updated = match status {
        self_update::Status::Updated(_) => true,
        self_update::Status::UpToDate(_) => false,
    };

    if updated {
        eprintln!("[AppUpdater] 업데이트 완료! 앱을 재시작합니다.");
        Ok(false)
    } else {
        eprintln!("[AppUpdater] 이미 최신 버전입니다.");
        Ok(true)
    }
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
        "새 앱 업데이트가 있습니다.\n\n현재 버전: {current}\n최신 버전: {latest_tag}\n\n지금 업데이트를 진행할까요?"
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

fn skip_auto_update_by_policy() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    std::env::var("OVERMAX_SKIP_APP_UPDATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
