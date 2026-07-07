use std::path::Path;

use super::AppUpdateConfig;

pub fn notify_previous_update(_app_dir: &Path) -> Result<bool, String> {
    Ok(true)
}

pub fn check_and_apply_update_blocking(
    _app_dir: &Path,
    _cfg: &AppUpdateConfig,
) -> Result<bool, String> {
    Ok(true)
}
