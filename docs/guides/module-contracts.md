# 모듈 계약 가이드 (기여자용)

> Overmax의 기능을 `overmax_app` 밖에서 가져다 쓰거나, 환경별 구현(예: Linux 컴포지터 위치 소스)을 붙이려는 기여자를 위한 계약 문서다.
> 배경: [TASKS.md](../../TASKS.md) 11번, [2026-10-28-app-boundary-inventory.md](../plans/2026-10-28-app-boundary-inventory.md), [2026-10-28-linux-window-geometry-sources.md](../plans/2026-10-28-linux-window-geometry-sources.md)

---

## 1. 계층과 의존 규칙

```
overmax_app        표현 계층 (egui View/Settings/트레이/다이얼로그) + 앱 프로세스 관리(업데이터·단일 인스턴스)
   │         └──────────────┐
overmax_runtime             overmax_engine     감지 결과 분배·V-Archive 작업·추천 계산·IPC  /  감지 워커(캡처+감지 조립)·자켓 인덱스 어댑터
   │                        │   ├── overmax_capture   캡처 엔진·창 추적·Portal
   │                        │   ├── overmax_detect    감지 파이프라인·ROI·PlayState·템플릿 (`JacketIndex` trait)
   │                        │   └── overmax_frame     공유 프레임 타입·아틀라스 슬롯 표 (capture·detect가 함께 의존)
   │                        │
overmax_data       저장·설정·외부 API·추천 엔진
   │
overmax_core / overmax_cv   공통 타입 · CV 알고리즘
```

- `overmax_detect`·`overmax_capture`·`overmax_frame`은 `overmax_data`에 의존하지 않는다. data와 감지를 잇는 것은 `overmax_engine`의 `ImageIndexJacket`(`JacketIndex` 구현)과 워커다. 각 항목은 그 항목을 소유한 crate의 루트에서 가져온다 (`overmax_frame::CapturedFrame`, `overmax_detect::RoiManager`, `overmax_capture::WindowTracker`). crate 안의 모듈은 비공개이며, 예외는 `overmax_detect::templates`(템플릿 판독 함수 묶음)와 Windows 전용 `overmax_capture::hdr_pipeline`이다.

- `overmax_runtime`은 감지 출력을 `overmax_core` 타입(`GameSessionState`, `VerifiedPlayEvent`)으로 받으므로 `overmax_engine`에 의존하지 않는다. 다른 감지 소스를 쓰는 프론트엔드도 runtime을 그대로 쓸 수 있다.

- 의존은 위에서 아래로만 흐른다. 아래 crate가 위 crate를 참조하지 않는다.
- `overmax_app` 아래의 crate는 egui/eframe에 의존하지 않는다. 화면 갱신이 필요하면 콜백(`Fn() + Send + Sync`)을 주입받는다.
- 사용자에게 보이는 다국어 문구는 app이 만든다. 아래 crate는 결과를 열거형으로 돌려준다 (예: `UploadOutcome`).
- Windows 동작을 바꾸는 기여는 별도 논의 대상이다. 확장 지점은 기본 구현이 현재 동작을 그대로 유지해야 한다.

---

## 2. 공개 API

| crate · 모듈 | 항목 | 용도 |
|:---|:---|:---|
| `overmax_runtime::hub` | `Runtime`, `RuntimeUpdate`, `RuntimeObserver`, `RecordCommit` | 감지 출력 1건을 기록 저장과 관찰자로 분배 |
| `overmax_runtime::ipc_observer` | `IpcObserver` | 허브 갱신을 IPC SSE 이벤트로 발행 ([IPC 프로토콜](ipc-protocol-guide.md)) |
| `overmax_runtime::ipc_server`, `transport` | `spawn_ipc_manager` 등 | 로컬 IPC 서버 (SSE + JSON-RPC) |
| `overmax_runtime::derived` | `recommend_for_state` | 감지 상태에 대한 추천 목록 계산 |
| `overmax_runtime::upload_plan` | `pattern_needs_upload`, `prepare_pattern_upload` | V-Archive 업로드 필요 판단·후보 생성 |
| `overmax_runtime::varchive_jobs` | `spawn_scan`, `spawn_fetch`, `spawn_upload`, `UploadOutcome` | V-Archive 백그라운드 작업 |
| `overmax_runtime::auto_upload` | `AutoUploadScheduler` | 자동 업로드 대기·발사 판단 (시간은 호출자가 주입) |
| `overmax_data::config` | `varchive_account`, `steam_session`, `settings_writer` | 계정 설정 조회, Steam 로그인 사용자, 설정 디바운스 저장 |
| `overmax_data::RecordManager` | `try_handle_verified_play`, `try_upsert`, `try_delete` | 저장 실패(`Err`)와 변경 없음(`Ok(false)`)을 구분하는 기록 API |
| `overmax_frame` | `CapturedFrame`, `ImageView`, `ImageRegion`, `RoiRect`, `RawRoiRect`, `WindowRect`, `WindowSnapshot`, `crop_roi`, `region_mean_bgr`, `compute_pixel_checksum`, `make_thumbnail`, `thumbnail_changed`, `mean_abs_diff`, `AtlasSlot`, `ATLAS_SLOTS`, `ATLAS_WIDTH`/`ATLAS_HEIGHT`/`ATLAS_SLOT_COUNT`, `build_virtual_atlas` | 캡처와 감지가 공유하는 프레임·사각형·창 기하 타입, GPU 아틀라스 슬롯 표 |
| `overmax_detect` | `DetectionPipeline<J>`, `DetectionOutput`, `SleepHint`, `JacketMatchStatus`, `JacketIndex`, `JacketHit` | 감지 파이프라인과 자켓 인덱스 경계 (§3.3) |
| `overmax_detect` | `RoiManager`, `GlobalRoiConfig`, `SceneRoiConfig`, `AtlasTranslator` | ROI 표와 아틀라스 좌표 해석 |
| `overmax_detect` | `detect_static_scene`, `check_open_match_badge`, `detect_freestyle_result_colorbar_match`, `detect_button_mode`, `detect_difficulty`, `detect_max_combo`, `MIN_VALID_RATE`, `templates::*` | 단일 프레임 판독 함수 (도구·테스트용) |
| `overmax_detect` | `RuntimeTelemetry`, `PipelineStatsCollector`, `PipelineTelemetrySnapshot`, `DetectionDeliveryTelemetry`, `TimingAggregator`, `format_duration_us`, `current_timestamp_str` | 감지·전달 텔레메트리 |
| `overmax_capture` | `CaptureEngine`, `CaptureErrorAction`, `AdaptiveCaptureEngine`, `PreferredCaptureEngine` (Windows), `hdr_pipeline` (Windows) | 화면 캡처 엔진 |
| `overmax_capture` | `WindowTracker`, `FocusState`, `PresentationObservation`, `SharedPresentationObservation`, `restore_foreground_by_title`, `encode_wide`·`find_hwnd_by_title` (Windows), `FocusObservation`·`FocusSource` (Linux) | 게임 창 추적 |
| `overmax_capture` (Linux) | `GameGeometrySource`, `NoGeometrySource`, `select_geometry_source`, `spawn_game_geometry_source`, `SharedGameGeometry`, `GameGeometry`, `GeometryChangedCallback` | 컴포지터 좌표의 게임 창 위치 소스 (§3.2) |
| `overmax_capture` (Linux, `portal-capture`) | `PortalCapture`, `PortalStatus`, `PortalConnection`, `PortalControl`, `ConnectionStatus`, `Confirmation`, `ResetScope` | XDG Desktop Portal 캡처 |
| `overmax_engine` | `detection_worker` (모듈), `DetectionPipeline` (= `overmax_detect::DetectionPipeline<ImageIndexJacket>`), `ImageIndexJacket` | 캡처+감지를 묶는 워커, data 이미지 인덱스 기반 자켓 인덱스 |

공개 API는 아직 버전 보장이 없다 (`0.x`). 시그니처를 바꾸는 변경은 이 문서와 함께 갱신한다.

---

## 3. 확장 지점 계약

### 3.1 `RuntimeObserver`

```rust
pub trait RuntimeObserver: Send {
    fn on_update(&mut self, update: &RuntimeUpdate<'_>);
}
```

- **읽기 전용**: 기록을 쓰거나 허브 상태를 바꾸지 않는다. 기록 저장은 허브의 고정 순서 단계(`SessionTracker` → `commit_record`)가 끝난 뒤 관찰자가 호출된다.
- **순서 무관**: 다른 관찰자의 실행 순서에 의존하지 않는다.
- **빠르게 반환**: 감지 출력마다 UI 루프에서 호출된다. 네트워크·디스크 I/O는 채널로 넘겨 별도 스레드에서 처리한다 (`IpcObserver`는 `try_send`만 한다).
- 보장되는 값: `is_pb`는 결과창 진입 시점 기록 기준, `event`는 저장 성공 여부와 무관하게 전달, `commit`은 `Updated` / `Unchanged` / `Failed(사유)`.

### 3.2 `GameGeometrySource` (Linux)

```rust
pub trait GameGeometrySource: Send {
    fn name(&self) -> &'static str;
    fn start(self: Box<Self>, title: &str, sink: SharedGameGeometry) -> Result<(), String>;
}
```

- 계약 전문은 [위치 소스 문서 §3](../plans/2026-10-28-linux-window-geometry-sources.md#3-gamegeometrysource-계약). 요약:
  1. 창은 제목(또는 app_id)으로 찾는다. X11 창 ID에 의존하지 않는다.
  2. `start`는 자기 스레드를 띄우고 즉시 반환한다. IPC 이벤트를 구독해 `sink`에 쓴다 (폴링 지양).
  3. `GameGeometry`의 `rect`는 컴포지터 **전역 논리 좌표**, `output`은 `wl_output` 이름이다.
  4. 추적 상실·복수 매칭·보이지 않는 workspace·IPC 끊김이면 `sink`를 `None`으로 비운다 (fail-closed).
- 등록: `select_geometry_source()`에 환경 변수 감지로 분기를 추가한다. 감지되지 않으면 `NoGeometrySource`를 유지한다.
- 오버레이는 창 모드이고 `output`이 현재 대상 출력과 같을 때만 값을 쓴다. 위치 갱신 시 오버레이를 깨우는 경로는 TASKS 12.3에서 추가 예정이다.

### 3.3 `JacketIndex`

```rust
pub trait JacketIndex {
    fn is_ready(&self) -> bool;
    fn similarity_threshold(&self) -> f32;
    fn check_centroid_kernel(&self, data: &[u8], width: usize, height: usize, channels: usize) -> bool;
    fn match_jacket(&self, data: &[u8], width: usize, height: usize, channels: usize) -> Option<JacketHit>;
}
```

- `DetectionPipeline<J: JacketIndex>`가 자켓 ROI(BGRA, `channels` = 4)로 호출한다. 정적 디스패치이며 매 프레임 경로에 있으므로 구현은 할당을 최소화한다.
- `is_ready`가 거짓이면 선곡 화면의 주기적 자켓 매칭(곡 갱신)을 건너뛰고 `DetectionOutput::image_db_ready`로 알린다. 장면 판정 경로는 `is_ready`와 무관하게 `check_centroid_kernel` → `match_jacket`을 호출하므로, 비어 있는 인덱스는 `match_jacket`에서 `None`을 돌려줘야 한다.
- `check_centroid_kernel`은 결과·선곡 화면 판정 전의 사전 게이트다. 판단할 근거(대표 분포)가 없으면 `true`를 돌려준다.
- `match_jacket`은 가장 유사한 후보 하나를 돌려준다. `image_id`는 곡 ID 문자열이며, 파이프라인은 `similarity >= similarity_threshold()`일 때만 장면 판정에 쓴다.
- 기본 구현은 `overmax_engine::ImageIndexJacket` (`overmax_data`의 `ImageIndexDb` + `JacketMatcher`, DB 형식 불변).

---

## 4. 통과해야 하는 테스트

| 구분 | 명령 | 비고 |
|:---|:---|:---|
| 전체 테스트 | `cargo test --workspace --locked` | CI `ci.yml`와 동일. Windows·Linux 모두 |
| 정적 검사 | `cargo clippy --workspace --all-targets --locked` | 경고 0 |
| 포맷 | `cargo fmt --all -- --check` | 기능 변경과 포맷 변경은 커밋을 나눈다 |
| Linux lifecycle 게이트 | `bash .github/scripts/linux-vertical-slice-lifecycle.sh` | Xvfb + Openbox에서 X11 창 추적·캡처 수명 주기 검증. `openbox`, `xvfb`, `xauth` 필요 |

위치 소스 구현은 추가로 다음 단위 테스트를 포함한다. 실제 컴포지터 없이 IPC 응답 픽스처(JSON 등)로 검증한다.

- 정상 응답 → 전역 논리 좌표 rect와 출력 이름 파싱
- 같은 제목 창 2개 이상 → `None`
- 게임 창이 현재 workspace에 없음 → `None`
- 연결 끊김·파싱 실패 → `None` (패닉 없음)

---

## 5. 검증 보고 양식

실환경 검증 결과는 PR 본문이나 이슈에 아래 양식으로 남긴다. 결과는 [위치 소스 문서 §2](../plans/2026-10-28-linux-window-geometry-sources.md#2-환경별-위치-소스) 표의 "검증" 열에 반영한다.

```markdown
### 검증 환경
- 배포판 / 커널:
- 컴포지터 / 버전:            (예: Hyprland 0.4x, niri 25.xx)
- 세션: Wayland + XWayland    (XWayland 버전)
- 출력 구성 / 배율:           (예: DP-1 2560x1440 @1.25, HDMI-A-1 1920x1080 @1.0)
- 게임 실행: Proton 버전 / 창 모드 여부 / 해상도
- Overmax 실행 방식: 터미널 / 데스크톱 엔트리 / Steam 실행 옵션

### 결과
- [ ] 창 모드에서 오버레이가 게임 창 기준으로 스냅됨 (snap 설정: )
- [ ] 게임 창 이동·크기 변경 후 따라감 (지연 체감: )
- [ ] 다른 출력으로 이동 시 대상 출력 전환
- [ ] workspace 전환 / 창 최소화 시 수동 배치로 복귀
- [ ] 위치 소스가 없는 환경에서 기존 동작 유지
- [ ] 인게임 프레임 영향 체감 여부

### 로그
(`[LinuxOverlay]` / `[WindowTracker]` 관련 디버그 로그 발췌)
```

---

## 6. 재사용 가능한 크레이트

Overmax 바깥 프로젝트(다른 리듬게임·다른 게임용 도구, 에이전트가 새로 만드는 프로젝트)에서 가져다 쓸 수 있는 크레이트와 그렇지 않은 크레이트의 구분이다. crates.io에는 배포하지 않으므로 git 의존성으로 쓴다 ([결정 기록](../decisions/workspace_and_reuse.md)).

```toml
[dependencies]
overmax_cv = { git = "https://github.com/orphera/overmax", package = "overmax_cv" }
overmax-frame = { git = "https://github.com/orphera/overmax", package = "overmax-frame" }
overmax-capture = { git = "https://github.com/orphera/overmax", package = "overmax-capture" }
```

| 크레이트 | 용도 | 플랫폼 | DJMAX 의존 | 재사용 |
|:---|:---|:---|:---|:---|
| `overmax_cv` | 이미지 해시(aHash/dHash/pHash), HOG, 그리드 히스토그램, 이진화, 비트마스크 문자 템플릿 매칭 | 전체 (순수 Rust, 외부 의존 없음) | 없음 | ✅ 그대로 사용 가능 |
| `overmax-frame` | BGRA 프레임 버퍼, ROI 사각형·크롭 뷰, 썸네일 변화 감지, 픽셀 체크섬, 창 기하 타입 | 전체 | `atlas_layout` (DJMAX ROI 46개 슬롯 표, `overmax-core::SceneType` 사용)만 해당 | ✅ 아틀라스 항목 제외하고 사용 |
| `overmax-capture` | 게임 창 추적 + 저부하 화면 캡처 (Windows: DXGI/WGC/GDI 적응형, HDR→SDR 복원 / Linux: XComposite·MIT-SHM, XDG Portal) | Windows, Linux | GPU 아틀라스 모드(`set_enable_gpu_atlas`, 기본 꺼짐)만 해당 | ✅ 아틀라스 모드를 끈 채로 사용 |
| `overmax-core` | 게임 상태·기록 도메인 타입 | 전체 | 전부 | ❌ 앱 전용 (`publish = false`) |
| `overmax-detect` | DJMAX 화면 판독 파이프라인 | 전체 | 전부 | ❌ 앱 전용 |
| `overmax-engine` | 캡처+판독 워커 | Windows, Linux | 전부 | ❌ 앱 전용 |
| `overmax-data` | 곡 DB, 기록 DB, V-Archive 연동, 설정 | 전체 | 전부 | ❌ 앱 전용 |
| `overmax-runtime` | 판독 결과 분배, IPC 서버, 자동 업로드 | 전체 | 전부 | ❌ 앱 전용 (외부 연동은 [IPC 프로토콜](ipc-protocol-guide.md) 사용) |
| `overmax-app` | egui 오버레이 앱 | Windows, Linux | 전부 | ❌ 앱 전용 |

- 재사용 크레이트의 API도 `0.x`이며 버전 보장이 없다. 외부에서 쓸 때는 `rev = "<커밋>"`으로 고정한다.
- 각 재사용 크레이트의 범위와 DJMAX 전용 항목은 해당 `lib.rs`의 크레이트 문서(`cargo doc -p <크레이트> --open`)에 적혀 있다.
