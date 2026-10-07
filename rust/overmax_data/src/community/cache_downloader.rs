use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::community::client::VArchiveDB;
use crate::community::sheet_meta::{
    AssistMeta, GoldMeta, PatternMetaEntry, PatternSheetMeta, PatternSheetMetaItem,
};
use crate::config::compatibility::DataCompatibility;
use crate::config::settings::Settings;
use crate::gateway::asset_download::{
    download_asset_bytes, download_asset_text, fetch_github_release_asset_url,
};
use overmax_core::Mode;

const PATTERN_META_CACHE: &str = "cache/pattern_meta.json";
const IMAGE_DB_OWNER: &str = "orphera";
const IMAGE_DB_REPO: &str = "overmax-image-db";
const IMAGE_DB_ASSET: &str = "image_index.db";
const IMAGE_DB_VERSION: &str = "image_db_version.txt";
const SHEET_ID: &str = "1ks1dwJyNjkAXYtQ_6UZIeNOCGOmhf2jMbakpTcJm9rw";
const DAY: Duration = Duration::from_secs(60 * 60 * 24);

const SHEET_GIDS: &[(Mode, &str)] = &[
    (Mode::B4, "979055934"),
    (Mode::B5, "112529029"),
    (Mode::B6, "2010625608"),
    (Mode::B8, "1833696991"),
];

type LogFn<'a> = &'a mut dyn FnMut(String);

pub struct CacheUpdateResult {
    pub updated_varchive_db: Option<VArchiveDB>,
    pub updated_sheet_meta: Option<PatternSheetMeta>,
    /// `image_index.db` 파일이 새 버전으로 교체됨 (이미 메모리에 로드된 인덱스는 낡은 상태)
    pub updated_image_index: bool,
}

/// `StartupCacheManager::poll_updates`가 이번 호출에서 반영한 갱신 종류.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PolledUpdates {
    /// `varchive_db` 또는 `sheet_meta`가 교체됨
    pub data: bool,
    /// `image_index.db`가 교체됨 — 호출자가 인덱스 소비자에게 재로드를 알려야 함
    pub image_index: bool,
}

pub struct StartupCacheManager {
    rx: Receiver<CacheUpdateResult>,
}

impl StartupCacheManager {
    pub fn init(root: &Path, settings: &Settings, log_tx: Sender<String>) -> Self {
        let root_buf = root.to_path_buf();
        let settings_clone = settings.clone();
        let (tx, rx) = mpsc::channel();

        if !has_all_required_caches(root, settings) {
            let log_tx_clone = log_tx.clone();
            refresh_startup_caches(root, settings, &mut |msg| {
                let _ = log_tx_clone.send(msg);
            });
        } else {
            std::thread::spawn(move || {
                let mut logs = Vec::new();
                let mut updated_any = false;
                let image_path = root_buf.join(&settings_clone.jacket_matcher().db_path);
                let image_version_before = local_version(&image_path);

                refresh_startup_caches(&root_buf, &settings_clone, &mut |msg| {
                    if msg.contains("갱신 완료") || msg.contains("업데이트 완료") {
                        updated_any = true;
                    }
                    logs.push(msg);
                });

                for msg in logs {
                    let _ = log_tx.send(msg);
                }

                if updated_any {
                    let compat = DataCompatibility::current();
                    let songs_path = root_buf.join(compat.songs_json);
                    let dlcs_path = root_buf.join(compat.dlcs_json);
                    let meta_path = root_buf.join(PATTERN_META_CACHE);

                    let mut new_vdb = VArchiveDB::new();
                    let _ = new_vdb.load_dlcs_from_file(&dlcs_path);
                    let vdb_ok = new_vdb.load_from_file(&songs_path).is_ok();

                    let new_vdb_opt = if vdb_ok { Some(new_vdb) } else { None };

                    let new_meta_opt = if meta_path.exists() {
                        new_vdb_opt
                            .as_ref()
                            .map(|vdb| PatternSheetMeta::load_cache(meta_path, vdb))
                    } else {
                        None
                    };

                    let _ = tx.send(CacheUpdateResult {
                        updated_varchive_db: new_vdb_opt,
                        updated_sheet_meta: new_meta_opt,
                        updated_image_index: local_version(&image_path) != image_version_before,
                    });
                }
            });
        }

        Self { rx }
    }

    pub fn poll_updates(
        &self,
        varchive_db: &mut Arc<VArchiveDB>,
        sheet_meta: &mut Arc<PatternSheetMeta>,
    ) -> PolledUpdates {
        let mut polled = PolledUpdates::default();
        while let Ok(res) = self.rx.try_recv() {
            if let Some(new_vdb) = res.updated_varchive_db {
                *varchive_db = Arc::new(new_vdb);
                polled.data = true;
            }
            if let Some(new_meta) = res.updated_sheet_meta {
                *sheet_meta = Arc::new(new_meta);
                polled.data = true;
            }
            polled.image_index |= res.updated_image_index;
        }
        polled
    }
}

pub fn has_all_required_caches(root: &Path, settings: &Settings) -> bool {
    let varchive = settings.varchive();

    let songs_path = root.join(&varchive.cache_path);
    let dlcs_path = root.join(&varchive.dlcs_cache_path);
    let pattern_meta_path = root.join(PATTERN_META_CACHE);
    let image_db_path = root.join(&settings.jacket_matcher().db_path);

    is_valid_file(&songs_path)
        && is_valid_file(&dlcs_path)
        && is_valid_file(&pattern_meta_path)
        && is_valid_file(&image_db_path)
}

fn is_valid_file(path: &Path) -> bool {
    path.exists()
        && std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
}

pub fn refresh_startup_caches(root: &Path, settings: &Settings, log: LogFn<'_>) {
    refresh_songs_json(root, settings, &mut *log);
    refresh_dlcs_json(root, settings, &mut *log);

    // Load VArchiveDB dynamically to map CSV rows to song IDs
    let compat = DataCompatibility::current();
    let songs_path = root.join(compat.songs_json);
    let dlcs_path = root.join(compat.dlcs_json);
    let mut varchive_db = VArchiveDB::new();

    // Load dlcs first if available
    let _ = varchive_db.load_dlcs_from_file(&dlcs_path);

    if let Err(e) = varchive_db.load_from_file(&songs_path) {
        log(format!(
            "[Cache] songs.json 로드 실패 (패턴 메타 매칭용): {e}"
        ));
    }

    refresh_pattern_meta(root, &varchive_db, &mut *log);
    refresh_image_index(root, settings, &mut *log);
}

fn refresh_songs_json(root: &Path, settings: &Settings, log: LogFn<'_>) {
    let varchive = settings.varchive();
    let path = root.join(&varchive.cache_path);
    let ttl = varchive.cache_ttl_sec;
    if !is_stale(&path, Duration::from_secs(ttl)) {
        return;
    }
    let url = &varchive.songs_api_url;
    let timeout = varchive.download_timeout_sec;
    match download_asset_bytes(url, Some(Duration::from_secs(timeout))) {
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

fn refresh_dlcs_json(root: &Path, settings: &Settings, log: LogFn<'_>) {
    let varchive = settings.varchive();
    let path = root.join(&varchive.dlcs_cache_path);
    let ttl = varchive.cache_ttl_sec;
    if !is_stale(&path, Duration::from_secs(ttl)) {
        return;
    }
    let url = &varchive.dlcs_api_url;
    let timeout = varchive.download_timeout_sec;
    match download_asset_bytes(url, Some(Duration::from_secs(timeout))) {
        Ok(bytes) => {
            if let Err(e) = write_atomic(&path, &bytes) {
                log(format!("[Cache] dlcs.json 저장 실패: {e}"));
            } else {
                log("[Cache] dlcs.json 갱신 완료".into());
            }
        }
        Err(e) => log(format!("[Cache] dlcs.json 갱신 실패: {e}")),
    }
}

fn refresh_pattern_meta(root: &Path, varchive_db: &VArchiveDB, log: LogFn<'_>) {
    let path = root.join(PATTERN_META_CACHE);
    if !is_stale(&path, DAY) {
        return;
    }
    type Key = (String, overmax_core::Mode, overmax_core::Difficulty);
    let mut items: HashMap<Key, PatternSheetMetaItem> = HashMap::new();
    for (mode, gid) in SHEET_GIDS {
        match download_asset_text(&sheet_csv_url(gid), Some(Duration::from_secs(10))) {
            Ok(csv) => merge_sheet_meta(&mut items, *mode, &csv, varchive_db),
            Err(e) => log(format!("[Cache] pattern meta {mode} 갱신 실패: {e}")),
        }
    }
    let entries: Vec<PatternMetaEntry> = items
        .into_iter()
        .map(|((song_id, mode, diff), meta)| PatternMetaEntry {
            song_id,
            mode,
            diff,
            meta,
        })
        .collect();
    let Ok(text) = serde_json::to_vec_pretty(&entries) else {
        return;
    };
    if let Err(e) = write_atomic(&path, &text) {
        log(format!("[Cache] pattern_meta.json 저장 실패: {e}"));
    } else {
        log("[Cache] pattern_meta.json 갱신 완료".into());
    }
}

fn refresh_image_index(root: &Path, settings: &Settings, log: LogFn<'_>) {
    let path = root.join(&settings.jacket_matcher().db_path);
    let Ok((tag, url)) =
        fetch_github_release_asset_url(IMAGE_DB_OWNER, IMAGE_DB_REPO, IMAGE_DB_ASSET)
    else {
        log("[ImageDBUpdater] 릴리즈 정보 조회 실패".into());
        return;
    };
    if local_version(&path).as_deref() == Some(tag.as_str()) && path.exists() {
        log(format!("[ImageDBUpdater] 최신 버전 유지 중: {tag}"));
        return;
    }
    match download_asset_bytes(&url, Some(Duration::from_secs(60)))
        .and_then(|b| write_atomic(&path, &b))
    {
        Ok(()) => {
            let _ = std::fs::write(version_path(&path), &tag);
            log(format!("[ImageDBUpdater] 업데이트 완료: {tag}"));
        }
        Err(e) => log(format!("[ImageDBUpdater] 다운로드 실패: {e}")),
    }
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

fn merge_sheet_meta(
    items: &mut HashMap<
        (String, overmax_core::Mode, overmax_core::Difficulty),
        PatternSheetMetaItem,
    >,
    mode: Mode,
    csv: &str,
    varchive_db: &VArchiveDB,
) {
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
        let Some(parsed_diff) = overmax_core::Difficulty::from_str(&diff) else {
            continue;
        };
        let meta = pattern_meta_value(mode, &values);
        let has_content = !meta.gold.is_none()
            || !meta.note.is_empty()
            || !meta.assist_key.is_none()
            || meta.keypart;

        if has_content {
            let level_str = pick(&values, &["레벨", "Level"]);
            let level = level_str.parse::<f64>().map(|f| f as u32).ok();
            let category = pick(&values, &["카테고리", "Category"]);
            let note = pick(&values, &["비고", "Note"]);

            let song_id = if let Some(song) =
                varchive_db.find_best_match(&title, mode, parsed_diff, level, &category, &note)
            {
                song.title.to_string()
            } else {
                norm(&title)
            };
            items.insert((song_id, mode, parsed_diff), meta);
        }
    }
}

fn pattern_meta_value(mode: Mode, values: &HashMap<String, String>) -> PatternSheetMetaItem {
    let raw_gold = pick(values, &["황배 여부", "황배여부"]);
    let gold = if raw_gold.is_empty() {
        GoldMeta::None
    } else if raw_gold.contains("[H]") {
        GoldMeta::HalfRandom
    } else if raw_gold.contains("[M]") {
        GoldMeta::MaxRandom
    } else {
        GoldMeta::Random
    };

    let note = pick(values, &["비고", "Note"]);
    let mut keypart = false;

    if mode == Mode::B8 {
        let raw_keypart = pick(values, &["키파트 위주", "키파트위주"]);
        if !raw_keypart.is_empty() {
            keypart = true;
        }
    }

    let raw_assist = pick(values, &["보조 키 여부", "보조키여부"]);
    let assist_key = if raw_assist.contains("❌") {
        AssistMeta::Used
    } else if raw_assist.contains("⚠️") || raw_assist.starts_with("⚠") {
        AssistMeta::Caution
    } else if raw_assist.contains("✅") {
        AssistMeta::NotUsed
    } else {
        AssistMeta::None
    };

    PatternSheetMetaItem {
        gold,
        note,
        assist_key,
        keypart,
    }
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
    use overmax_core::Difficulty;

    #[test]
    fn csv_parser_handles_quoted_commas() {
        let rows = parse_csv("곡명,난이도,비고\n\"A, B\",SC,\"변속, 급감속\"\n");

        assert_eq!(rows[1][0], "A, B");
        assert_eq!(rows[1][2], "변속, 급감속");
    }

    #[test]
    fn sheet_meta_merge_matches_python_cache_shape() {
        let mut db = VArchiveDB::new();
        let mut patterns: [[Option<crate::community::client::PatternInfo>; 4]; 4] =
            Default::default();
        patterns[Mode::B5 as usize][Difficulty::SC as usize] =
            Some(crate::community::client::PatternInfo {
                level: Some(12),
                floor: None,
                floor_name: None,
                rating: None,
            });

        db.songs.push(crate::community::client::Song {
            title: "1".into(),
            name: "Love ☆ Panic".into(),
            composer: Arc::from("ESTi"),
            dlc_code: Arc::from(""),
            patterns,
        });

        type Key = (String, Mode, Difficulty);
        let mut items: HashMap<Key, PatternSheetMetaItem> = HashMap::new();
        merge_sheet_meta(
            &mut items,
            Mode::B5,
            "곡명,난이도,황배 여부,비고,보조 키 여부\nLove ☆ Panic,SC,O,개인차,❌\n",
            &db,
        );

        let key = ("1".to_string(), Mode::B5, Difficulty::SC);
        assert_eq!(
            items.get(&key).unwrap(),
            &PatternSheetMetaItem {
                gold: GoldMeta::Random,
                note: "개인차".into(),
                assist_key: AssistMeta::Used,
                keypart: false,
            }
        );
    }
}
