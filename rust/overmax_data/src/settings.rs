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

pub fn merge_settings_layers(defaults: Value, settings_json: Value, settings_user_json: Value) -> Value {
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

    for (section, key) in [
        ("window_tracker", "poll_interval_sec"),
        ("screen_capture", "ocr_interval_sec"),
        ("jacket_matcher", "match_interval_sec"),
    ] {
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
            jacket.insert("similarity_threshold".to_string(), json!(0.6));
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
                    let sub_diff = diff_settings(&Value::Object(base_obj.clone()), &Value::Object(val_obj.clone()));
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
    
    let text = serde_json::to_string_pretty(diff)
        .unwrap_or_else(|_| "{}".to_string());
    fs::write(&paths.settings_user_json, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        diff_settings, load_merged_settings, merge_settings_layers, normalize_settings,
        SettingsPaths,
    };
    use serde_json::{json, Value};
    use std::fs;

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
        let root = std::env::temp_dir().join(format!(
            "overmax-settings-test-{}",
            std::process::id()
        ));
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
        assert_eq!(diff, json!({
            "b": {"y": 25, "z": 30},
            "d": 4
        }));
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
