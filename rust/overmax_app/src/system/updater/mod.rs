pub mod version;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{check_and_apply_update_blocking, notify_previous_update};
#[cfg(target_os = "windows")]
pub use windows::{check_and_apply_update_blocking, notify_previous_update};

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
    pub fn from_settings(settings: &overmax_data::Settings) -> Self {
        let mut c = Self::default();
        let u = settings.app_update();
        c.enabled = u.enabled;
        c.owner = u.owner.unwrap_or_else(|| "orphera".to_string());
        c.repo = u.repo.unwrap_or_else(|| "overmax".to_string());
        c.asset_name = u.asset_name.unwrap_or_else(|| "overmax.zip".to_string());
        c.latest_release_url = u.latest_release_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
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

/// Main executable to restart after update (default `overmax.exe` on Windows, `overmax` on Linux).
pub fn main_exe_name() -> String {
    std::env::var("OVERMAX_MAIN_EXE").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") { "overmax.exe" } else { "overmax" }.into()
    })
}
