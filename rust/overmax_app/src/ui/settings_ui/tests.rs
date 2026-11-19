use super::save_settings_to_disk;
use overmax_data::{load_merged_settings, SettingsPaths};
use serde_json::json;
use std::fs;

#[test]
fn save_user_roundtrip_matches_python_delta_policy() {
    let root = std::env::temp_dir().join(format!("overmax-app-settings-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"overlay":{"scale":1.0,"base_opacity":0.8},"window_tracker":{"poll_interval_sec":0.5}}"#,
    )
    .unwrap();
    fs::write(root.join("settings.user.json"), "{}").unwrap();

    let defaults = json!({
        "overlay": {"scale": 1.0, "base_opacity": 0.8},
        "window_tracker": {"poll_interval_sec": 0.5}
    });
    let base = overmax_data::load_base_settings(&root, defaults.clone());
    let mut merged = load_merged_settings(&root, defaults.clone());
    let mut draft = merged.clone();
    draft["overlay"]["base_opacity"] = json!(0.55);

    save_settings_to_disk(&root, &defaults, &base, &mut draft, &mut merged).unwrap();

    let reloaded = load_merged_settings(&root, defaults);
    assert_eq!(reloaded["overlay"]["base_opacity"], json!(0.55));
    assert_eq!(reloaded["overlay"]["scale"], json!(1.0));

    let user_text = fs::read_to_string(root.join("settings.user.json")).unwrap();
    assert!(user_text.contains("base_opacity"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn save_settings_to_paths_roundtrip() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    fs::write(
        root.join("settings.json"),
        r#"{"overlay":{"scale":1.0,"base_opacity":0.8}}"#,
    )
    .unwrap();
    fs::write(root.join("settings.user.json"), "{}").unwrap();

    let paths = SettingsPaths::in_dir(root);
    let defaults = json!({"overlay": {"scale": 1.0, "base_opacity": 0.8}});
    let base = overmax_data::load_base_settings_from_paths(&paths, defaults.clone());
    let mut merged = overmax_data::load_merged_settings_from_paths(&paths, defaults.clone());
    let mut draft = merged.clone();
    draft["overlay"]["scale"] = json!(1.5);

    super::save_settings_to_paths(&paths, &defaults, &base, &mut draft, &mut merged).unwrap();

    let reloaded = overmax_data::load_merged_settings_from_paths(&paths, defaults);
    assert_eq!(reloaded["overlay"]["scale"], json!(1.5));
}
