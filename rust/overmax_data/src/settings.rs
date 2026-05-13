use serde_json::{Map, Value};
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

#[cfg(test)]
mod tests {
    use super::{load_merged_settings, merge_settings_layers, SettingsPaths};
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
}
