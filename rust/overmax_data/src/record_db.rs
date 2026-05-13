use rusqlite::{params, Connection, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::fs;

pub struct RecordDB {
    db_path: PathBuf,
    steam_id: Mutex<String>,
    pub is_ready: bool,
}

impl RecordDB {
    const UNKNOWN_STEAM_ID: &'static str = "__unknown__";

    pub fn new(db_path: impl AsRef<Path>, steam_id: Option<&str>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
            steam_id: Mutex::new(Self::normalize_steam_id(steam_id)),
            is_ready: false,
        }
    }

    fn normalize_steam_id(steam_id: Option<&str>) -> String {
        match steam_id {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => Self::UNKNOWN_STEAM_ID.to_string(),
        }
    }

    pub fn masked_steam_id(&self) -> String {
        self.mask_id(&self.get_steam_id())
    }

    fn mask_id(&self, steam_id: &str) -> String {
        if steam_id == Self::UNKNOWN_STEAM_ID {
            return steam_id.to_string();
        }
        if steam_id.len() <= 8 {
            return "***".to_string();
        }
        format!("{}...{}", &steam_id[..4], &steam_id[steam_id.len() - 4..])
    }

    pub fn initialize(&mut self) -> bool {
        if let Some(parent) = self.db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let conn_result = Connection::open(&self.db_path);
        if let Ok(mut conn) = conn_result {
            if self.create_records_table(&conn).is_ok() {
                self.ensure_schema(&mut conn);
                self.is_ready = true;
                return true;
            }
        }
        false
    }

    fn create_records_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                steam_id      TEXT NOT NULL,
                song_id       TEXT NOT NULL,
                button_mode   TEXT NOT NULL,
                difficulty    TEXT NOT NULL,
                rate          REAL NOT NULL,
                is_max_combo  INTEGER NOT NULL DEFAULT 0,
                updated_at    INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
            )",
            [],
        )?;
        Ok(())
    }

    fn table_has_column(&self, conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column_name {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_schema(&self, conn: &mut Connection) {
        if let Ok(has_col) = self.table_has_column(conn, "records", "is_max_combo") {
            if !has_col {
                let _ = conn.execute("DROP TABLE records", []);
                let _ = self.create_records_table(conn);
            }
        }
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let new_sid = Self::normalize_steam_id(steam_id);
        let mut guard = self.steam_id.lock().unwrap();
        let old_sid = guard.clone();
        let changed = old_sid != new_sid;
        *guard = new_sid.clone();
        (changed, self.mask_id(&old_sid), self.mask_id(&new_sid))
    }

    pub fn get_steam_id(&self) -> String {
        self.steam_id.lock().unwrap().clone()
    }

    pub fn upsert(
        &self,
        song_id: i32,
        button_mode: &str,
        difficulty: &str,
        rate: f64,
        is_max_combo: bool,
    ) -> bool {
        if !self.is_ready {
            return false;
        }

        let sid = song_id.to_string();
        let steam_id = self.get_steam_id();
        let is_max_combo_int = if is_max_combo { 1 } else { 0 };

        if let Ok(conn) = Connection::open(&self.db_path) {
            let result = conn.execute(
                "INSERT INTO records (
                    steam_id, song_id, button_mode, difficulty, rate, is_max_combo
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(steam_id, song_id, button_mode, difficulty) DO UPDATE SET
                    rate          = excluded.rate,
                    is_max_combo  = excluded.is_max_combo,
                    updated_at    = CAST(strftime('%s', 'now') AS INTEGER)",
                params![steam_id, sid, button_mode, difficulty, rate, is_max_combo_int],
            );
            return result.is_ok();
        }
        false
    }

    pub fn get(&self, song_id: i32, button_mode: &str, difficulty: &str) -> Option<(f64, bool)> {
        if !self.is_ready {
            return None;
        }

        let steam_id = self.get_steam_id();
        if let Ok(conn) = Connection::open(&self.db_path) {
            let mut stmt = conn.prepare(
                "SELECT rate, is_max_combo FROM records
                 WHERE steam_id=?1 AND song_id=?2 AND button_mode=?3 AND difficulty=?4",
            ).ok()?;
            let result: Result<(f64, i32)> = stmt.query_row(
                params![steam_id, song_id.to_string(), button_mode, difficulty],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            if let Ok((rate, is_max_combo)) = result {
                return Some((rate, is_max_combo != 0));
            }
        }
        None
    }

    pub fn get_rate_map(&self, song_ids: &[i32]) -> std::collections::HashMap<(i32, String, String), (f64, bool)> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, rate, is_max_combo 
             FROM records 
             WHERE steam_id=?1 AND song_id IN ({})", 
            placeholders
        );

        let mut map = std::collections::HashMap::new();
        if let Ok(conn) = Connection::open(&self.db_path) {
            if let Ok(mut stmt) = conn.prepare(&query) {
                let mut p = Vec::new();
                p.push(&steam_id as &dyn rusqlite::ToSql);
                let song_ids_str: Vec<String> = song_ids.iter().map(|s| s.to_string()).collect();
                for id_str in &song_ids_str {
                    p.push(id_str as &dyn rusqlite::ToSql);
                }

                if let Ok(mut rows) = stmt.query(&*p) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (Ok(song_id_str), Ok(button_mode), Ok(difficulty), Ok(rate), Ok(is_max_combo_int)) =
                            (
                                row.get::<_, String>(0),
                                row.get::<_, String>(1),
                                row.get::<_, String>(2),
                                row.get::<_, f64>(3),
                                row.get::<_, i32>(4),
                            )
                        {
                            if let Ok(sid) = song_id_str.parse::<i32>() {
                                map.insert((sid, button_mode, difficulty), (rate, is_max_combo_int != 0));
                            }
                        }
                    }
                }
            }
        }
        map
    }
}
