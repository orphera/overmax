use std::path::Path;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

pub(super) fn find_steam_path() -> Option<String> {
    for path in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") {
        if let Ok(steam_path) = key.get_value::<String, _>("SteamPath") {
            let trimmed = steam_path
                .trim()
                .trim_end_matches('/')
                .trim_end_matches('\\');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for subkey in [r"Software\Valve\Steam", r"Software\Wow6432Node\Valve\Steam"] {
        if let Ok(key) = hklm.open_subkey(subkey) {
            for val_name in ["SteamPath", "InstallPath"] {
                if let Ok(steam_path) = key.get_value::<String, _>(val_name) {
                    let trimmed = steam_path
                        .trim()
                        .trim_end_matches('/')
                        .trim_end_matches('\\');
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    find_steam_from_processes()
}

fn find_steam_from_processes() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe_name = OsString::from_wide(&entry.szExeFile[..len]);
                if exe_name.to_string_lossy().eq_ignore_ascii_case("steam.exe") {
                    let pid = entry.th32ProcessID;
                    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if !handle.is_null() {
                        let mut buffer = [0u16; 4096];
                        let mut size = buffer.len() as u32;
                        if QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size)
                            != 0
                        {
                            CloseHandle(handle);
                            CloseHandle(snapshot);
                            let full_path = OsString::from_wide(&buffer[..size as usize]);
                            let path = PathBuf::from(full_path);
                            if let Some(parent) = path.parent() {
                                return Some(parent.to_string_lossy().into_owned());
                            }
                            return None;
                        }
                        CloseHandle(handle);
                    }
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    None
}
