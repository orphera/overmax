//! IPC 서버 통합 테스트: 실제 소켓을 통해 SSE 스트림과 JSON-RPC를 검증한다.
//!
//! 검증 항목 (docs/plans/2026-08-26-ipc-service-architecture.md):
//! - 대역 바인딩 및 endpoint 파일 기록 (§5.1)
//! - Host 헤더 검증 (§5.4 보호 가드)
//! - SSE named-event 엔벨로프 + 접속 시 state_snapshot 선송신 (§5.3)
//! - JSON-RPC 2.0 get_current_context (§5.4)

use overmax_app::system::ipc_server::{
    spawn_ipc_manager, update_latest_recommendations, update_latest_state, BoundPortSlot,
    IpcDataSources, IpcEvent,
};
use overmax_core::{GameSessionState, PlayContext, SceneType};
use overmax_data::{RecordDB, RecordManager, VArchiveDB};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TEST_PORT: u16 = 30197;

fn test_settings(port: u16, enabled: bool) -> Arc<Mutex<Value>> {
    Arc::new(Mutex::new(json!({
        "ipc": { "enabled": enabled, "port": port }
    })))
}

fn test_data_sources() -> IpcDataSources {
    let dir = tempfile::tempdir().expect("tempdir for db");
    // tempdir은 함수 종료 시 해제되지만 테스트 수명 동안 DB 파일이 유지되면 충분하다.
    // 실제 경로를 유지하기 위해 tempdir을 leak한다 (테스트 전용, 프로세스 종료 시 정리).
    let path = dir.path().to_path_buf();
    std::mem::forget(dir);
    let mut db = RecordDB::new(path.join("test_records.db"), None);
    db.initialize();
    let record_db = Arc::new(db);
    IpcDataSources {
        varchive_db: Arc::new(VArchiveDB::new()),
        record_manager: Arc::new(RecordManager::new(record_db)),
    }
}

fn wait_bound_port(slot: &BoundPortSlot, timeout: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(guard) = slot.lock() {
            if let Some(p) = *guard {
                return Some(p);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn http_get_events(port: u16, host: &str) -> std::io::Result<TcpStream> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    write!(
        s,
        "GET /events HTTP/1.1\r\nHost: {host}\r\nAccept: text/event-stream\r\n\r\n"
    )?;
    Ok(s)
}

fn read_until_data(reader: &mut BufReader<TcpStream>) -> Option<Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() > deadline {
            return None;
        }
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }
        if let Some(data) = line.strip_prefix("data: ") {
            let value: Value = serde_json::from_str(data.trim()).ok()?;
            // 엔벨로프 필수 필드 검증 (§5.3)
            assert_eq!(
                value.get("protocol").and_then(Value::as_str),
                Some("overmax-ipc/1")
            );
            assert!(value.get("seq").and_then(Value::as_u64).is_some());
            assert!(value.get("ts_ms").is_some());
            return Some(value);
        }
    }
}

fn stable_state(song_id: i32) -> GameSessionState {
    GameSessionState {
        scene: SceneType::Freestyle,
        context: Some(PlayContext {
            song_id,
            mode: overmax_core::Mode::B5,
            diff: overmax_core::Difficulty::SC,
            rate: 99.23,
            is_max_combo: false,
        }),
        is_stable: true,
        is_fullscreen: false,
    }
}

#[test]
fn ipc_sse_stream_and_rpc_end_to_end() {
    let root = tempfile::tempdir().expect("tempdir");
    let settings = test_settings(TEST_PORT, true);

    let (ipc_cmd_tx, ipc_cmd_rx) = std::sync::mpsc::channel();
    let (publisher, handle, slot) = spawn_ipc_manager(
        root.path().to_path_buf(),
        settings,
        "test",
        ipc_cmd_tx,
        test_data_sources(),
    );

    // ── 바인딩 대기 + endpoint 파일 검증 ──
    let port =
        wait_bound_port(&slot, Duration::from_secs(5)).expect("server did not bind within timeout");
    assert!(
        (30100..=30199).contains(&port),
        "bound port {port} outside recommended band"
    );

    let endpoint_path = root.path().join("cache").join("ipc_endpoint.json");
    let mut waited = Instant::now();
    let endpoint: Value = loop {
        if let Ok(body) = std::fs::read_to_string(&endpoint_path) {
            break serde_json::from_str(&body).expect("endpoint json parse");
        }
        assert!(
            waited.elapsed() < Duration::from_secs(5),
            "endpoint file missing"
        );
        std::thread::sleep(Duration::from_millis(50));
        waited = Instant::now();
    };
    assert_eq!(
        endpoint.get("port").and_then(Value::as_u64),
        Some(port as u64)
    );

    // Starting the app during gameplay must expose a scene without inventing a song.
    update_latest_state(GameSessionState {
        scene: SceneType::Gameplay,
        is_fullscreen: true,
        ..GameSessionState::detecting()
    });
    assert_snapshot_over_sse_and_rpc(
        port,
        json!({"scene":"Gameplay", "stable":false, "fullscreen":true, "context":null}),
    );

    // ── 안정화 상태 등록 → 이후 접속의 state_snapshot 원천이 됨 ──
    update_latest_state(stable_state(1234));

    // ── SSE 접속: 정상 Host ──
    let stream = http_get_events(port, "127.0.0.1").expect("connect /events");
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("status line");
    assert!(
        status_line.contains("200"),
        "unexpected status: {status_line}"
    );
    let mut saw_event_stream = false;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).expect("header");
        if header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().contains("text/event-stream") {
            saw_event_stream = true;
        }
    }
    assert!(saw_event_stream, "missing Content-Type: text/event-stream");

    // 접속 직후 state_snapshot 선송신 검증
    let first = read_until_data(&mut reader).expect("no first frame");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("state_snapshot"),
        "first event must be state_snapshot"
    );
    let ctx_payload = &first["payload"]["context"];
    assert_eq!(
        ctx_payload.get("song_id").and_then(Value::as_i64),
        Some(1234)
    );

    // 라이브 이벤트 푸시 검증 (scene_detected)
    publisher.publish(IpcEvent::SceneDetected {
        scene: "Freestyle".into(),
    });
    let pushed = read_until_data(&mut reader).expect("no pushed frame");
    assert_eq!(
        pushed.get("type").and_then(Value::as_str),
        Some("scene_detected")
    );
    assert_eq!(
        pushed["payload"]["scene"].as_str(),
        Some("Freestyle"),
        "scene_detected payload must contain scene string"
    );

    // 라이브 이벤트 푸시 검증 (song_detected)
    publisher.publish(IpcEvent::SongDetected {
        song_id: 1234,
        mode: "5B".into(),
        diff: "SC".into(),
        rate: 99.23,
        is_max_combo: false,
        meta: overmax_app::system::ipc_server::SongMeta {
            title: Some("Test Song".into()),
            floor_name: Some("SC 15".into()),
        },
    });
    let song_pushed = read_until_data(&mut reader).expect("no song_detected frame");
    assert_eq!(
        song_pushed.get("type").and_then(Value::as_str),
        Some("song_detected")
    );
    assert_eq!(song_pushed["payload"]["song_id"].as_i64(), Some(1234));
    assert_eq!(song_pushed["payload"]["mode"].as_str(), Some("5B"));
    assert_eq!(song_pushed["payload"]["diff"].as_str(), Some("SC"));
    assert_eq!(song_pushed["payload"]["title"].as_str(), Some("Test Song"));
    assert_eq!(song_pushed["payload"]["floor_name"].as_str(), Some("SC 15"));

    // 라이브 이벤트 푸시 검증 (play_verified)
    publisher.publish(IpcEvent::PlayVerified {
        song_id: 1234,
        mode: "5B".into(),
        diff: "SC".into(),
        rate: 99.50,
        is_max_combo: true,
        is_pb: true,
        meta: overmax_app::system::ipc_server::SongMeta {
            title: Some("Test Song".into()),
            floor_name: Some("SC 15".into()),
        },
    });
    let play_pushed = read_until_data(&mut reader).expect("no play_verified frame");
    assert_eq!(
        play_pushed.get("type").and_then(Value::as_str),
        Some("play_verified")
    );
    assert_eq!(play_pushed["payload"]["song_id"].as_i64(), Some(1234));
    assert_eq!(play_pushed["payload"]["is_max_combo"].as_bool(), Some(true));
    assert_eq!(play_pushed["payload"]["is_pb"].as_bool(), Some(true));

    drop(reader);
    drop(stream);

    // New clients connect after the scene transition, with no scene_detected
    // event to replay. Both snapshot endpoints must return the current scene
    // while retaining only the last stable song context.
    let last_context = first["payload"]["context"].clone();
    for (scene, fullscreen) in [
        (SceneType::Gameplay, true),
        (SceneType::Paused, true),
        (SceneType::Unknown, false),
        (SceneType::Freestyle, false),
    ] {
        let mut state = GameSessionState {
            scene,
            is_fullscreen: fullscreen,
            ..GameSessionState::detecting()
        };
        if scene == SceneType::Freestyle {
            // An unverified selection must not replace the cached song.
            state.context = stable_state(9999).context;
        }
        update_latest_state(state);
        assert_snapshot_over_sse_and_rpc(
            port,
            json!({
                "scene":format!("{scene:?}"), "stable":false,
                "fullscreen":fullscreen, "context":last_context,
            }),
        );
    }

    // A newly stable result replaces all song fields and the current status.
    let mut result = stable_state(4321);
    result.scene = SceneType::ResultFreestyle;
    result.context.as_mut().unwrap().mode = overmax_core::Mode::B8;
    result.context.as_mut().unwrap().rate = 99.5;
    update_latest_state(result);
    assert_snapshot_over_sse_and_rpc(
        port,
        json!({
            "scene":"ResultFreestyle", "stable":true, "fullscreen":false,
            "context":{"song_id":4321, "mode":"8B", "diff":"SC", "rate":99.5, "is_max_combo":false},
        }),
    );
    update_latest_state(stable_state(1234));

    // ── Host 검증 가드: 외부 호칭은 403 ──
    let mut evil = TcpStream::connect(("127.0.0.1", port)).expect("connect evil");
    write!(
        evil,
        "GET /events HTTP/1.1\r\nHost: attacker.example.com\r\n\r\n"
    )
    .unwrap();
    let mut evil_reader = BufReader::new(evil);
    let mut status = String::new();
    evil_reader.read_line(&mut status).unwrap();
    assert!(status.contains("403"), "host guard must reject: {status}");

    // ── JSON-RPC: get_current_context ──
    let mut rpc = TcpStream::connect(("127.0.0.1", port)).expect("connect rpc");
    let body = json!({"jsonrpc":"2.0","id":7,"method":"get_current_context"});
    let payload = body.to_string();
    write!(
        rpc,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )
    .unwrap();
    let mut rpc_reader = BufReader::new(rpc);
    let mut rpc_status = String::new();
    rpc_reader.read_line(&mut rpc_status).unwrap();
    assert!(rpc_status.contains("200"), "rpc failed: {rpc_status}");
    let _rpc_body = String::new();
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        rpc_reader.read_line(&mut line).unwrap();
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut buf = vec![0u8; content_len];
    rpc_reader.read_exact(&mut buf).unwrap();
    let resp: Value = serde_json::from_slice(&buf).unwrap();
    assert_eq!(resp.get("id").and_then(Value::as_i64), Some(7));
    assert_eq!(
        resp["result"]["context"]["song_id"].as_i64(),
        Some(1234),
        "rpc must return latest snapshot context"
    );

    // ── JSON-RPC: get_recommendations (캐시 등록 후 조회) ──
    update_latest_recommendations(json!({"entries": [], "avg_rate": -1.0}));
    let mut rpc2 = TcpStream::connect(("127.0.0.1", port)).expect("connect rpc2");
    let body2 = json!({"jsonrpc":"2.0","id":8,"method":"get_recommendations"});
    let payload2 = body2.to_string();
    write!(
        rpc2,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        payload2.len(),
        payload2
    )
    .unwrap();
    let mut rpc2_reader = BufReader::new(rpc2);
    let mut rpc2_status = String::new();
    rpc2_reader.read_line(&mut rpc2_status).unwrap();
    assert!(rpc2_status.contains("200"), "rpc2 failed: {rpc2_status}");
    let mut content_len2 = 0usize;
    loop {
        let mut line = String::new();
        rpc2_reader.read_line(&mut line).unwrap();
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len2 = v.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut buf2 = vec![0u8; content_len2];
    rpc2_reader.read_exact(&mut buf2).unwrap();
    let resp2: Value = serde_json::from_slice(&buf2).unwrap();
    assert_eq!(resp2.get("id").and_then(Value::as_i64), Some(8));
    assert!(
        resp2["result"].get("entries").is_some(),
        "rpc must return recommendations cache"
    );

    // ── JSON-RPC: set_overlay_visibility → GUI 명령 채널 수신 확인 ──
    let mut rpc3 = TcpStream::connect(("127.0.0.1", port)).expect("connect rpc3");
    let body3 = json!({"jsonrpc":"2.0","id":9,"method":"set_overlay_visibility","params":[false]});
    let payload3 = body3.to_string();
    write!(
        rpc3,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        payload3.len(),
        payload3
    )
    .unwrap();
    let mut rpc3_reader = BufReader::new(rpc3);
    let mut rpc3_status = String::new();
    rpc3_reader.read_line(&mut rpc3_status).unwrap();
    assert!(rpc3_status.contains("200"), "rpc3 failed: {rpc3_status}");
    let mut content_len3 = 0usize;
    loop {
        let mut line = String::new();
        rpc3_reader.read_line(&mut line).unwrap();
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len3 = v.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut buf3 = vec![0u8; content_len3];
    rpc3_reader.read_exact(&mut buf3).unwrap();
    let resp3: Value = serde_json::from_slice(&buf3).unwrap();
    assert_eq!(resp3.get("id").and_then(Value::as_i64), Some(9));
    let cmd = ipc_cmd_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("GUI command not delivered");
    match cmd {
        overmax_app::system::ipc_server::IpcCommand::SetOverlayVisibility(visible) => {
            assert!(!visible, "visibility param must be false");
        }
    }

    // ── Content-Type 강제 가드: text/plain POST는 415 ──
    let mut plain = TcpStream::connect(("127.0.0.1", port)).expect("connect plain");
    write!(
        plain,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}"
    )
    .unwrap();
    let mut plain_reader = BufReader::new(plain);
    let mut plain_status = String::new();
    plain_reader.read_line(&mut plain_status).unwrap();
    assert!(
        plain_status.contains("415"),
        "content-type guard failed: {plain_status}"
    );

    // ── JSON-RPC: get_recent_plays (mode 생략 → 세션 스냅샷에서 추론) ──
    let resp4 = post_rpc(port, 10, "get_recent_plays", json!([null, 5]));
    assert!(
        resp4["result"].get("plays").is_some(),
        "get_recent_plays must return plays array"
    );
    assert!(resp4["result"]["plays"].is_array());

    // ── JSON-RPC: get_song_info (없는 곡은 -32001) ──
    let resp5 = post_rpc(port, 11, "get_song_info", json!([999999]));
    assert!(
        resp5.get("error").is_some(),
        "unknown song must return error"
    );

    // ── 종료 처리: shutdown 플래그 설정이 오류 없이 완료되는지 확인 ──
    handle.shutdown();
}

/// Connect after an update, without relying on a live scene event, then compare RPC.
fn assert_snapshot_over_sse_and_rpc(port: u16, expected: Value) {
    let stream = http_get_events(port, "127.0.0.1").expect("connect after scene transition");
    let mut reader = BufReader::new(stream);
    let snapshot = read_until_data(&mut reader).expect("initial state_snapshot missing");
    assert_eq!(snapshot["type"], "state_snapshot");
    assert_eq!(snapshot["payload"], expected);
    let response = post_rpc(port, 100, "get_current_context", json!([]));
    assert_eq!(response["result"], expected);
}

/// POST /rpc 헬퍼 — 요청 전송 후 JSON 응답 본문을 반환한다.
fn post_rpc(port: u16, id: i64, method: &str, params: Value) -> Value {
    let mut rpc = TcpStream::connect(("127.0.0.1", port)).expect("connect rpc");
    let body = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
    let payload = body.to_string();
    write!(
        rpc,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )
    .unwrap();
    let mut reader = BufReader::new(rpc);
    let mut status = String::new();
    reader.read_line(&mut status).unwrap();
    assert!(
        status.contains("200") || status.contains("500"),
        "{method} unexpected status: {status}"
    );
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut buf = vec![0u8; content_len];
    reader.read_exact(&mut buf).unwrap();
    serde_json::from_slice(&buf).expect("rpc response parse")
}
