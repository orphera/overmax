use std::path::PathBuf;

pub(super) fn find_steam_path() -> Option<String> {
    steam_path_candidates()
        .into_iter()
        .find(|path| path.join("config").join("loginusers.vdf").exists())
        .map(|path| path.to_string_lossy().into_owned())
}

fn steam_path_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        paths.push(PathBuf::from(xdg_data_home).join("Steam"));
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join(".steam").join("steam"));
        paths.push(home.join(".local").join("share").join("Steam"));
    }
    paths
}
