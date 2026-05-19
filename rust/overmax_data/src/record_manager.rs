use crate::record_db::RecordDB;
use crate::sync::load_varchive_record_cache;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type RecordKey = (i32, String, String);
type RecordValue = (f64, bool);

pub trait RecordSource {
    fn is_ready(&self) -> bool;
    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue>;
}

pub struct RecordManager {
    record_db: Arc<RecordDB>,
    varchive_cache_root: PathBuf,
    varchive_cache: Mutex<HashMap<RecordKey, RecordValue>>,
}

impl RecordManager {
    pub fn new(record_db: Arc<RecordDB>, varchive_cache_root: impl AsRef<Path>) -> Self {
        Self {
            record_db,
            varchive_cache_root: varchive_cache_root.as_ref().to_path_buf(),
            varchive_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn refresh(&self) {
        let steam_id = self.record_db.get_steam_id();
        let cache = load_varchive_record_cache(&self.varchive_cache_root, &steam_id);
        if let Ok(mut guard) = self.varchive_cache.lock() {
            *guard = cache;
        }
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let result = self.record_db.set_steam_id(steam_id);
        if result.0 {
            self.refresh();
        }
        result
    }

    pub fn delete(&self, song_id: i32, button_mode: &str, difficulty: &str) -> bool {
        if self.record_db.delete(song_id, button_mode, difficulty) {
            let steam_id = self.record_db.get_steam_id();
            if let Ok(mut guard) = self.varchive_cache.lock() {
                guard.remove(&(song_id, button_mode.to_string(), difficulty.to_string()));
            }
            if !steam_id.is_empty() && steam_id != "__unknown__" {
                let btn = button_mode.replace("B", "").parse::<i32>().unwrap_or(4);
                let _ = crate::sync::delete_varchive_cache_record(
                    &self.varchive_cache_root,
                    &steam_id,
                    btn,
                    song_id,
                    difficulty,
                );
            }
            return true;
        }
        false
    }

    fn merge_varchive_cache(&self, result: &mut HashMap<RecordKey, RecordValue>, song_ids: &[i32]) {
        let Ok(cache) = self.varchive_cache.lock() else {
            return;
        };
        for (key, &(v_rate, v_mc)) in cache.iter() {
            if !song_ids.contains(&key.0) {
                continue;
            }
            result
                .entry(key.clone())
                .and_modify(|entry| {
                    entry.0 = entry.0.max(v_rate);
                    entry.1 |= v_mc;
                })
                .or_insert((v_rate, v_mc));
        }
    }
}

impl RecordSource for RecordDB {
    fn is_ready(&self) -> bool {
        self.is_ready
    }

    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue> {
        RecordDB::get_rate_map(self, song_ids)
    }
}

impl RecordSource for RecordManager {
    fn is_ready(&self) -> bool {
        self.record_db.is_ready
    }

    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue> {
        let mut result = self.record_db.get_rate_map(song_ids);
        self.merge_varchive_cache(&mut result, song_ids);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rate_map_merges_local_and_varchive_cache_by_best_rate() {
        let dir = test_dir("record-manager-merge");
        let db_path = dir.join("record.db");
        let cache_root = dir.join("varchive");
        let steam_id = "765611";
        std::fs::create_dir_all(cache_root.join(steam_id)).unwrap();

        let mut db = RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());
        assert!(db.upsert(42, "4B", "MX", 98.0, false));
        write_cache(&cache_root, steam_id);

        let db = Arc::new(db);
        let manager = RecordManager::new(db, &cache_root);
        manager.refresh();

        let map = manager.get_rate_map(&[42, 99]);

        assert_eq!(
            map.get(&(42, "4B".into(), "MX".into())),
            Some(&(99.5, true))
        );
        assert_eq!(
            map.get(&(99, "4B".into(), "SC".into())),
            Some(&(97.0, false))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    fn write_cache(cache_root: &Path, steam_id: &str) {
        let payload = json!({
            "records": [
                {"title": "42", "pattern": "MX", "score": 99.5, "maxCombo": true},
                {"title": "99", "pattern": "SC", "score": 97.0, "maxCombo": false}
            ]
        });
        std::fs::write(
            cache_root.join(steam_id).join("4.json"),
            payload.to_string(),
        )
        .unwrap();
    }

    fn test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
