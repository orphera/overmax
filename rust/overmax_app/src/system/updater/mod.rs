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
    pub linux_asset_name: String,
    pub latest_release_url: Option<String>,
}

impl Default for AppUpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            owner: "orphera".into(),
            repo: "overmax".into(),
            asset_name: "overmax.zip".into(),
            linux_asset_name: "overmax-linux-x86_64.tar.gz".into(),
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
        c.linux_asset_name = u
            .linux_asset_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "overmax-linux-x86_64.tar.gz".to_string());
        c.latest_release_url = u
            .latest_release_url
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

pub fn main_exe_name() -> String {
    std::env::var("OVERMAX_MAIN_EXE")
        .unwrap_or_else(|_| format!("overmax{}", std::env::consts::EXE_SUFFIX))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::AppUpdateConfig;
    use serde_json::json;

    #[test]
    fn linux_asset_name_is_backward_compatible_and_overridable() {
        let legacy = serde_json::from_value(json!({"app_update": {"enabled": true}}))
            .expect("legacy settings");
        assert_eq!(
            AppUpdateConfig::from_settings(&legacy).linux_asset_name,
            "overmax-linux-x86_64.tar.gz"
        );

        let custom = serde_json::from_value(
            json!({"app_update": {"enabled": true, "linux_asset_name": "custom.tgz"}}),
        )
        .expect("custom settings");
        assert_eq!(
            AppUpdateConfig::from_settings(&custom).linux_asset_name,
            "custom.tgz"
        );
    }
}
