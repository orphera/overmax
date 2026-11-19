use super::*;
use std::sync::Arc;
use std::thread;

/// 2026-04-24 Python 구현(`95048ec8^`)의 레거시 records 스키마.
/// `is_max_combo` 컬럼만 없고 나머지 컬럼은 현행과 동일하다.
const LEGACY_RECORDS_DDL: &str = "
    CREATE TABLE records (
        steam_id    TEXT NOT NULL,
        song_id     TEXT NOT NULL,
        button_mode TEXT NOT NULL,
        difficulty  TEXT NOT NULL,
        rate        REAL NOT NULL,
        updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
    );";

fn seed_legacy_db(db_path: &Path, rows: usize) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(LEGACY_RECORDS_DDL).unwrap();
    for i in 0..rows {
        conn.execute(
            "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "76561198000000001",
                (1000 + i).to_string(),
                Mode::B4.as_str(),
                Difficulty::MX.as_str(),
                90.0 + i as f64
            ],
        )
        .unwrap();
    }
}

#[test]
fn legacy_records_migration_preserves_existing_rows() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("legacy_record.db");
    const ROWS: usize = 50;
    seed_legacy_db(&db_path, ROWS);

    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize(), "레거시 DB 초기화 실패");

    // 마이그레이션은 성공 응답만 돌려주고 행을 지우면 안 된다.
    let count: i64 = db
        .open_conn()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, ROWS as i64, "레거시 기록이 삭제됨");

    // 기존 행이 조회되고, 추가된 컬럼은 기본값으로 채워진다.
    let rec = db
        .get(1000, Mode::B4, Difficulty::MX)
        .expect("레거시 행 조회 실패");
    assert!((rec.0 - 90.0).abs() < 1e-4, "레거시 값 훼손: {}", rec.0);
    assert!(!rec.1, "is_max_combo 기본값이 0 이어야 함");
}

#[test]
fn legacy_migrated_db_accepts_upsert_with_max_combo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("legacy_upsert.db");
    seed_legacy_db(&db_path, 1);

    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    // 신규 행은 is_max_combo=true 로 기록되어야 한다(기본값 0 에 머무르지 않음).
    assert!(db.upsert(7777, Mode::B6, Difficulty::HD, 99.5, true, false));
    let rec = db
        .get(7777, Mode::B6, Difficulty::HD)
        .expect("신규 행 저장 실패");
    assert!(rec.1, "is_max_combo=true 가 보존되지 않음");
}

/// BUSY 가 계속되면 op 은 정확히 3회 실행되고 마지막 BUSY 를 그대로 돌려준다.
/// (루프 뒤의 `op(&conn)` 은 도달 불가: attempt == 2 의 모든 결과가 루프 안에서 반환된다.)
#[test]
fn with_retry_runs_op_three_times_on_persistent_busy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db = RecordDB::new(temp_dir.path().join("retry.db"), None);

    let mut calls = 0;
    let res: Result<()> = db.with_retry(|_| {
        calls += 1;
        Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5 /* SQLITE_BUSY */),
            None,
        ))
    });

    assert_eq!(calls, 3, "재시도 계약(최대 3회)과 실행 횟수가 다름");
    assert!(matches!(
        res,
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.extended_code == 5
    ));
}

/// 마이그레이션(ALTER TABLE)이 실패하면 초기화도 실패로 보고해야 한다.
/// 과거에는 실패를 삼키고 is_ready=true 가 되어, 이후 is_max_combo 를 쓰는
/// 모든 조회/기록이 "성공" 경로에서 조용히 실패했다.
#[test]
fn initialize_reports_failure_when_migration_cannot_alter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("locked_migration.db");

    // 현행 스키마(테이블·인덱스 전부)를 만든 뒤 is_max_combo 만 제거해
    // 마이그레이션 대상인 레거시 records 형태로 만든다.
    assert!(RecordDB::new(&db_path, None).initialize());
    Connection::open(&db_path)
        .unwrap()
        .execute("ALTER TABLE records DROP COLUMN is_max_combo", [])
        .unwrap();

    // 다른 연결이 쓰기 잠금을 쥐고 있으면 ALTER 는 busy_timeout 후 실패한다.
    // CREATE ... IF NOT EXISTS 는 전부 no-op 이라 잠금 없이 통과한다.
    let locker = Connection::open(&db_path).unwrap();
    locker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let mut db = RecordDB::new(&db_path, None);
    assert!(!db.initialize(), "ALTER 실패를 삼키고 초기화 성공을 보고함");
    assert!(!db.is_ready);

    // 잠금이 풀리면 다시 초기화해 정상적으로 마이그레이션된다.
    locker.execute_batch("ROLLBACK").unwrap();
    assert!(db.initialize());
    assert!(db.is_ready);
    assert!(db.upsert(1, Mode::B4, Difficulty::MX, 99.0, true, false));
    assert!(db.get(1, Mode::B4, Difficulty::MX).unwrap().1);
}

#[test]
fn test_concurrent_record_db_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("concurrent_record.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());
    let db = Arc::new(db);

    let mut handles = Vec::new();
    // 8 concurrent writer threads
    for t_idx in 0..8 {
        let db_clone = db.clone();
        handles.push(thread::spawn(move || {
            for i in 0..20 {
                let song_id = t_idx * 100 + i;
                let success = db_clone.upsert(
                    song_id,
                    Mode::B4,
                    Difficulty::MX,
                    99.50 + (i as f64 * 0.01),
                    true,
                    false,
                );
                assert!(
                    success,
                    "Thread {} failed to upsert song {}",
                    t_idx, song_id
                );
            }
        }));
    }

    // 4 concurrent reader threads
    for _ in 0..4 {
        let db_clone = db.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..20 {
                let map = db_clone.get_rate_map(&[1, 2, 100, 200, 300]);
                let _ = map.len();
                thread::sleep(std::time::Duration::from_millis(1));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let all_rows = db.all_records_for_steam("76561198000000001");
    assert_eq!(all_rows.len(), 8 * 20);
}

#[test]
fn test_get_updated_at_map() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_updated_at.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    assert!(db.upsert(101, Mode::B4, Difficulty::NM, 98.5, false, false));
    assert!(db.upsert(102, Mode::B6, Difficulty::HD, 99.0, true, false));

    let map = db.get_updated_at_map(&[101, 102, 999]);
    assert_eq!(map.len(), 2);
    let updated_101 = map.get(&(101, Mode::B4, Difficulty::NM)).copied().unwrap();
    let updated_102 = map.get(&(102, Mode::B6, Difficulty::HD)).copied().unwrap();
    assert!(updated_101 >= before);
    assert!(updated_102 >= before);
}

#[test]
fn test_get_varchive_top50_summary_and_rating_map() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_varchive_top50.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();
    for i in 1..=55 {
        let json = serde_json::json!({
            "score": 99.0 + (i as f64 * 0.01),
            "maxCombo": true,
            "updatedAt": "2026-08-21T00:00:00.000Z",
            "rating": 100.0 + (i as f64 * 1.0),
        });
        conn.execute(
            "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "76561198000000001",
                i.to_string(),
                "4B",
                "SC",
                json.to_string(),
            ],
        )
        .unwrap();
    }

    let summary = db.get_varchive_top50_summary("76561198000000001", Mode::B4);
    assert_eq!(summary.total_recorded_count, 50);
    assert_eq!(summary.rank_map.len(), 50);
    assert_eq!(
        summary.rank_map.get(&(55, Mode::B4, Difficulty::SC)),
        Some(&1)
    );
    assert_eq!(
        summary.rank_map.get(&(6, Mode::B4, Difficulty::SC)),
        Some(&50)
    );
    assert_eq!(summary.cutoff_rating, 106.0);
    assert_eq!(summary.rank_map.get(&(5, Mode::B4, Difficulty::SC)), None);

    let rating_map = db.get_varchive_rating_map(&[55, 6, 5, 999]);
    assert_eq!(rating_map.len(), 3);
    assert_eq!(
        rating_map.get(&(55, Mode::B4, Difficulty::SC)),
        Some(&155.0)
    );
    assert_eq!(rating_map.get(&(6, Mode::B4, Difficulty::SC)), Some(&106.0));
    assert_eq!(rating_map.get(&(5, Mode::B4, Difficulty::SC)), Some(&105.0));
}

/// 서버가 일시적으로 빈 목록을 주면 전체 조회가 기존 Top-50 캐시를
/// 지워 버리면 안 된다. 빈 배열이면 기존 캐시를 보존하고 정상 종료한다.
#[test]
fn varchive_empty_merge_preserves_existing_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("varchive_empty_merge.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let payload = serde_json::json!({
        "records": [
            {
                "title": "1234",
                "pattern": "SC",
                "score": 99.5,
                "maxCombo": true,
                "updatedAt": "2026-08-21T00:00:00.000Z",
                "rating": 155.0,
            }
        ]
    });
    db.merge_varchive_fetched_records("76561198000000001", 4, &payload, true)
        .unwrap();

    let before = db.get_varchive_rating_map(&[1234]);
    assert_eq!(before.len(), 1, "초기 병합 실패");
    assert_eq!(before.get(&(1234, Mode::B4, Difficulty::SC)), Some(&155.0));

    // 서버가 빈 배열을 반환하는 상황(일시적 장애/레이트 한도 등)
    let empty = serde_json::json!({ "records": [] });
    db.merge_varchive_fetched_records("76561198000000001", 4, &empty, true)
        .unwrap();

    let after = db.get_varchive_rating_map(&[1234]);
    assert_eq!(
        after.len(),
        1,
        "빈 응답으로 캐시가 소실됨(공식 Top-50 랭크/레이팅 전량 손실)"
    );
    assert_eq!(after.get(&(1234, Mode::B4, Difficulty::SC)), Some(&155.0));
}

/// 숫자가 아닌 title 이나 알 수 없는 pattern 은 저장하지 않는다.
/// 저장하면 읽기 경로에서 song_id 0(실존 곡 "비상 ~Stay With Me~")으로 매핑된다.
#[test]
fn varchive_merge_skips_invalid_song_id_and_difficulty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("varchive_invalid.db");
    let steam = "76561198000000001";
    let mut db = RecordDB::new(&db_path, Some(steam));
    assert!(db.initialize());

    let payload = serde_json::json!({
        "records": [
            { "title": "abc", "pattern": "SC", "score": 99.0, "maxCombo": true,
              "updatedAt": "2026-08-21T00:00:00.000Z", "rating": 150.0 },
            { "title": "42", "pattern": "XX", "score": 98.0, "maxCombo": false,
              "updatedAt": "2026-08-21T00:00:00.000Z", "rating": 140.0 },
            { "title": 7, "pattern": "MX", "score": 97.0, "maxCombo": false,
              "updatedAt": "2026-08-21T00:00:00.000Z", "rating": 130.0 }
        ]
    });
    db.merge_varchive_fetched_records(steam, 4, &payload, true)
        .unwrap();

    let stored: Vec<(String, String)> = {
        let conn = db.open_conn().unwrap();
        let mut stmt = conn
            .prepare("SELECT song_id, difficulty FROM varchive_records ORDER BY song_id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(stored, vec![("7".to_string(), "MX".to_string())]);

    let map = db.load_varchive_records(steam).unwrap();
    assert!(
        !map.keys().any(|(sid, _, _)| *sid == 0),
        "잘못된 title 이 song_id 0 으로 매핑됨"
    );
    assert!(map.contains_key(&(7, Mode::B4, Difficulty::MX)));
}

/// 쓰기 검증 이전에 이미 저장된 잘못된 행도 읽기에서 song_id 0 으로
/// 둔갑하면 안 된다(기존 사용자 DB 호환).
#[test]
fn load_varchive_records_skips_stored_unparsable_song_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("varchive_legacy_invalid.db");
    let steam = "76561198000000001";
    let mut db = RecordDB::new(&db_path, Some(steam));
    assert!(db.initialize());

    let raw = r#"{"title":"abc","pattern":"SC","score":99.0,"maxCombo":true,"updatedAt":"2026-08-21T00:00:00.000Z","rating":150.0}"#;
    db.open_conn()
        .unwrap()
        .execute(
            "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
             VALUES (?1, 'abc', ?2, 'SC', ?3)",
            params![steam, Mode::B4.as_str(), raw],
        )
        .unwrap();

    let map = db.load_varchive_records(steam).unwrap();
    assert!(
        !map.contains_key(&(0, Mode::B4, Difficulty::SC)),
        "저장된 잘못된 title 이 song_id 0 으로 매핑됨"
    );
}

/// 비어 있지 않은 갱신은 기존 목록을 대체해야 한다(기존 동작 유지).
#[test]
fn varchive_non_empty_merge_replaces_previous_entries() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("varchive_replace.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let first = serde_json::json!({
        "records": [{
            "title": "111", "pattern": "SC", "score": 98.0,
            "maxCombo": false, "updatedAt": "2026-08-21T00:00:00.000Z",
            "rating": 150.0,
        }]
    });
    db.merge_varchive_fetched_records("76561198000000001", 4, &first, true)
        .unwrap();

    let second = serde_json::json!({
        "records": [{
            "title": "222", "pattern": "SC", "score": 99.0,
            "maxCombo": true, "updatedAt": "2026-08-22T00:00:00.000Z",
            "rating": 151.0,
        }]
    });
    db.merge_varchive_fetched_records("76561198000000001", 4, &second, true)
        .unwrap();

    let map = db.get_varchive_rating_map(&[111, 222]);
    assert!(
        !map.contains_key(&(111, Mode::B4, Difficulty::SC)),
        "전체 조회가 기존 항목을 대체해야 함"
    );
    assert_eq!(map.get(&(222, Mode::B4, Difficulty::SC)), Some(&151.0));
}

#[test]
fn test_get_recent_records() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_recent.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();
    for i in 1..=5 {
        conn.execute(
            "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "76561198000000001",
                i.to_string(),
                "4B",
                "NM",
                98.0 + (i as f64 * 0.2),
                1,
                1000 + i * 10,
            ],
        ).unwrap();
    }

    let recent = db.get_recent_records("76561198000000001", Mode::B4, 3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].song_id, 5);
    assert_eq!(recent[0].updated_at, 1050);
    assert_eq!(recent[1].song_id, 4);
    assert_eq!(recent[2].song_id, 3);
}

#[test]
fn test_play_events_logging_and_deduplication() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_play_events.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let event1 = VerifiedPlayEvent {
        song_id: 100,
        mode: Mode::B4,
        diff: Difficulty::SC,
        rate: 98.5,
        is_max_combo: true,
        is_result_screen: true,
    };

    // 1. 정상 결과창 이벤트 로깅 성공
    assert!(db.insert_play_event(&event1, 1000));

    // 2. 45초 이내 동일 패턴 재진입 (레벨업 팝업 가림 후 복귀) -> 디바운스 차단 (false)
    let dup_event = VerifiedPlayEvent {
        song_id: 100,
        mode: Mode::B4,
        diff: Difficulty::SC,
        rate: 98.5,
        is_max_combo: true,
        is_result_screen: true,
    };
    assert!(!db.insert_play_event(&dup_event, 1010)); // 10초 후 재진입 시도

    // 3. 45초 초과 후 다음 판 플레이 -> 삽입 성공
    let next_event = VerifiedPlayEvent {
        song_id: 100,
        mode: Mode::B4,
        diff: Difficulty::SC,
        rate: 99.0,
        is_max_combo: true,
        is_result_screen: true,
    };
    assert!(db.insert_play_event(&next_event, 1060)); // 60초 후 플레이

    // 4. 선곡 화면 이벤트는 play_events 삽입 거부
    let song_select_event = VerifiedPlayEvent {
        song_id: 101,
        mode: Mode::B4,
        diff: Difficulty::NM,
        rate: 99.9,
        is_max_combo: true,
        is_result_screen: false,
    };
    assert!(!db.insert_play_event(&song_select_event, 1100));

    // 5. MAX COMBO 연출 지연으로 45초 내에 뒤늦게 is_max_combo = true가 들어온 경우: In-place UPDATE 성공
    // 처음 1200초에 MC false로 들어갔다고 가정할 때, 1201초에 MC true로 오면 update 성공
    let pre_mc_event = VerifiedPlayEvent {
        song_id: 200,
        mode: Mode::B4,
        diff: Difficulty::SC,
        rate: 98.0,
        is_max_combo: false,
        is_result_screen: true,
    };
    assert!(db.insert_play_event(&pre_mc_event, 1200));
    let post_mc_event = VerifiedPlayEvent {
        song_id: 200,
        mode: Mode::B4,
        diff: Difficulty::SC,
        rate: 98.0,
        is_max_combo: true,
        is_result_screen: true,
    };
    assert!(db.insert_play_event(&post_mc_event, 1201)); // In-place UPDATE로 true 반환

    // 6. get_recent_records가 play_events에서 최신 상태를 조회하는지 검증 (MC가 1로 갱신됨)
    let recent = db.get_recent_records("76561198000000001", Mode::B4, 10);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].song_id, 200);
    assert!(recent[0].is_max_combo); // MC가 true로 갱신되었음
    assert_eq!(recent[1].song_id, 100);
    assert_eq!(recent[1].updated_at, 1060);
    assert_eq!(recent[2].song_id, 100);
    assert_eq!(recent[2].updated_at, 1000);
}

/// play_events가 limit 미만일 때 records로 보충하되 중복 제거하는 fallback 검증.
///
/// 테스트 시나리오:
/// play_events(실제 완주 로그)만을 100% 조회하며,
/// 선곡창 휠 스크롤로 오염될 수 있는 records 테이블의 허위 레코드는 끌어오지 않음을 검증한다.
#[test]
fn test_recent_records_exclusive_play_events_without_records_pollution() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_fallback.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();

    // records 테이블: 5건 (갱신 시각 기준 정렬)
    // 곡 100(SC)은 play_events에도 존재 → 중복 제거 대상
    for &(sid, diff, rate, ts) in &[
        (100, "SC", 97.5, 900), // play_events와 중복 (스킵 대상)
        (200, "NM", 96.0, 800),
        (201, "NM", 95.5, 700),
        (202, "NM", 95.0, 600),
        (203, "NM", 94.0, 500),
    ] {
        conn.execute(
            "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "76561198000000001",
                sid.to_string(),
                "4B",
                diff,
                rate,
                0,
                ts,
            ],
        )
        .unwrap();
    }

    // play_events 테이블: 2건 (실제 플레이 이력)
    for (sid, diff, rate, ts) in [(100, "SC", 98.5, 2000), (101, "SC", 97.0, 1900)] {
        conn.execute(
            "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "76561198000000001",
                sid.to_string(),
                "4B",
                diff,
                rate,
                0,
                ts,
            ],
        )
        .unwrap();
    }
    drop(conn);

    // play_events 테이블의 실제 완주 기록만 정확하게 반환함을 검증
    // records 테이블(선곡창 휠 스크롤 오염)은 세션 이력으로 끌어오지 않음
    let recent = db.get_recent_records("76561198000000001", Mode::B4, 5);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].song_id, 100);
    assert_eq!(recent[0].updated_at, 2000); // play_events의 played_at
    assert_eq!(recent[1].song_id, 101);
    assert_eq!(recent[1].updated_at, 1900);

    // limit=1일 때 최신 1건만 반환
    let recent_small = db.get_recent_records("76561198000000001", Mode::B4, 1);
    assert_eq!(recent_small.len(), 1);
    assert_eq!(recent_small[0].song_id, 100);

    // play_events가 없는 모드는 0건 반환 (records 테이블의 허위 레코드를 끌어오지 않음)
    let recent_6b = db.get_recent_records("76561198000000001", Mode::B6, 5);
    assert_eq!(recent_6b.len(), 0);
}

#[test]
fn test_song_select_browsing_preserves_records_updated_at() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("record_updated_at.db");
    let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();
    let old_timestamp = 1700000000i64; // 과거 시각

    // 1. 과거 기록 삽입 (rate: 98.5%, is_max_combo: 0, updated_at: 1700000000)
    conn.execute(
        "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            "76561198000000001",
            "100",
            "4B",
            "SC",
            98.5,
            0,
            old_timestamp,
        ],
    )
    .unwrap();
    drop(conn);

    // 2. 선곡창(only_if_improved = false)에서 동일한 값으로 upsert 호출
    let updated = db.upsert(100, Mode::B4, Difficulty::SC, 98.5, false, false);
    assert!(!updated, "동일한 기록 재방문 시 쓰기를 스킵해야 함");

    // 3. updated_at이 과거 시각 그대로 보존되었는지 검증
    let conn = db.open_conn().unwrap();
    let current_ts: i64 = conn
        .query_row(
            "SELECT updated_at FROM records WHERE steam_id = '76561198000000001' AND song_id = '100'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        current_ts, old_timestamp,
        "선곡창 탐색으로 기존 updated_at이 오염되지 않아야 함"
    );

    // 4. 점수가 개선된 경우(98.5 -> 99.0)에는 updated_at이 현재로 갱신됨을 검증
    let improved = db.upsert(100, Mode::B4, Difficulty::SC, 99.0, false, false);
    assert!(improved);

    let new_ts: i64 = conn
        .query_row(
            "SELECT updated_at FROM records WHERE steam_id = '76561198000000001' AND song_id = '100'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        new_ts > old_timestamp,
        "실제 점수 개선 시에는 updated_at이 갱신되어야 함"
    );
}
