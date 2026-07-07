use std::path::PathBuf;

pub(super) fn find_steam_path() -> Option<String> {
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        let path = PathBuf::from(xdg_data_home).join("Steam");
        if has_loginusers_vdf(&path) {
            return Some(path.to_string_lossy().into_owned());
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        for path in [
            home.join(".steam").join("steam"),
            home.join(".local").join("share").join("Steam"),
        ] {
            if has_loginusers_vdf(&path) {
                return Some(path.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn has_loginusers_vdf(path: &PathBuf) -> bool {
    path.join("config").join("loginusers.vdf").exists()
}
