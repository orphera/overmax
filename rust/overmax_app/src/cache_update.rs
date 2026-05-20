//! Startup cache refreshes matching the Python runtime policy.

use reqwest::blocking::Client;
use reqwest::header::{CACHE_CONTROL, PRAGMA, USER_AGENT};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const USER_AGENT_VALUE: &str = concat!("overmax-rs/", env!("CARGO_PKG_VERSION"));
const SONGS_API_FALLBACK: &str = "https://v-archive.net/db/v2/songs.json";
const PATTERN_META_CACHE: &str = "cache/pattern_meta.json";
const IMAGE_DB_OWNER: &str = "orphera";
const IMAGE_DB_REPO: &str = "overmax-image-db";
const IMAGE_DB_ASSET: &str = "image_index.db";
const IMAGE_DB_VERSION: &str = "image_db_version.txt";
const SHEET_ID: &str = "1ks1dwJyNjkAXYtQ_6UZIeNOCGOmhf2jMbakpTcJm9rw";
const DAY: Duration = Duration::from_secs(60 * 60 * 24);

const SHEET_GIDS: &[(&str, &str)] = &[
    ("4B", "979055934"),
    ("5B", "112529029"),
    ("6B", "2010625608"),
    ("8B", "1833696991"),
];

type LogFn<'a> = &'a mut dyn FnMut(String);

pub fn refresh_startup_caches(root: &Path, settings: &Value, log: LogFn<'_>) {
    refresh_songs_json(root, settings, &mut *log);
    refresh_pattern_meta(root, &mut *log);
    refresh_image_index(root, settings, &mut *log);
}

fn refresh_songs_json(root: &Path, settings: &Value, log: LogFn<'_>) {
    let path = root.join(setting_str(
        settings,
        "varchive",
        "cache_path",
        "cache/songs.json",
    ));
    let ttl = setting_u64(settings, "varchive", "cache_ttl_sec", DAY.as_secs());
    if !is_stale(&path, Duration::from_secs(ttl)) {
        return;
    }
    let url = setting_str(settings, "varchive", "songs_api_url", SONGS_API_FALLBACK);
    let timeout = setting_u64(settings, "varchive", "download_timeout_sec", 10);
    match download_bytes(url, Duration::from_secs(timeout)) {
        Ok(bytes) => {
            if let Err(e) = write_atomic(&path, &bytes) {
                log(format!("[Cache] songs.json 저장 실패: {e}"));
            } else {
                log("[Cache] songs.json 갱신 완료".into());
            }
        }
        Err(e) => log(format!("[Cache] songs.json 갱신 실패: {e}")),
    }
}

fn refresh_pattern_meta(root: &Path, log: LogFn<'_>) {
    let path = root.join(PATTERN_META_CACHE);
    if !is_stale(&path, DAY) {
        return;
    }
    let mut items = serde_json::Map::new();
    for (mode, gid) in SHEET_GIDS {
        match download_text(&sheet_csv_url(gid), Duration::from_secs(10)) {
            Ok(csv) => merge_sheet_meta(&mut items, mode, &csv),
            Err(e) => log(format!("[Cache] pattern meta {mode} 갱신 실패: {e}")),
        }
    }
    let Ok(text) = serde_json::to_vec_pretty(&Value::Object(items)) else {
        return;
    };
    if let Err(e) = write_atomic(&path, &text) {
        log(format!("[Cache] pattern_meta.json 저장 실패: {e}"));
    } else {
        log("[Cache] pattern_meta.json 갱신 완료".into());
    }
}

fn refresh_image_index(root: &Path, settings: &Value, log: LogFn<'_>) {
    let path = root.join(setting_str(
        settings,
        "jacket_matcher",
        "db_path",
        "cache/image_index.db",
    ));
    let Ok((tag, url)) = latest_release_asset(IMAGE_DB_OWNER, IMAGE_DB_REPO, IMAGE_DB_ASSET) else {
        log("[ImageDBUpdater] 릴리즈 정보 조회 실패".into());
        return;
    };
    if local_version(&path).as_deref() == Some(tag.as_str()) && path.exists() {
        log(format!("[ImageDBUpdater] 최신 버전 유지 중: {tag}"));
        return;
    }
    match download_bytes(&url, Duration::from_secs(60)).and_then(|b| write_atomic(&path, &b)) {
        Ok(()) => {
            let _ = std::fs::write(version_path(&path), &tag);
            log(format!("[ImageDBUpdater] 업데이트 완료: {tag}"));
        }
        Err(e) => log(format!("[ImageDBUpdater] 다운로드 실패: {e}")),
    }
}

fn latest_release_asset(
    owner: &str,
    repo: &str,
    asset_name: &str,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let client = client(Duration::from_secs(10));
    let data: Value = no_cache_get(&client, &url)
        .send()?
        .error_for_status()?
        .json()?;
    let tag = data
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("tag_name 없음")?;
    let assets = data
        .get("assets")
        .and_then(Value::as_array)
        .ok_or("assets 없음")?;
    for asset in assets {
        if asset.get("name").and_then(Value::as_str) != Some(asset_name) {
            continue;
        }
        let Some(download_url) = asset.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        return Ok((tag.to_string(), download_url.to_string()));
    }
    Err(format!("asset 없음: {asset_name}").into())
}

fn client(timeout: Duration) -> Client {
    Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| Client::new())
}

fn download_bytes(
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let client = client(timeout);
    let bytes = no_cache_get(&client, url)
        .send()?
        .error_for_status()?
        .bytes()?;
    Ok(bytes.to_vec())
}

fn download_text(
    url: &str,
    timeout: Duration,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = download_bytes(url, timeout)?;
    Ok(String::from_utf8_lossy(&bytes)
        .trim_start_matches('\u{feff}')
        .to_string())
}

fn no_cache_get(client: &Client, url: &str) -> reqwest::blocking::RequestBuilder {
    client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn is_stale(path: &Path, ttl: Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    SystemTime::now().duration_since(modified).unwrap_or(ttl) >= ttl
}

fn setting_str<'a>(settings: &'a Value, section: &str, key: &str, fallback: &'a str) -> &'a str {
    settings
        .get(section)
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

fn setting_u64(settings: &Value, section: &str, key: &str, fallback: u64) -> u64 {
    settings
        .get(section)
        .and_then(|v| v.get(key))
        .and_then(Value::as_u64)
        .unwrap_or(fallback)
}

fn local_version(db_path: &Path) -> Option<String> {
    std::fs::read_to_string(version_path(db_path))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn version_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(IMAGE_DB_VERSION)
}

fn sheet_csv_url(gid: &str) -> String {
    format!("https://docs.google.com/spreadsheets/d/{SHEET_ID}/gviz/tq?tqx=out:csv&gid={gid}")
}

fn merge_sheet_meta(items: &mut serde_json::Map<String, Value>, mode: &str, csv: &str) {
    let rows = parse_csv(csv);
    let Some(headers) = rows.first() else {
        return;
    };
    for row in rows.iter().skip(1) {
        let values = row_map(headers, row);
        let title = pick(&values, &["곡명", "Title"]);
        let diff = pick(&values, &["난이도", "Diff"]);
        if title.is_empty() || diff.is_empty() {
            continue;
        }
        let meta = pattern_meta_value(mode, &values);
        if meta.as_object().is_some_and(|m| {
            m.values()
                .any(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        }) {
            items.insert(format!("{mode}|{}|{}", norm(&title), norm(&diff)), meta);
        }
    }
}

fn pattern_meta_value(mode: &str, values: &HashMap<String, String>) -> Value {
    let raw_gold = pick(values, &["황배 여부", "황배여부"]);
    let gold = if raw_gold.is_empty() {
        String::new()
    } else if raw_gold.contains("[H]") {
        "핲랜".to_string()
    } else if raw_gold.contains("[M]") {
        "맥랜".to_string()
    } else {
        "랜덤".to_string()
    };

    let mut note = pick(values, &["비고", "Note"]);
    let mut keypart = false;

    if mode == "8B" {
        let raw_keypart = pick(values, &["키파트 위주", "키파트위주"]);
        if !raw_keypart.is_empty() {
            keypart = true;
            if note.is_empty() {
                note = "키파트 위주 패턴".to_string();
            } else {
                note = format!("{} | 키파트 위주 패턴", note);
            }
        }
    }

    let mut meta = json!({
        "gold": gold,
        "note": note,
        "keypart": keypart,
    });

    if mode == "5B" {
        let raw_assist = pick(values, &["보조 키 여부", "보조키여부"]);
        if !raw_assist.is_empty() {
            let assist = match raw_assist.as_str() {
                "❌" => "사용",
                "️️⚠️" | "⚠" => "주의",
                "✅" => "미사용",
                other => other,
            };
            meta["assist_key"] = json!(assist);
        } else {
            meta["assist_key"] = json!("");
        }
    }
    meta
}

fn pick(values: &HashMap<String, String>, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| values.get(*key).map(|v| v.trim().to_string()))
        .unwrap_or_default()
}

fn row_map(headers: &[String], row: &[String]) -> HashMap<String, String> {
    headers.iter().cloned().zip(row.iter().cloned()).collect()
}

fn norm(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => push_cell(&mut row, &mut cell),
            '\n' if !quoted => push_row(&mut rows, &mut row, &mut cell),
            '\r' if !quoted => {}
            _ => cell.push(ch),
        }
    }
    push_row(&mut rows, &mut row, &mut cell);
    rows
}

fn push_cell(row: &mut Vec<String>, cell: &mut String) {
    row.push(std::mem::take(cell));
}

fn push_row(rows: &mut Vec<Vec<String>>, row: &mut Vec<String>, cell: &mut String) {
    push_cell(row, cell);
    if row.iter().any(|v| !v.is_empty()) {
        rows.push(std::mem::take(row));
    } else {
        row.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_handles_quoted_commas() {
        let rows = parse_csv("곡명,난이도,비고\n\"A, B\",SC,\"변속, 급감속\"\n");

        assert_eq!(rows[1][0], "A, B");
        assert_eq!(rows[1][2], "변속, 급감속");
    }

    #[test]
    fn sheet_meta_merge_matches_python_cache_shape() {
        let mut items = serde_json::Map::new();
        merge_sheet_meta(
            &mut items,
            "5B",
            "곡명,난이도,황배 여부,비고,보조 키 여부\nLove ☆ Panic,SC,O,개인차,○\n",
        );

        assert_eq!(
            items.get("5B|love☆panic|sc").unwrap(),
            &json!({"gold": "정배", "note": "개인차", "keypart": false, "assist_key": "사용"})
        );
    }
}
