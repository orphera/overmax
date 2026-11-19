//! RecordDB 모듈 진입점 및 데이터 구조체, 코어 CRUD, 커넥션 풀, 플레이 이벤트 로깅 책임.
use overmax_core::{Difficulty, Mode, RecordKey, RecordValue, VerifiedPlayEvent};
use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RawSyncCandidateRow {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub local_rate: f64,
    pub local_mc: bool,
    pub varchive_rate: Option<f64>,
    pub varchive_mc: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VArchiveTop50Summary {
    /// 50위 곡의 레이팅 (50개 미만인 경우 가장 낮은 곡의 레이팅 또는 0.0)
    pub cutoff_rating: f64,
    /// 1위 ~ 50위 곡들의 순위 맵 (RecordKey -> 1-based rank)
    pub rank_map: std::collections::HashMap<RecordKey, usize>,
    /// 1위 ~ 50위 곡들의 레이팅 맵 (RecordKey -> Performance / V-Archive Rating)
    pub rating_map: std::collections::HashMap<RecordKey, f64>,
    /// 1위 ~ 50위 곡들의 정확도 맵 (RecordKey -> Rate %)
    pub rate_map: std::collections::HashMap<RecordKey, f64>,
    /// 모드 내 등록된 유효 레이팅(rating > 0) 곡 수
    pub total_recorded_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecentRecordEntry {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub rate: f64,
    pub is_max_combo: bool,
    pub updated_at: i64,
}

pub struct RecordDB {
    db_path: PathBuf,
    steam_id: Mutex<String>,
    pub is_ready: bool,
}

mod queries;
mod schema;
mod sync;
mod upload_history;

pub use upload_history::{UploadHistoryRecord, UPLOAD_HISTORY_LIMIT};

const NOT_READY: &str = "record DB is not initialized";

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

    /// Internal connection factory ensuring WAL mode, busy timeout, and synchronous settings.
    pub fn open_conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(conn)
    }

    /// Executes an operation with exponential backoff retry for transient SQLITE_BUSY errors.
    pub fn with_retry<T, F>(&self, mut op: F) -> Result<T>
    where
        F: FnMut(&Connection) -> Result<T>,
    {
        let mut backoff = std::time::Duration::from_millis(10);
        for attempt in 0..3 {
            let conn = self.open_conn()?;
            match op(&conn) {
                Ok(val) => return Ok(val),
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if (err.extended_code == 5 /* SQLITE_BUSY */ || err.extended_code == 261 /* SQLITE_BUSY_RECOVERY */ || err.extended_code == 6/* SQLITE_LOCKED */)
                        && attempt < 2 =>
                {
                    std::thread::sleep(backoff);
                    backoff *= 2;
                }
                Err(e) => return Err(e),
            }
        }
        let conn = self.open_conn()?;
        op(&conn)
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let new_sid = Self::normalize_steam_id(steam_id);
        let mut guard = overmax_core::lock_or_recover(&self.steam_id);
        let old_sid = guard.clone();
        let changed = old_sid != new_sid;
        *guard = new_sid.clone();
        (changed, self.mask_id(&old_sid), self.mask_id(&new_sid))
    }

    pub fn get_steam_id(&self) -> String {
        overmax_core::lock_or_recover(&self.steam_id).clone()
    }

    pub fn upsert(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
        rate: f64,
        is_max_combo: bool,
        only_if_improved: bool,
    ) -> bool {
        self.try_upsert(
            song_id,
            button_mode,
            difficulty,
            rate,
            is_max_combo,
            only_if_improved,
        )
        .unwrap_or(false)
    }

    /// `upsert`와 같되 저장 실패(DB 미준비·SQLite 오류)를 `Err`로 구분한다.
    /// `Ok(false)`는 기존 기록과 같거나 개선이 아니어서 쓰지 않았음을 뜻한다.
    pub fn try_upsert(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
        rate: f64,
        is_max_combo: bool,
        only_if_improved: bool,
    ) -> std::result::Result<bool, String> {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return Err(NOT_READY.to_string());
        }

        let sid = song_id.to_string();
        let steam_id = self.get_steam_id();
        let is_max_combo_int = if is_max_combo { 1 } else { 0 };

        let res = self.with_retry(|conn| {
            let mut existing_rate: Option<f64> = None;
            let mut existing_max_combo: Option<i32> = None;

            let query_res = conn.query_row(
                "SELECT rate, is_max_combo FROM records 
                 WHERE steam_id = ?1 AND song_id = ?2 AND button_mode = ?3 AND difficulty = ?4",
                params![steam_id, sid, button_mode, difficulty],
                |row| {
                    let r: Option<f64> = row.get(0).ok();
                    let mc: Option<i32> = row.get(1).ok();
                    Ok((r, mc))
                },
            );

            if let Ok((r, mc)) = query_res {
                existing_rate = r;
                existing_max_combo = mc;
            }

            let mut final_rate = rate;
            let mut final_max_combo = is_max_combo_int;

            if only_if_improved {
                let should_update_rate = existing_rate.is_none_or(|ext_r| rate > ext_r);
                let should_update_combo =
                    existing_max_combo.is_none_or(|ext_mc| is_max_combo_int > ext_mc);

                if !should_update_rate && !should_update_combo {
                    return Ok(false);
                }

                final_rate = existing_rate.map_or(rate, |ext_r| rate.max(ext_r));
                final_max_combo = existing_max_combo
                    .map_or(is_max_combo_int, |ext_mc| is_max_combo_int.max(ext_mc));
            } else {
                // 선곡창(only_if_improved = false) 탐색 시:
                // 기존 기록과 rate 및 is_max_combo가 완전히 동일하다면 updated_at을 오염시키지 않고 즉시 반환
                if let (Some(ext_r), Some(ext_mc)) = (existing_rate, existing_max_combo) {
                    if (ext_r - rate).abs() < 0.001 && ext_mc == is_max_combo_int {
                        return Ok(false);
                    }
                }
            }

            conn.execute(
                "INSERT INTO records (
                    steam_id, song_id, button_mode, difficulty, rate, is_max_combo
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(steam_id, song_id, button_mode, difficulty) DO UPDATE SET
                    rate          = excluded.rate,
                    is_max_combo  = excluded.is_max_combo,
                    updated_at    = CAST(strftime('%s', 'now') AS INTEGER)",
                params![
                    steam_id,
                    sid,
                    button_mode,
                    difficulty,
                    final_rate,
                    final_max_combo
                ],
            )?;
            Ok(true)
        });
        res.map_err(|e| e.to_string())
    }

    pub fn delete(&self, song_id: i32, button_mode: Mode, difficulty: Difficulty) -> bool {
        self.try_delete(song_id, button_mode, difficulty)
            .unwrap_or(false)
    }

    /// `delete`와 같되 삭제 실패를 `Err`로 구분한다. `Ok(false)`는 지울 기록이 없었음을 뜻한다.
    pub fn try_delete(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
    ) -> std::result::Result<bool, String> {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return Err(NOT_READY.to_string());
        }

        let sid = song_id.to_string();
        let steam_id = self.get_steam_id();

        let res = self.with_retry(|conn| {
            let rows = conn.execute(
                "DELETE FROM records WHERE steam_id=?1 AND song_id=?2 AND button_mode=?3 AND difficulty=?4",
                params![steam_id, sid, button_mode, difficulty],
            )?;
            Ok(rows > 0)
        });
        res.map_err(|e| e.to_string())
    }

    pub fn get(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
    ) -> Option<RecordValue> {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return None;
        }

        let steam_id = self.get_steam_id();
        if let Ok(conn) = self.open_conn() {
            let mut stmt = conn
                .prepare(
                    "SELECT rate, is_max_combo FROM records
                 WHERE steam_id=?1 AND song_id=?2 AND button_mode=?3 AND difficulty=?4",
                )
                .ok()?;
            let result: Result<(f64, i32)> = stmt.query_row(
                params![steam_id, song_id.to_string(), button_mode, difficulty],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            if let Ok((rate, is_max_combo)) = result {
                return Some((rate as f32, is_max_combo != 0));
            }
        }
        None
    }

    fn with_rate_map_connection<T>(&self, f: impl FnOnce(&Connection) -> T) -> Option<T> {
        thread_local! {
            static RATE_MAP_CONN: std::cell::RefCell<Option<(PathBuf, Connection)>> =
                const { std::cell::RefCell::new(None) };
        }

        RATE_MAP_CONN.with(|cell| {
            let mut slot = cell.borrow_mut();
            let need_new = match slot.as_ref() {
                Some((path, _)) => path != &self.db_path,
                None => true,
            };
            if need_new {
                let conn = self.open_conn().ok()?;
                *slot = Some((self.db_path.clone(), conn));
            }
            slot.as_ref().map(|(_, conn)| f(conn))
        })
    }

    pub fn get_rate_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, RecordValue> {
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
        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(&query) {
                let mut p = Vec::new();
                p.push(&steam_id as &dyn rusqlite::ToSql);
                let song_ids_str: Vec<String> = song_ids.iter().map(|s| s.to_string()).collect();
                for id_str in &song_ids_str {
                    p.push(id_str as &dyn rusqlite::ToSql);
                }

                if let Ok(mut rows) = stmt.query(&*p) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (
                            Ok(song_id_str),
                            Ok(button_mode),
                            Ok(difficulty),
                            Ok(rate),
                            Ok(is_max_combo_int),
                        ) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, f64>(3),
                            row.get::<_, i32>(4),
                        ) {
                            if let Ok(sid) = song_id_str.parse::<i32>() {
                                if let (Some(m), Some(d)) = (
                                    Mode::from_str(&button_mode),
                                    Difficulty::from_str(&difficulty),
                                ) {
                                    map.insert((sid, m, d), (rate as f32, is_max_combo_int != 0));
                                }
                            }
                        }
                    }
                }
            }
        });
        map
    }

    pub fn get_updated_at_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, i64> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, updated_at
             FROM records
             WHERE steam_id=?1 AND song_id IN ({})",
            placeholders
        );

        let mut map = std::collections::HashMap::new();
        let _ =
            self.with_rate_map_connection(|conn| {
                if let Ok(mut stmt) = conn.prepare(&query) {
                    let mut p = Vec::new();
                    p.push(&steam_id as &dyn rusqlite::ToSql);
                    let song_ids_str: Vec<String> =
                        song_ids.iter().map(|s| s.to_string()).collect();
                    for id_str in &song_ids_str {
                        p.push(id_str as &dyn rusqlite::ToSql);
                    }
                    if let Ok(mut rows) = stmt.query(&*p) {
                        while let Ok(Some(row)) = rows.next() {
                            if let (
                                Ok(song_id_str),
                                Ok(button_mode),
                                Ok(difficulty),
                                Ok(updated_at),
                            ) = (
                                row.get::<_, String>(0),
                                row.get::<_, String>(1),
                                row.get::<_, String>(2),
                                row.get::<_, i64>(3),
                            ) {
                                if let Ok(sid) = song_id_str.parse::<i32>() {
                                    if let (Some(m), Some(d)) = (
                                        Mode::from_str(&button_mode),
                                        Difficulty::from_str(&difficulty),
                                    ) {
                                        map.insert((sid, m, d), updated_at);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        map
    }

    /// 결과창에서 발생한 플레이 이벤트를 45초 디바운스 가드 하에 play_events 테이블에 누적 기록한다.
    pub fn insert_play_event(&self, event: &VerifiedPlayEvent, now_unix: i64) -> bool {
        if !self.is_ready || !event.is_result_screen || event.rate < 0.01 {
            return false;
        }

        let steam_id = self.get_steam_id();
        if steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return false;
        }

        let song_id_str = event.song_id.to_string();
        let button_mode = event.mode.as_str();
        let diff_str = event.diff.as_str();
        let debounce_window = (now_unix - 45).max(0);

        let result = self.with_retry(|conn| {
            // 45초 이내 동일 패턴 디바운스 가드 (레벨업/언락 팝업 재진입 방지)
            // 단, 연출 지연으로 MAX COMBO가 뒤늦게 감지되었거나 Rate가 개선된 경우 기존 행을 즉시 UPDATE한다.
            let mut check_stmt = conn.prepare_cached(
                "SELECT id, rate, is_max_combo FROM play_events
                 WHERE steam_id = ?1 AND song_id = ?2 AND button_mode = ?3 AND difficulty = ?4
                   AND played_at >= ?5
                 ORDER BY played_at DESC
                 LIMIT 1",
            )?;
            let mut rows = check_stmt.query(rusqlite::params![
                steam_id,
                song_id_str,
                button_mode,
                diff_str,
                debounce_window
            ])?;

            if let Ok(Some(row)) = rows.next() {
                let existing_id: i64 = row.get(0)?;
                let existing_rate: f64 = row.get(1)?;
                let existing_mc: i32 = row.get(2)?;
                let new_mc = if event.is_max_combo { 1 } else { 0 };

                let event_rate_f64 = event.rate as f64;
                let mc_improved = existing_mc == 0 && new_mc == 1;
                let rate_improved = event_rate_f64 > existing_rate + 0.001;

                if mc_improved || rate_improved {
                    let final_rate = existing_rate.max(event_rate_f64);
                    let final_mc = existing_mc.max(new_mc);
                    let mut update_stmt = conn.prepare_cached(
                        "UPDATE play_events
                         SET rate = ?1, is_max_combo = ?2
                         WHERE id = ?3",
                    )?;
                    update_stmt.execute(rusqlite::params![final_rate, final_mc, existing_id])?;
                    return Ok(true);
                }

                return Ok(false);
            }

            let mut insert_stmt = conn.prepare_cached(
                "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            insert_stmt.execute(rusqlite::params![
                steam_id,
                song_id_str,
                button_mode,
                diff_str,
                event.rate,
                if event.is_max_combo { 1 } else { 0 },
                now_unix,
            ])?;
            Ok(true)
        });

        result.unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
