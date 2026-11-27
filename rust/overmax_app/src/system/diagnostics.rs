//! 문제 제보용 진단 정보 zip 내보내기.
//!
//! 로그, 실행 환경, 사용자 설정을 하나의 zip으로 묶는다. 계정·경로처럼 플레이어를 특정할 수 있는
//! 값은 묶기 전에 가린다.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use overmax_data::AppPaths;
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::log_file;

/// 저장 대화상자 기본 파일 이름
pub fn default_file_name() -> String {
    format!("overmax-diagnostics-{}.zip", log_file::file_stamp())
}

/// `dest`에 진단 zip을 만든다.
pub fn export(paths: &AppPaths, dest: &Path) -> io::Result<()> {
    let masker = Masker::from_env();
    let mut zip = ZipWriter::new(File::create(dest)?);
    let options = SimpleFileOptions::default();

    zip.start_file("system.txt", options)?;
    zip.write_all(masker.mask(&system_info(paths)).as_bytes())?;

    if let Ok(raw) = fs::read_to_string(paths.settings_user_json()) {
        zip.start_file("settings.user.json", options)?;
        zip.write_all(masker.mask(&mask_settings(&raw)).as_bytes())?;
    }

    let log_dir = log_file::log_dir(paths.data_dir());
    if let Ok(entries) = fs::read_dir(&log_dir) {
        let mut logs: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| log_file::is_log_file(p))
            .collect();
        logs.sort();
        for path in logs {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let bytes = fs::read(&path)?;
            zip.start_file(format!("logs/{name}"), options)?;
            zip.write_all(masker.mask(&String::from_utf8_lossy(&bytes)).as_bytes())?;
        }
    }

    zip.finish()?;
    Ok(())
}

fn system_info(paths: &AppPaths) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!(
        "version: {}\nos: {} {}\nportable: {}\nexe: {exe}\ndata_dir: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        paths.is_portable(),
        paths.data_dir().display(),
    )
}

/// V-Archive 계정 매핑(`varchive.user_map`)은 Steam ID와 계정 파일 경로를 담고 있어 개수만 남긴다.
fn mask_settings(raw: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(raw) else {
        return "<settings.user.json: JSON 파싱 실패>".to_string();
    };
    if let Some(map) = value
        .get_mut("varchive")
        .and_then(|v| v.get_mut("user_map"))
    {
        let count = map.as_object().map_or(0, |m| m.len());
        *map = Value::String(format!("<{count} entries redacted>"));
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// 자유 텍스트에서 홈 디렉터리·사용자 이름·Steam ID를 가린다.
struct Masker {
    home: Option<String>,
    user: Option<String>,
}

impl Masker {
    fn from_env() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok();
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok();
        Self::new(home, user)
    }

    fn new(home: Option<String>, user: Option<String>) -> Self {
        let keep = |s: &String| s.trim().len() >= 3;
        Self {
            home: home.filter(keep),
            user: user.filter(keep),
        }
    }

    fn mask(&self, text: &str) -> String {
        let mut out = text.to_string();
        if let Some(home) = &self.home {
            out = out.replace(home.as_str(), "~");
            // JSON 문자열은 역슬래시가 이스케이프되어 있다.
            out = out.replace(&home.replace('\\', "\\\\"), "~");
        }
        if let Some(user) = &self.user {
            out = out.replace(user.as_str(), "<user>");
        }
        mask_steam_ids(&out)
    }
}

/// SteamID64(7656119로 시작하는 17자리 숫자)를 가린다.
fn mask_steam_ids(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_digit()) {
            let end = bytes[i..]
                .iter()
                .position(|b| !b.is_ascii_digit())
                .map_or(bytes.len(), |n| i + n);
            let run = &text[i..end];
            if run.len() == 17 && run.starts_with("7656119") {
                out.push_str("<steamid>");
            } else {
                out.push_str(run);
            }
            i = end;
        } else {
            let ch = text[i..].chars().next().unwrap_or_default();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_home_user_and_steam_id() {
        let masker = Masker::new(Some(r"C:\Users\alice".into()), Some("alice".into()));
        let text = r#"open C:\Users\alice\x.db; json "C:\\Users\\alice\\y"; id 76561198000000001 n 1234 by alice"#;
        assert_eq!(
            masker.mask(text),
            r#"open ~\x.db; json "~\\y"; id <steamid> n 1234 by <user>"#
        );
    }

    #[test]
    fn redacts_varchive_user_map() {
        let raw = r#"{"language":"ko","varchive":{"user_map":{"76561198000000001":{"account_path":"a"}}}}"#;
        let masked = mask_settings(raw);
        assert!(masked.contains("<1 entries redacted>"));
        assert!(!masked.contains("account_path"));
        assert!(masked.contains("\"language\": \"ko\""));
    }

    #[test]
    fn export_writes_expected_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new_portable(temp.path());
        fs::write(paths.settings_user_json(), r#"{"language":"ko"}"#).expect("settings");
        let logs = log_file::log_dir(paths.data_dir());
        fs::create_dir_all(&logs).expect("logs");
        fs::write(logs.join("overmax-20261127-000000.log"), "hello").expect("log");

        let dest = temp.path().join("out.zip");
        export(&paths, &dest).expect("export");

        let mut archive = zip::ZipArchive::new(File::open(&dest).expect("open")).expect("zip");
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| {
                archive
                    .by_index(i)
                    .expect("entry")
                    .name()
                    .expect("name")
                    .to_string()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "logs/overmax-20261127-000000.log",
                "settings.user.json",
                "system.txt"
            ]
        );
        assert!(archive.by_name("system.txt").is_ok());
    }
}
