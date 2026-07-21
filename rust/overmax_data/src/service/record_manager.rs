use crate::store::record_db::RecordDB;
use overmax_core::{RecordKey, RecordValue};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub trait RecordSource {
    fn is_ready(&self) -> bool;
    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue>;
}

pub struct RecordManager {
    record_db: Arc<RecordDB>,
    varchive_cache: Mutex<HashMap<RecordKey, RecordValue>>,
    data_revision: AtomicU64,
    dirty_record_keys: Mutex<HashSet<RecordKey>>,
    full_dirty: AtomicBool,
}

impl RecordManager {
    pub fn new(record_db: Arc<RecordDB>) -> Self {
        Self {
            record_db,
            varchive_cache: Mutex::new(HashMap::new()),
            data_revision: AtomicU64::new(0),
            dirty_record_keys: Mutex::new(HashSet::new()),
            full_dirty: AtomicBool::new(true),
        }
    }

    pub fn refresh(&self) {
        let steam_id = self.record_db.get_steam_id();
        let cache = self
            .record_db
            .load_varchive_records(&steam_id)
            .unwrap_or_default();
        if let Ok(mut guard) = self.varchive_cache.lock() {
            *guard = cache;
        }
        self.full_dirty.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.dirty_record_keys.lock() {
            guard.clear();
        }
        self.data_revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let result = self.record_db.set_steam_id(steam_id);
        if result.0 {
            self.refresh();
        }
        result
    }

    pub fn upsert(
        &self,
        song_id: i32,
        button_mode: &str,
        difficulty: &str,
        rate: f32,
        is_max_combo: bool,
        only_if_improved: bool,
    ) -> bool {
        if self.record_db.upsert(
            song_id,
            button_mode,
            difficulty,
            rate as f64,
            is_max_combo,
            only_if_improved,
        ) {
            if let Ok(mut guard) = self.dirty_record_keys.lock() {
                guard.insert((song_id, button_mode.to_string(), difficulty.to_string()));
            }
            self.data_revision.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn delete(&self, song_id: i32, button_mode: &str, difficulty: &str) -> bool {
        if self.record_db.delete(song_id, button_mode, difficulty) {
            if let Ok(mut guard) = self.varchive_cache.lock() {
                guard.remove(&(song_id, button_mode.to_string(), difficulty.to_string()));
            }
            if let Ok(mut guard) = self.dirty_record_keys.lock() {
                guard.insert((song_id, button_mode.to_string(), difficulty.to_string()));
            }
            self.data_revision.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn data_revision(&self) -> u64 {
        self.data_revision.load(Ordering::SeqCst)
    }

    pub fn consume_dirty_info(&self) -> (bool, HashSet<RecordKey>) {
        let full_dirty = self.full_dirty.swap(false, Ordering::SeqCst);
        let mut keys = HashSet::new();
        if let Ok(mut guard) = self.dirty_record_keys.lock() {
            std::mem::swap(&mut *guard, &mut keys);
        }
        (full_dirty, keys)
    }

    pub fn get_local_record(
        &self,
        song_id: i32,
        button_mode: &str,
        difficulty: &str,
    ) -> Option<RecordValue> {
        self.record_db.get(song_id, button_mode, difficulty)
    }

    pub fn get_varchive_cache_record(
        &self,
        song_id: i32,
        button_mode: &str,
        difficulty: &str,
    ) -> Option<RecordValue> {
        let guard = overmax_core::lock_or_recover(&self.varchive_cache);
        guard
            .get(&(song_id, button_mode.to_string(), difficulty.to_string()))
            .copied()
    }

    fn merge_varchive_cache(&self, result: &mut HashMap<RecordKey, RecordValue>, song_ids: &[i32]) {
        let cache = overmax_core::lock_or_recover(&self.varchive_cache);
        let song_ids_set: HashSet<i32> = song_ids.iter().copied().collect();
        for (key, &(v_rate, v_mc)) in cache.iter() {
            if !song_ids_set.contains(&key.0) {
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
        assert!(db.upsert(42, "4B", "MX", 98.0, false, false));
        write_cache(&cache_root, steam_id);
        db.migrate_json_cache_to_db(&cache_root).unwrap();

        let db = Arc::new(db);
        let manager = RecordManager::new(db);
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

    #[test]
    fn test_recommendation_caching_and_stats() {
        use crate::community::client::VArchiveDB;
        use crate::service::recommend::Recommender;

        let mut vdb = VArchiveDB::new();
        let song1_json = serde_json::json!({
            "name": "Song A",
            "title": "1",
            "composer": "Artist A",
            "dlcCode": "pack",
            "patterns": {
                "4B": {
                    "MX": {
                        "level": 15,
                        "floorName": "15.0"
                    }
                }
            }
        });
        let song2_json = serde_json::json!({
            "name": "Song B",
            "title": "2",
            "composer": "Artist B",
            "dlcCode": "pack",
            "patterns": {
                "4B": {
                    "MX": {
                        "level": 15,
                        "floorName": "15.0"
                    }
                }
            }
        });
        vdb.songs = vec![
            serde_json::from_value(song1_json).unwrap(),
            serde_json::from_value(song2_json).unwrap(),
        ];

        let dir = test_dir("recommend-stats-cache");
        let db_path = dir.join("record.db");
        let mut db = RecordDB::new(&db_path, None);
        assert!(db.initialize());

        assert!(db.upsert(1, "4B", "MX", 99.0, false, false));
        assert!(db.upsert(2, "4B", "MX", 97.0, false, false));

        let record_db = Arc::new(db);
        let record_manager = Arc::new(RecordManager::new(record_db));
        record_manager.refresh();

        let recommender = Recommender::new(Arc::new(vdb), record_manager);

        let result = recommender.recommend(1, "4B", "MX", 0.1, 10, true);

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].song_id, 2);

        assert_eq!(result.total_count, 2);
        assert_eq!(result.has_record_count, 2);
        assert_eq!(result.avg_rate, 98.0);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_record_manager_helpers() {
        let dir = test_dir("record-manager-helpers");
        let db_path = dir.join("record.db");
        let cache_root = dir.join("varchive");
        let steam_id = "765611";
        std::fs::create_dir_all(cache_root.join(steam_id)).unwrap();

        let mut db = RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());
        assert!(db.upsert(123, "5B", "SC", 99.80, true, false));
        write_cache(&cache_root, steam_id); // Writes MX/SC cache: MX=99.5, SC=97.0 for song 42/99
        db.migrate_json_cache_to_db(&cache_root).unwrap();

        let db = Arc::new(db);
        let manager = RecordManager::new(db);
        manager.refresh();

        // 1. Verify get_local_record
        assert_eq!(
            manager.get_local_record(123, "5B", "SC"),
            Some((99.80, true))
        );
        assert_eq!(manager.get_local_record(999, "4B", "NM"), None);

        // 2. Verify get_varchive_cache_record
        // Write cache has MX 99.5 for song 42
        assert_eq!(
            manager.get_varchive_cache_record(42, "4B", "MX"),
            Some((99.5, true))
        );
        assert_eq!(manager.get_varchive_cache_record(42, "4B", "NM"), None);

        let _ = std::fs::remove_dir_all(dir);
    }
}
