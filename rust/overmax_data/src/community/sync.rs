//! Local record vs V-Archive cached API records — mirrors `data/sync_manager.py`.

use crate::community::client::VArchiveDB;
use crate::store::record_db::RecordDB;
use overmax_core::RecordKey;

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
    pub fn key(&self) -> RecordKey {
        (
            self.song_id,
            self.button_mode.clone(),
            self.difficulty.clone(),
        )
    }

    pub fn key_ref(&self) -> (i32, &str, &str) {
        (self.song_id, &self.button_mode, &self.difficulty)
    }

    pub fn matches_key(&self, key: &RecordKey) -> bool {
        self.song_id == key.0 && self.button_mode == key.1 && self.difficulty == key.2
    }

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
        if self.overmax_rate >= 100.0 {
            parts.push("P".to_string());
        } else if self.overmax_mc && !self.varchive_mc.unwrap_or(false) {
            parts.push("M".to_string());
        }
        parts.join(" · ")
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

/// Builds sync candidates for one Steam id using SQL LEFT JOIN on local `record.db`.
pub fn build_candidates(
    varchive_db: &VArchiveDB,
    record_db: &RecordDB,
    steam_id: &str,
) -> Vec<SyncCandidate> {
    let raw_rows = record_db.query_sync_candidates(steam_id);
    if raw_rows.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::with_capacity(raw_rows.len());

    for row in raw_rows {
        let (song_name, composer, dlc) = match varchive_db.search_by_id(row.song_id) {
            Some(s) => (
                s.name.clone(),
                s.composer.to_string(),
                s.dlc_code.to_string(),
            ),
            None => (row.song_id.to_string(), String::new(), String::new()),
        };

        candidates.push(SyncCandidate {
            song_id: row.song_id,
            song_name,
            composer,
            dlc,
            button_mode: row.button_mode,
            difficulty: row.difficulty,
            overmax_rate: row.local_rate,
            overmax_mc: row.local_mc,
            varchive_rate: row.varchive_rate,
            varchive_mc: row.varchive_mc,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_candidates_filters_already_synced_records_from_sqlite_db() {
        let dir = std::env::temp_dir().join(format!("varch-build-cand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("record.db");
        let mut rdb = RecordDB::new(&db_path, Some("76561198000000000"));
        assert!(rdb.initialize());

        // 1. Insert local records into record.db
        rdb.upsert(1, "4B", "MX", 99.5, true, false); // Already synced identical score
        rdb.upsert(2, "4B", "MX", 98.0, false, false); // Local is 98.0%, V-Archive has 95.0% (Improved!)
        rdb.upsert(3, "4B", "MX", 99.0, false, false); // Unregistered on V-Archive

        // 2. Insert V-Archive records into SQLite table
        let v_payload = json!({
            "records": [
                {"title": "1", "pattern": "MX", "score": 99.5, "maxCombo": true},
                {"title": "2", "pattern": "MX", "score": 95.0, "maxCombo": false}
            ]
        });
        rdb.merge_varchive_fetched_records("76561198000000000", 4, &v_payload, false)
            .unwrap();

        let vdb = VArchiveDB::new();
        let candidates = build_candidates(&vdb, &rdb, "76561198000000000");

        // Candidate 1 (synced) should be FILTERED OUT!
        // Candidate 2 (improved score) and Candidate 3 (unregistered) should be PRESENT!
        assert_eq!(candidates.len(), 2);
        let ids: Vec<i32> = candidates.iter().map(|c| c.song_id).collect();
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&1));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
