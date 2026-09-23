# Overmax IPC 프로토콜 연동 가이드 (`overmax-ipc/1`)

Overmax의 실시간 게임 상태 및 추천 정보를 외부 애플리케이션(OBS 위젯, 터미널 대시보드, 디스코드 봇, AI 에이전트 등)에서 구독하고 제어하기 위한 공식 연동 규격서입니다.

---

## 1. 개요 (Overview)

* **프로토콜 식별자**: `overmax-ipc/1`
* **기본 주소**: `http://127.0.0.1:30110` (기본 설정: `ipc.enabled = false`)
* **포트 대역**: `30100` ~ `30199` (포트 충돌 시 fallback 자동 바인딩)
* **보안 모델**:
  * Localhost(`127.0.0.1`, `[::1]`) 전용 바인딩
  * `Host` 헤더 검증을 통한 Web Browser 발 DNS Rebinding 공격 원천 차단
* **레퍼런스 예제 코드**: [`examples/ipc_client_demo.py`](../../examples/ipc_client_demo.py)

---

## 2. 엔드포인트 자동 발견 (Endpoint Discovery)

Overmax 실행 시 포트 대역(30100~30199) 중 사용 가능한 포트에 자동 바인딩되며, 바인딩 결과는 아래 파일에 원자적으로 기록됩니다:

* **경로**: `cache/ipc_endpoint.json`
* **형식**:
  ```json
  {
    "protocol": "overmax-ipc/1",
    "host": "127.0.0.1",
    "port": 30110
  }
  ```
  *(IPC가 비활성화된 경우 `"host": null, "port": null` 로 기록됩니다)*

---

## 3. 실시간 이벤트 스트림 (SSE: `GET /events`)

`GET /events`로 HTTP 연결을 수립하면 `text/event-stream` 형식의 Server-Sent Events를 실시간으로 수신합니다.

### 3.1 이벤트 엔벨로프 규격 (Envelope)
```json
{
  "protocol": "overmax-ipc/1",
  "type": "song_detected",
  "seq": 42,
  "ts_ms": 1770000000000,
  "app_version": "0.5.0",
  "payload": { ... }
}
```

### 3.2 이벤트 종류

| 이벤트 명 (`type`) | 발생 시점 | 주요 `payload` 필드 |
| :--- | :--- | :--- |
| **`state_snapshot`** | 클라이언트 최초 접속 즉시 1회 전송 | `scene`, `stable`, `fullscreen`, `context: { song_id, mode, diff, rate, is_max_combo }` |
| **`scene_detected`** | 감지 씬 변경 시 | `scene: "Freestyle" \| "Gameplay" \| "Paused" \| "Unknown" \| "ResultFreestyle" \| ...` |
| **`song_detected`** | 선곡 화면에서 곡/난이도 인식 시 | `song_id`, `mode`, `diff`, `rate`, `is_max_combo`, `title`, `floor_name` |
| **`play_verified`** | 결과 화면에서 실제 완주 판정 확정 시 | `song_id`, `mode`, `diff`, `rate`, `is_max_combo`, `is_pb`, `title`, `floor_name` |
| **`context_updated`** | 실시간 플레이 컨텍스트 갱신 시 | `context: { ... }` |

> 💡 **Keep-Alive Heartbeat**: 15초 동안 이벤트가 없으면 소켓 연결 유지를 위해 `: ping\n\n` 주석 프레임이 자동 전송됩니다.

`state_snapshot`과 `get_current_context`는 같은 IPC 캐시를 사용합니다. 첫 감지 출력 이후 `scene`, `stable`, `fullscreen`은 최신 감지 상태를 반영하고, `context`는 마지막 안정 곡 정보를 보존합니다. 안정 곡 정보가 아직 없으면 `context`는 `null`입니다.

예를 들어 선곡 확정 후 Gameplay에 진입하면 중간 접속한 클라이언트도 `scene="Gameplay"`, `stable=false`를 받습니다. 함께 전달되는 이전 `context`는 현재 플레이 곡이 확정됐다는 뜻이 아닙니다. 곡이 미확정인 동안 context를 교체하지 않으며 새 안정 출력이 오면 갱신합니다. Unknown 전환이나 포커스 상실 시에도 현재 상태와 보존된 context를 구분해야 합니다.

---

## 4. 인바운드 제어 및 데이터 질의 (JSON-RPC 2.0: `POST /rpc`)

`POST /rpc` 엔드포인트를 통해 JSON-RPC 2.0 표준 규격으로 게임 상태를 질의하거나 오버레이를 제어할 수 있습니다.

* **요청 헤더**: `Content-Type: application/json`
* **요청 본문 최대 크기**: 64 KB

### 4.1 지원 메서드 목록

#### 1) `list_methods`
* **설명**: 지원하는 모든 RPC 메서드 목록 조회
* **요청**: `{"jsonrpc": "2.0", "id": 1, "method": "list_methods"}`
* **응답**: `{"jsonrpc": "2.0", "id": 1, "result": {"methods": [...], "protocol": "overmax-ipc/1"}}`

#### 2) `get_current_context`
* **설명**: 최신 감지 씬 상태와 마지막 안정 곡 context 조회. `stable=false`에서는 context가 이전 씬의 값일 수 있음
* **요청**: `{"jsonrpc": "2.0", "id": 2, "method": "get_current_context"}`
* **응답**:
  ```json
  {
    "jsonrpc": "2.0",
    "id": 2,
    "result": {
      "scene": "Freestyle",
      "stable": true,
      "fullscreen": true,
      "context": {
        "song_id": 1234,
        "mode": "4B",
        "diff": "SC",
        "rate": 99.45,
        "is_max_combo": true
      }
    }
  }
  ```

#### 3) `get_recommendations`
* **설명**: 현재 추천 엔진이 계산한 실시간 추천 곡 목록 조회
* **요청**: `{"jsonrpc": "2.0", "id": 3, "method": "get_recommendations"}`

#### 4) `get_song_info`
* **설명**: 곡 ID에 대한 타이틀, 작곡가, DLC 정보 및 전 패턴 난이도/Floor 조회
* **요청**: `{"jsonrpc": "2.0", "id": 4, "method": "get_song_info", "params": [1234]}`

#### 5) `get_recent_plays`
* **설명**: 최근 로컬 플레이 기록 조회
* **요청**: `{"jsonrpc": "2.0", "id": 5, "method": "get_recent_plays", "params": ["4B", 10]}`

#### 6) `set_overlay_visibility`
* **설명**: 인게임 오버레이 창 표시/숨김 토글 제어
* **요청**: `{"jsonrpc": "2.0", "id": 6, "method": "set_overlay_visibility", "params": [false]}`

---

## 5. 빠른 시작 예제 (Python)

```bash
# 별도 pip 패키지 설치 없이 표준 라이브러리만으로 즉시 실행
python examples/ipc_client_demo.py
```
