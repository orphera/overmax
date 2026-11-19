use super::{
    diff_settings, load_base_settings, load_merged_settings, merge_settings_layers,
    normalize_settings, AutoUploadScope, Settings, SettingsPaths,
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
    let root = std::env::temp_dir().join(format!("overmax-settings-test-{}", std::process::id()));
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
        "language": "fr", // unknown should reset to "auto"
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
        },
        "recommend": {
            "target_rate": 99.6 // should snap to 99.5 or 99.7 => (99.6-99.5)=0.1 vs (99.7-99.6)=0.1 => 99.5
        },
        "ipc": {
            "port": 99999 // out of 1024..=65535, should fall back to 30110
        }
    });

    normalize_settings(&mut settings);

    assert_eq!(settings["language"], json!("auto"));
    assert_eq!(settings["overlay"]["scale"], json!(1.0));
    assert_eq!(settings["overlay"]["base_opacity"], json!(1.0));
    assert_eq!(settings["window_tracker"]["poll_interval_sec"], json!(0.05));
    assert_eq!(
        settings["varchive"]["user_map"]["some_id"],
        json!({"v_id": "some_v_id", "account_path": ""})
    );
    assert_eq!(settings["recommend"]["target_rate"], json!(99.5));
    assert_eq!(settings["ipc"]["port"], json!(30110));
}

#[test]
fn auto_upload_defaults_off_result_only_three_seconds() {
    let settings: Settings = serde_json::from_value(json!({"varchive": {}})).unwrap();
    let auto = settings.varchive().auto_upload;
    assert!(!auto.enabled);
    assert_eq!(auto.scope, AutoUploadScope::ResultOnly);
    assert_eq!(auto.delay_sec, 3);
}

#[test]
fn normalize_settings_repairs_invalid_auto_upload_values() {
    let mut settings = json!({
        "varchive": {
            "auto_upload": {"enabled": "yes", "scope": "everywhere", "delay_sec": 10}
        }
    });

    normalize_settings(&mut settings);

    assert_eq!(
        settings["varchive"]["auto_upload"],
        json!({"enabled": false, "scope": "result_only", "delay_sec": 3})
    );
    // 교정된 값은 Settings 전체 역직렬화를 깨뜨리지 않아야 한다.
    assert!(serde_json::from_value::<Settings>(settings).is_ok());
}

#[test]
fn normalize_settings_keeps_valid_auto_upload_values() {
    let original = json!({
        "varchive": {
            "auto_upload": {"enabled": true, "scope": "select_and_result", "delay_sec": 0}
        }
    });
    let mut settings = original.clone();

    normalize_settings(&mut settings);

    assert_eq!(settings, original);
}

#[test]
fn test_normalize_settings_keeps_valid_language() {
    for valid in ["ko", "en", "ja", "auto"] {
        let mut settings = json!({"language": valid});
        normalize_settings(&mut settings);
        assert_eq!(settings["language"], json!(valid));
    }
}
