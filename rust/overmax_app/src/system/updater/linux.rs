use std::path::Path;

use super::version::is_newer_version;
use super::{app_version, main_exe_name, AppUpdateConfig};

const BIN_PATH_IN_ARCHIVE: &str = "overmax/overmax";

pub fn notify_previous_update(_app_dir: &Path) -> Result<bool, String> {
    Ok(true)
}

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
        .bin_path_in_archive(BIN_PATH_IN_ARCHIVE)
        .target("")
        .identifier(&cfg.linux_asset_name)
        .current_version(app_version())
        .no_confirm(true)
        .show_download_progress(false);

    let updater = match builder.build() {
        Ok(updater) => updater,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이터 구성 실패: {error}");
            return Ok(true);
        }
    };
    let latest_release = match updater.get_latest_release() {
        Ok(release) => release,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이트 확인 실패: {error}");
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
        Ok(status) => status,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이트 실패: {error}");
            show_update_error(&error.to_string());
            return Ok(true);
        }
    };

    if status.updated() {
        eprintln!("[AppUpdater] 업데이트 완료! 앱을 재시작합니다.");
        Ok(false)
    } else {
        eprintln!("[AppUpdater] 이미 최신 버전입니다.");
        Ok(true)
    }
}

fn ask_update_confirm(current: &str, latest: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title("Overmax Update")
        .set_description(format!(
            "새 앱 업데이트가 있습니다.\n\n현재 버전: {current}\n최신 버전: {latest}\n\n지금 업데이트를 진행할까요?"
        ))
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

fn show_update_error(error: &str) {
    let _ = rfd::MessageDialog::new()
        .set_title("Overmax Update Error")
        .set_description(format!("자동 패치가 완료되지 않았습니다.\n\n사유: {error}"))
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn skip_auto_update_by_policy() -> bool {
    cfg!(debug_assertions)
        || std::env::var("OVERMAX_SKIP_APP_UPDATE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}
