use eframe::egui::ViewportId;

pub fn vp_debug() -> ViewportId {
    ViewportId::from_hash_of("overmax_debug_vp")
}

pub fn vp_settings() -> ViewportId {
    ViewportId::from_hash_of("overmax_settings_vp")
}

pub fn vp_sync() -> ViewportId {
    ViewportId::from_hash_of("overmax_sync_vp")
}

pub fn first_steam_from_settings(settings: &overmax_data::Settings) -> String {
    let varchive = settings.varchive();
    varchive.user_map.keys().next().cloned().unwrap_or_default()
}

pub fn account_path_for_steam(settings: &overmax_data::Settings, steam: &str) -> String {
    settings
        .varchive()
        .user_map
        .get(steam)
        .and_then(|entry| entry.account_path.clone())
        .unwrap_or_default()
}

pub fn button_num(mode: &str) -> i32 {
    overmax_data::community::client::Mode::from_str(mode)
        .map(|m| m.button_count())
        .unwrap_or(4)
}

/// 시스템 파일 탐색기로 지정된 디렉터리를 엽니다.
pub fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unsupported platform",
        ))
    }
}
