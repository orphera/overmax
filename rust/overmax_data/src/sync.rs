//! Local record vs V-Archive cached API records — mirrors `data/sync_manager.py`.

use crate::record_db::RecordDB;
use crate::varchive::VArchiveDB;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SyncCandidate {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub dlc: String,
    pub button_mode: String,
    pub difficulty: String,
    pub overmax_rate: f64,
    pub overmax_mc: bool,
    pub varchive_rate: Option<f64>,
    pub varchive_mc: Option<bool>,
    pub upload_status: String,
    pub upload_message: String,
}

impl SyncCandidate {
    pub fn reason_label(&self) -> String {
        let mut parts = Vec::new();
        if self.varchive_rate.is_none() {
            parts.push("미등록".to_string());
        } else if self.overmax_rate > self.varchive_rate.unwrap_or(0.0) {
            parts.push(format!(
                "+{:.2}%",
                self.overmax_rate - self.varchive_rate.unwrap_or(0.0)
            ));
        }
        if self.overmax_mc && !self.varchive_mc.unwrap_or(false) {
            parts.push("MC".to_string());
        }
        parts.join(" · ")
    }
}

/// Loads `cache/varchive/{steam_id}/{4|5|6|8}.json` into a lookup map.
pub fn load_varchive_record_cache(
    cache_root: &Path,
    steam_id: &str,
) -> HashMap<(i32, String, String), (f64, bool)> {
    let mut cache = HashMap::new();
    if steam_id.is_empty() || steam_id == "__unknown__" {
        return cache;
    }
    let user_dir = cache_root.join(steam_id);
    for button in [4i32, 5, 6, 8] {
        let button_mode = format!("{button}B");
        let path = user_dir.join(format!("{button}.json"));
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(records) = root.get("records").and_then(|v| v.as_array()) else {
            continue;
        };
        merge_record_entries(&mut cache, records, &button_mode);
    }
    cache
}

fn merge_record_entries(
    cache: &mut HashMap<(i32, String, String), (f64, bool)>,
    records: &[Value],
    button_mode: &str,
) {
    for rec in records {
        let Some(title) = rec.get("title") else {
            continue;
        };
        let Some(song_id) = parse_song_id(title) else {
            continue;
        };
        let Some(diff) = rec.get("pattern").and_then(|v| v.as_str()) else {
            continue;
        };
        let rate = rec
            .get("score")
            .and_then(|v| v.as_f64())
            .or_else(|| rec.get("score").and_then(|v| v.as_i64()).map(|i| i as f64))
            .unwrap_or(0.0);
        let is_max_combo = rec
            .get("maxCombo")
            .and_then(|v| v.as_bool())
            .or_else(|| rec.get("maxCombo").and_then(|v| v.as_u64()).map(|n| n != 0))
            .unwrap_or(false);
        cache.insert(
            (song_id, button_mode.to_string(), diff.to_string()),
            (rate, is_max_combo),
        );
    }
}

fn parse_song_id(title: &Value) -> Option<i32> {
    match title {
        Value::Number(n) => n.as_i64().and_then(|v| i32::try_from(v).ok()),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn sort_key(c: &SyncCandidate) -> (i8, f64) {
    match c.varchive_rate {
        None => (1, -c.overmax_rate),
        Some(vr) => {
            let diff = c.overmax_rate - vr;
            if diff > 0.0 {
                (0, -diff)
            } else {
                (2, 0.0)
            }
        }
    }
}

/// Builds sync candidates for one Steam id using local `record.db` and on-disk V-Archive cache.
pub fn build_candidates(
    varchive_db: &VArchiveDB,
    record_db: &RecordDB,
    steam_id: &str,
    varchive_cache_root: &Path,
) -> Vec<SyncCandidate> {
    let local_map = record_db.all_records_for_steam(steam_id);
    if local_map.is_empty() {
        return Vec::new();
    }
    let varchive_cache = load_varchive_record_cache(varchive_cache_root, steam_id);
    let mut candidates = Vec::new();

    for ((song_id, mode, diff), (local_rate, local_mc)) in local_map {
        if local_rate <= 0.0 {
            continue;
        }
        let v_entry = varchive_cache.get(&(song_id, mode.clone(), diff.clone()));
        let v_rate = v_entry.map(|e| e.0);
        let v_mc = v_entry.map(|e| e.1);

        let is_candidate = v_rate.is_none()
            || local_rate > v_rate.unwrap_or(0.0)
            || (local_mc && !v_mc.unwrap_or(false));
        if !is_candidate {
            continue;
        }

        let (song_name, composer, dlc) = match varchive_db.search_by_id(song_id) {
            Some(s) => (s.name.clone(), s.composer.clone(), s.dlc_code.clone()),
            None => (song_id.to_string(), String::new(), String::new()),
        };

        candidates.push(SyncCandidate {
            song_id,
            song_name,
            composer,
            dlc,
            button_mode: mode,
            difficulty: diff,
            overmax_rate: local_rate,
            overmax_mc: local_mc,
            varchive_rate: v_rate,
            varchive_mc: v_mc,
            upload_status: String::new(),
            upload_message: String::new(),
        });
    }

    candidates.sort_by(|a, b| {
        sort_key(a)
            .partial_cmp(&sort_key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

/// Merges one score into `cache/varchive/{steam_id}/{button}.json` (same shape as Python client).
pub fn upsert_varchive_cache_record(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
    song_id: i32,
    difficulty: &str,
    score: f64,
    is_max_combo: bool,
) -> Result<(), String> {
    let user_dir = cache_root.join(steam_id);
    fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join(format!("{button}.json"));

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "records": [] }))
    } else {
        json!({ "records": [] })
    };

    let records = root
        .as_object_mut()
        .and_then(|m| {
            m.entry("records".to_string())
                .or_insert_with(|| json!([]))
                .as_array_mut()
        })
        .ok_or_else(|| "invalid cache shape".to_string())?;

    let title = song_id.to_string();
    let mut updated = false;
    for rec in records.iter_mut() {
        let Some(obj) = rec.as_object_mut() else {
            continue;
        };
        let title_match = match obj.get("title") {
            Some(Value::String(s)) => s == &title,
            Some(Value::Number(n)) => n.as_i64() == Some(song_id as i64),
            _ => false,
        };
        let pat_match = obj.get("pattern").and_then(|v| v.as_str()) == Some(difficulty);
        if title_match && pat_match {
            obj.insert("score".into(), json!(score));
            obj.insert("maxCombo".into(), json!(is_max_combo));
            updated = true;
            break;
        }
    }
    if !updated {
        records.push(json!({
            "title": title,
            "pattern": difficulty,
            "score": score,
            "maxCombo": is_max_combo,
        }));
    }

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn save_fetched_records_to_cache(
    cache_root: &Path,
    steam_id: &str,
    v_id: &str,
    button: i32,
    data: &Value,
) -> Result<(), String> {
    let user_dir = cache_root.join(steam_id);
    fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join(format!("{button}.json"));

    let records = data.get("records").cloned().unwrap_or_else(|| json!([]));
    let updated_at = data
        .get("user")
        .and_then(|u| u.get("updated_at"))
        .cloned()
        .unwrap_or_else(|| json!(null));

    let cache_data = json!({
        "v_id": v_id,
        "button": button,
        "records": records,
        "updated_at": updated_at,
    });

    let text = serde_json::to_string_pretty(&cache_data).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn delete_varchive_cache_record(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
    song_id: i32,
    difficulty: &str,
) -> Result<(), String> {
    let path = cache_root.join(steam_id).join(format!("{button}.json"));
    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut root = serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?;

    if let Some(obj) = root.as_object_mut() {
        if let Some(records) = obj.get_mut("records").and_then(|r| r.as_array_mut()) {
            let title = song_id.to_string();
            records.retain(|rec| {
                if let Some(rec_obj) = rec.as_object() {
                    let title_match = match rec_obj.get("title") {
                        Some(Value::String(s)) => s == &title,
                        Some(Value::Number(n)) => n.as_i64() == Some(song_id as i64),
                        _ => false,
                    };
                    let pat_match = rec_obj.get("pattern").and_then(|v| v.as_str()) == Some(difficulty);
                    !(title_match && pat_match)
                } else {
                    true
                }
            });
        }
    }

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_cache_file_like_python_client() {
        let dir = std::env::temp_dir().join(format!("varch-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();
        let payload = json!({
            "records": [
                {"title": "42", "pattern": "MX", "score": 99.5, "maxCombo": true}
            ]
        });
        std::fs::write(dir.join("765611").join("4.json"), payload.to_string()).unwrap();

        let m = load_varchive_record_cache(&dir, "765611");
        assert_eq!(m.get(&(42, "4B".into(), "MX".into())), Some(&(99.5, true)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_song_id_zero_from_varchive_cache() {
        let dir = std::env::temp_dir().join(format!("varch-cache-zero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();
        let payload = json!({
            "records": [
                {"title": "0", "pattern": "NM", "score": 88.0, "maxCombo": false}
            ]
        });
        std::fs::write(dir.join("765611").join("4.json"), payload.to_string()).unwrap();

        let m = load_varchive_record_cache(&dir, "765611");
        assert_eq!(m.get(&(0, "4B".into(), "NM".into())), Some(&(88.0, false)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
