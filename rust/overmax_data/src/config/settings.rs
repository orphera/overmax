use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsPaths {
    pub settings_json: PathBuf,
    pub settings_user_json: PathBuf,
}

impl SettingsPaths {
    pub fn in_dir(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            settings_json: root.join("settings.json"),
            settings_user_json: root.join("settings.user.json"),
        }
    }
}

pub fn load_merged_settings(root: impl AsRef<Path>, defaults: Value) -> Value {
    let paths = SettingsPaths::in_dir(root);
    merge_settings_layers(
        defaults,
        load_json_object(&paths.settings_json),
        load_json_object(&paths.settings_user_json),
    )
}

/// Packaged defaults merged with `settings.json` only (no `settings.user.json`), for delta-save base.
pub fn load_base_settings(root: impl AsRef<Path>, defaults: Value) -> Value {
    let paths = SettingsPaths::in_dir(root);
    merge_settings_layers(
        defaults,
        load_json_object(&paths.settings_json),
        empty_object(),
    )
}

pub fn merge_settings_layers(
    defaults: Value,
    settings_json: Value,
    settings_user_json: Value,
) -> Value {
    let mut merged = object_or_empty(defaults);
    merge_object_value(&mut merged, settings_json);
    merge_object_value(&mut merged, settings_user_json);
    Value::Object(merged)
}

fn load_json_object(path: &Path) -> Value {
    let Ok(text) = fs::read_to_string(path) else {
        return empty_object();
    };

    serde_json::from_str(&text).unwrap_or_else(|_| empty_object())
}

fn merge_object_value(base: &mut Map<String, Value>, override_value: Value) {
    let Value::Object(override_map) = override_value else {
        return;
    };

    merge_maps(base, override_map);
}

fn merge_maps(base: &mut Map<String, Value>, override_map: Map<String, Value>) {
    for (key, value) in override_map {
        match (base.get_mut(&key), value) {
            (Some(Value::Object(base_child)), Value::Object(override_child)) => {
                merge_maps(base_child, override_child);
            }
            (_, replacement) => {
                base.insert(key, replacement);
            }
        }
    }
}

fn object_or_empty(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

const ALLOWED_SCALES: &[f64] = &[0.75, 1.0, 1.25, 1.5];

pub fn normalize_settings(settings: &mut Value) {
    let Value::Object(map) = settings else { return };

    if let Some(Value::Object(overlay)) = map.get_mut("overlay") {
        if let Some(scale) = overlay.get("scale").and_then(|v| v.as_f64()) {
            let mut closest = 1.0;
            let mut min_diff = f64::MAX;
            for &s in ALLOWED_SCALES {
                let diff = (s - scale).abs();
                if diff < min_diff {
                    min_diff = diff;
                    closest = s;
                }
            }
            overlay.insert("scale".to_string(), json!(closest));
        } else if overlay.contains_key("scale") {
            overlay.insert("scale".to_string(), json!(1.0));
        }

        if let Some(opacity) = overlay.get("base_opacity").and_then(|v| v.as_f64()) {
            let clamped = opacity.clamp(0.1, 1.0);
            overlay.insert("base_opacity".to_string(), json!(clamped));
        } else if overlay.contains_key("base_opacity") {
            overlay.insert("base_opacity".to_string(), json!(0.8));
        }
    }

    for (section, key) in [("window_tracker", "poll_interval_sec")] {
        if let Some(Value::Object(sec)) = map.get_mut(section) {
            if let Some(val) = sec.get(key).and_then(|v| v.as_f64()) {
                sec.insert(key.to_string(), json!(val.max(0.05)));
            }
        }
    }

    if let Some(Value::Object(jacket)) = map.get_mut("jacket_matcher") {
        if let Some(threshold) = jacket.get("similarity_threshold").and_then(|v| v.as_f64()) {
            let clamped = threshold.clamp(0.0, 1.0);
            jacket.insert("similarity_threshold".to_string(), json!(clamped));
        } else if jacket.contains_key("similarity_threshold") {
            jacket.insert("similarity_threshold".to_string(), json!(0.65));
        }

        if let Some(margin) = jacket.get("margin_threshold").and_then(|v| v.as_f64()) {
            let clamped = margin.max(0.0);
            jacket.insert("margin_threshold".to_string(), json!(clamped));
        } else if jacket.contains_key("margin_threshold") {
            jacket.insert("margin_threshold".to_string(), json!(3.0));
        }
    }

    if let Some(Value::Object(varchive)) = map.get_mut("varchive") {
        if let Some(Value::Object(user_map)) = varchive.get_mut("user_map") {
            for (_, val) in user_map.iter_mut() {
                if let Some(s) = val.as_str() {
                    let mut new_val = Map::new();
                    new_val.insert("v_id".to_string(), json!(s));
                    new_val.insert("account_path".to_string(), json!(""));
                    *val = Value::Object(new_val);
                }
            }
        }
    }

    if let Some(Value::Object(sf)) = map.get_mut("sync_filter") {
        if let Some(min_idx) = sf.get("min_level_idx").and_then(|v| v.as_u64()) {
            sf.insert("min_level_idx".to_string(), json!(min_idx.min(29)));
        }
        if let Some(max_idx) = sf.get("max_level_idx").and_then(|v| v.as_u64()) {
            sf.insert("max_level_idx".to_string(), json!(max_idx.min(29)));
        }
        if let Some(min_r) = sf.get("min_rate").and_then(|v| v.as_f64()) {
            sf.insert("min_rate".to_string(), json!(min_r.clamp(0.0, 100.0)));
        }
        if let Some(max_r) = sf.get("max_rate").and_then(|v| v.as_f64()) {
            sf.insert("max_rate".to_string(), json!(max_r.clamp(0.0, 100.0)));
        }
    }
}

pub fn diff_settings(base: &Value, current: &Value) -> Value {
    let mut diff = Map::new();
    let Value::Object(base_map) = base else {
        return current.clone();
    };
    let Value::Object(current_map) = current else {
        return current.clone();
    };

    for (key, val) in current_map {
        match base_map.get(key) {
            None => {
                diff.insert(key.clone(), val.clone());
            }
            Some(base_val) => {
                if let (Value::Object(base_obj), Value::Object(val_obj)) = (base_val, val) {
                    let sub_diff = diff_settings(
                        &Value::Object(base_obj.clone()),
                        &Value::Object(val_obj.clone()),
                    );
                    if let Value::Object(sub_map) = &sub_diff {
                        if !sub_map.is_empty() {
                            diff.insert(key.clone(), sub_diff);
                        }
                    }
                } else if base_val != val {
                    diff.insert(key.clone(), val.clone());
                }
            }
        }
    }
    Value::Object(diff)
}

pub fn save_user_settings(root: impl AsRef<Path>, diff: &Value) -> std::io::Result<()> {
    let paths = SettingsPaths::in_dir(root);
    if let Some(parent) = paths.settings_user_json.parent() {
        fs::create_dir_all(parent)?;
    }

    let text = serde_json::to_string_pretty(diff).unwrap_or_else(|_| "{}".to_string());
    fs::write(&paths.settings_user_json, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        diff_settings, load_base_settings, load_merged_settings, merge_settings_layers,
        normalize_settings, SettingsPaths,
    };
    use serde_json::{json, Value};
    use std::fs;

    #[test]
    fn load_base_ignores_user_json_layer() {
        let root = std::env::temp_dir().join(format!("overmax-base-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("settings.json"), r#"{"overlay":{"scale":1.25}}"#).unwrap();
        fs::write(
            root.join("settings.user.json"),
            r#"{"overlay":{"base_opacity":0.3}}"#,
        )
        .unwrap();

        let defaults = json!({"overlay":{"scale":1.0,"base_opacity":0.8}});
        let base = load_base_settings(&root, defaults.clone());
        assert_eq!(base["overlay"]["scale"], json!(1.25));
        assert_eq!(base["overlay"]["base_opacity"], json!(0.8));

        let merged = load_merged_settings(&root, defaults);
        assert_eq!(merged["overlay"]["base_opacity"], json!(0.3));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn user_settings_override_packaged_settings_and_defaults() {
        let merged = merge_settings_layers(
            json!({
                "overlay": {"scale": 1.0, "base_opacity": 0.8},
                "varchive": {"cache_ttl_sec": 86400}
            }),
            json!({
                "overlay": {"base_opacity": 0.6},
                "varchive": {"cache_ttl_sec": 60}
            }),
            json!({
                "overlay": {"scale": 1.25}
            }),
        );

        assert_eq!(merged["overlay"]["scale"], json!(1.25));
        assert_eq!(merged["overlay"]["base_opacity"], json!(0.6));
        assert_eq!(merged["varchive"]["cache_ttl_sec"], json!(60));
    }

    #[test]
    fn non_object_layers_are_ignored() {
        let merged = merge_settings_layers(
            json!({"overlay": {"scale": 1.0}}),
            Value::Null,
            json!(["invalid"]),
        );

        assert_eq!(merged, json!({"overlay": {"scale": 1.0}}));
    }

    #[test]
    fn loads_settings_json_then_user_settings_json_from_root() {
        let root =
            std::env::temp_dir().join(format!("overmax-settings-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        fs::write(root.join("settings.json"), r#"{"overlay":{"scale":1.25}}"#).unwrap();
        fs::write(
            root.join("settings.user.json"),
            r#"{"overlay":{"base_opacity":0.7}}"#,
        )
        .unwrap();

        let merged = load_merged_settings(&root, json!({"overlay":{"scale":1.0}}));
        assert_eq!(merged["overlay"]["scale"], json!(1.25));
        assert_eq!(merged["overlay"]["base_opacity"], json!(0.7));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uses_python_compatible_file_names() {
        let paths = SettingsPaths::in_dir("data");

        assert!(paths.settings_json.ends_with("settings.json"));
        assert!(paths.settings_user_json.ends_with("settings.user.json"));
    }

    #[test]
    fn test_diff_settings() {
        let base = json!({
            "a": 1,
            "b": {"x": 10, "y": 20},
            "c": 3
        });
        let current = json!({
            "a": 1,
            "b": {"x": 10, "y": 25, "z": 30},
            "d": 4
        });

        let diff = diff_settings(&base, &current);
        assert_eq!(
            diff,
            json!({
                "b": {"y": 25, "z": 30},
                "d": 4
            })
        );
    }

    #[test]
    fn test_normalize_settings() {
        let mut settings = json!({
            "overlay": {
                "scale": 1.1, // should snap to 1.0 or 1.25. (1.1-1.0)=0.1, (1.25-1.1)=0.15 => 1.0
                "base_opacity": 1.5 // should clamp to 1.0
            },
            "window_tracker": {
                "poll_interval_sec": 0.01 // should become 0.05
            },
            "varchive": {
                "user_map": {
                    "some_id": "some_v_id"
                }
            }
        });

        normalize_settings(&mut settings);

        assert_eq!(settings["overlay"]["scale"], json!(1.0));
        assert_eq!(settings["overlay"]["base_opacity"], json!(1.0));
        assert_eq!(settings["window_tracker"]["poll_interval_sec"], json!(0.05));
        assert_eq!(
            settings["varchive"]["user_map"]["some_id"],
            json!({"v_id": "some_v_id", "account_path": ""})
        );
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WindowTrackerSettings {
    #[serde(default = "default_window_title")]
    pub window_title: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_sec: f64,
}

fn default_window_title() -> String {
    "DJMAX RESPECT V".to_string()
}
fn default_poll_interval() -> f64 {
    0.5
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScreenCaptureSettings {
    #[serde(default = "default_logo_cooldown")]
    pub logo_ocr_cooldown_sec: f64,
    #[serde(default = "default_idle_sleep")]
    pub idle_sleep_sec: f64,
    #[serde(default = "default_active_sleep")]
    pub active_sleep_ms: u64,
    #[serde(default = "default_background_sleep")]
    pub background_sleep_ms: u64,
}

fn default_logo_cooldown() -> f64 {
    1.0
}
fn default_idle_sleep() -> f64 {
    0.5
}
fn default_active_sleep() -> u64 {
    120
}
fn default_background_sleep() -> u64 {
    500
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DebugWindowSettings {
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default = "default_debug_title")]
    pub title: String,
}

fn default_max_lines() -> usize {
    500
}
fn default_debug_title() -> String {
    "Overmax Debug Log".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OverlayPosition {
    #[serde(default = "default_snap")]
    pub snap: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

fn default_snap() -> String {
    "manual".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OverlaySettings {
    #[serde(default = "default_base_opacity")]
    pub base_opacity: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub lite_mode: bool,
    #[serde(default)]
    pub position: OverlayPosition,
}

fn default_base_opacity() -> f64 {
    0.8
}
fn default_scale() -> f64 {
    1.0
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self {
            snap: default_snap(),
            x: None,
            y: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JacketMatcherSettings {
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_similarity")]
    pub similarity_threshold: f64,
    #[serde(default)]
    pub disable_hog: bool,
    #[serde(default = "default_margin")]
    pub margin_threshold: f64,
}

fn default_db_path() -> String {
    "cache/image_index.db".to_string()
}
fn default_similarity() -> f64 {
    0.65
}
fn default_margin() -> f64 {
    3.0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppUpdateSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub asset_name: Option<String>,
    pub linux_asset_name: Option<String>,
    pub latest_release_url: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VArchiveUserMap {
    pub v_id: Option<String>,
    pub account_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VArchiveSettings {
    #[serde(default = "default_songs_url")]
    pub songs_api_url: String,
    #[serde(default = "default_songs_cache")]
    pub songs_cache_path: String,
    #[serde(default = "default_dlcs_url")]
    pub dlcs_api_url: String,
    #[serde(default = "default_dlcs_cache")]
    pub dlcs_cache_path: String,
    #[serde(default = "default_songs_cache")]
    pub cache_path: String,
    #[serde(default = "default_ttl")]
    pub cache_ttl_sec: u64,
    #[serde(default = "default_timeout")]
    pub download_timeout_sec: u64,
    #[serde(default)]
    pub user_map: std::collections::HashMap<String, VArchiveUserMap>,
}

fn default_songs_url() -> String {
    "https://v-archive.net/db/v2/songs.json".to_string()
}
fn default_songs_cache() -> String {
    "cache/songs.json".to_string()
}
fn default_dlcs_url() -> String {
    "https://v-archive.net/db/dlcs.json".to_string()
}
fn default_dlcs_cache() -> String {
    "cache/dlcs.json".to_string()
}
fn default_ttl() -> u64 {
    86400
}
fn default_timeout() -> u64 {
    10
}

impl Default for WindowTrackerSettings {
    fn default() -> Self {
        Self {
            window_title: default_window_title(),
            poll_interval_sec: default_poll_interval(),
        }
    }
}
impl Default for ScreenCaptureSettings {
    fn default() -> Self {
        Self {
            logo_ocr_cooldown_sec: default_logo_cooldown(),
            idle_sleep_sec: default_idle_sleep(),
            active_sleep_ms: default_active_sleep(),
            background_sleep_ms: default_background_sleep(),
        }
    }
}
impl Default for DebugWindowSettings {
    fn default() -> Self {
        Self {
            max_lines: default_max_lines(),
            title: default_debug_title(),
        }
    }
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            base_opacity: default_base_opacity(),
            scale: default_scale(),
            lite_mode: false,
            position: OverlayPosition::default(),
        }
    }
}
impl Default for JacketMatcherSettings {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            similarity_threshold: default_similarity(),
            disable_hog: false,
            margin_threshold: default_margin(),
        }
    }
}
impl Default for AppUpdateSettings {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            owner: None,
            repo: None,
            asset_name: None,
            linux_asset_name: None,
            latest_release_url: None,
        }
    }
}
impl Default for VArchiveSettings {
    fn default() -> Self {
        Self {
            songs_api_url: default_songs_url(),
            songs_cache_path: default_songs_cache(),
            dlcs_api_url: default_dlcs_url(),
            dlcs_cache_path: default_dlcs_cache(),
            cache_path: default_songs_cache(),
            cache_ttl_sec: default_ttl(),
            download_timeout_sec: default_timeout(),
            user_map: std::collections::HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncFilterSettings {
    #[serde(default)]
    pub open: bool,

    #[serde(default = "default_true")]
    pub mode_4b: bool,
    #[serde(default = "default_true")]
    pub mode_5b: bool,
    #[serde(default = "default_true")]
    pub mode_6b: bool,
    #[serde(default = "default_true")]
    pub mode_8b: bool,

    #[serde(default = "default_true")]
    pub diff_nm: bool,
    #[serde(default = "default_true")]
    pub diff_hd: bool,
    #[serde(default = "default_true")]
    pub diff_mx: bool,
    #[serde(default = "default_true")]
    pub diff_sc: bool,

    #[serde(default = "default_min_level_idx")]
    pub min_level_idx: usize,
    #[serde(default = "default_max_level_idx")]
    pub max_level_idx: usize,

    #[serde(default = "default_min_rate")]
    pub min_rate: f64,
    #[serde(default = "default_max_rate")]
    pub max_rate: f64,

    #[serde(default)]
    pub require_mc_not_on_varchive: bool,

    #[serde(default)]
    pub exclude_unuploaded: bool,
}

fn default_min_level_idx() -> usize {
    0
}
fn default_max_level_idx() -> usize {
    29
}
fn default_min_rate() -> f64 {
    0.0
}
fn default_max_rate() -> f64 {
    100.0
}

impl Default for SyncFilterSettings {
    fn default() -> Self {
        Self {
            open: false,
            mode_4b: true,
            mode_5b: true,
            mode_6b: true,
            mode_8b: true,
            diff_nm: true,
            diff_hd: true,
            diff_mx: true,
            diff_sc: true,
            min_level_idx: 0,
            max_level_idx: 29,
            min_rate: 0.0,
            max_rate: 100.0,
            require_mc_not_on_varchive: false,
            exclude_unuploaded: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub window_tracker: Option<WindowTrackerSettings>,
    #[serde(default)]
    pub screen_capture: Option<ScreenCaptureSettings>,
    #[serde(default)]
    pub debug_window: Option<DebugWindowSettings>,
    #[serde(default)]
    pub overlay: Option<OverlaySettings>,
    #[serde(default)]
    pub jacket_matcher: Option<JacketMatcherSettings>,
    #[serde(default)]
    pub app_update: Option<AppUpdateSettings>,
    #[serde(default)]
    pub varchive: Option<VArchiveSettings>,
    #[serde(default)]
    pub sync_filter: Option<SyncFilterSettings>,
}

impl Settings {
    pub fn window_tracker(&self) -> WindowTrackerSettings {
        self.window_tracker.clone().unwrap_or_default()
    }
    pub fn screen_capture(&self) -> ScreenCaptureSettings {
        self.screen_capture.clone().unwrap_or_default()
    }
    pub fn debug_window(&self) -> DebugWindowSettings {
        self.debug_window.clone().unwrap_or_default()
    }
    pub fn overlay(&self) -> OverlaySettings {
        self.overlay.clone().unwrap_or_default()
    }
    pub fn jacket_matcher(&self) -> JacketMatcherSettings {
        self.jacket_matcher.clone().unwrap_or_default()
    }
    pub fn app_update(&self) -> AppUpdateSettings {
        self.app_update.clone().unwrap_or_default()
    }
    pub fn varchive(&self) -> VArchiveSettings {
        self.varchive.clone().unwrap_or_default()
    }
    pub fn sync_filter(&self) -> SyncFilterSettings {
        self.sync_filter.clone().unwrap_or_default()
    }
}
