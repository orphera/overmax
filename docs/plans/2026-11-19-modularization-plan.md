# 기능 모듈화 계획 (#43)

> 이슈: [orphera/overmax#43](https://github.com/orphera/overmax/issues/43) · TASKS 13.2
> 선행: TASKS 11번 (app 경계 정리 — `overmax_runtime` 분리, [module-contracts.md](../guides/module-contracts.md))
> 상태: **확정 (2026-11-19)** — 단계 0부터 진행

---

## 1. 목표와 제약

**목표 (2026-11-19 소유자 결정)**

1. **감지만 따로**: 프레임 → 곡/모드/난이도/rate 인식을 egui·SQLite·캡처 API 없이 단독 라이브러리로 쓸 수 있게 한다.
2. **플랫폼 분리**: Windows/Linux 캡처·창 추적·오버레이를 플랫폼별 모듈로 나눠, 기여자가 한 플랫폼만 보고 붙일 수 있게 한다.
3. **내부 정리**: 1,000줄 이상 파일을 책임 단위로 쪼갠다.

**범위**: 구조·crate·공개 API는 크게 바꿔도 된다. **사용자가 보는 동작은 바꾸지 않는다.** `settings.user.json`, `record.db`, `image_index.db`, `songs.json` 호환성 유지. 다른 프론트엔드(egui 외 UI)는 이번 목표가 아니다.

**AGENTS.md와의 관계**: "대규모 재작성 금지"의 예외로, 이 작업 자체가 명시적 요청이다. 대신 아래 규칙으로 위험을 묶는다.

- 한 커밋 = 한 종류의 변경. **이동 커밋**(내용 수정 없이 위치·`use`·가시성만)과 **변경 커밋**(시그니처·로직)을 섞지 않는다.
- 각 커밋에서 `cargo test --workspace`, `cargo clippy --all-targets` 통과. Linux 대상은 WSL에서도 빌드.
- 단계(§5)마다 게이트를 통과해야 다음 단계로 간다.
- 인게임 경로(캡처·감지 루프)는 성능 불변: 동적 디스패치·할당·복사를 새로 넣지 않는다. 필요하면 제네릭/정적 디스패치로.

---

## 2. 현재 상태 (2026-11-19)

| crate | 줄 수 | 내용 |
|:---|---:|:---|
| `overmax_core` | 0.4k | 공통 타입 (`GameSessionState`, `PatternRecord`, `Mode` 등) |
| `overmax_cv` | 1.9k | 이진화·세그먼트·템플릿 매칭 알고리즘 |
| `overmax_data` | 13.0k | 설정·기록 DB·이미지 인덱스·자켓 매칭·추천·외부 API |
| `overmax_engine` | 18.5k | **캡처 + 창 추적 + 감지 + 워커 루프**가 한 crate |
| `overmax_runtime` | 2.2k | 감지 결과 분배·V-Archive 작업·IPC |
| `overmax_app` | 15.6k | egui UI + 플랫폼 오버레이 + 앱 프로세스 관리 |

**막히는 지점**

1. **engine 내부 순환**: `capture`가 `detector::atlas_layout`·`RoiRect`를 쓰고(GPU 아틀라스), `detector`가 `capture::frame::CapturedFrame`·`frame_utils`·`window_tracker::WindowRect`를 쓴다. 한 crate 안이라 지금은 문제없지만 그대로는 둘로 못 나눈다.
2. **감지가 data에 묶임**: `DetectionPipeline`이 `overmax_data::JacketMatcher`·`ImageIndexDb`(SQLite)를 직접 들고 있고, 워커가 `Settings`를 읽는다. 감지만 쓰려 해도 data 전체(SQLite·HTTP·추천)가 따라온다.
3. **app이 engine 내부를 깊게 참조**: `detector::telemetry`, `capture::portal::connection::PortalControl`, `atlas_layout::build_virtual_atlas`, `play_state::MIN_VALID_RATE` 등 약 40곳.
4. **플랫폼 코드가 파일 안에 섞임**: `#[cfg]`가 app 75곳, engine 95곳. `detection_worker.rs`는 Linux portal 분기가 함수 곳곳에 있다.
5. **큰 파일**: `linux_layer_overlay.rs` 2,575 · `detection_pipeline.rs` 1,718 · `detection_worker.rs` 1,717 · `i18n.rs` 1,483 · `play_state.rs` 1,355 · `settings_ui.rs` 1,233 · `native_app.rs` 1,185 · `native_app_viewports.rs` 1,133 · `atlas_layout.rs` 1,108 · `capture_engine/linux.rs` 1,047 · `record_db/mod.rs` 1,189 · `settings.rs` 1,017

---

## 3. 목표 구조

```
overmax_app ─────────────┬──────────────┐
  (egui UI, 프로세스 관리)  │              │
  platform/{windows,linux} │              │
                       overmax_runtime   overmax_engine  ← 워커 루프만 (추적→캡처→감지 조립, 설정 반영)
                           │             ┌───┴────────────┐
                       overmax_data      overmax_capture   overmax_detect
                           │             (windows/, linux/) │
                           │                 └──────┬───────┘
                           │                   overmax_frame   ← 프레임·ROI·아틀라스 좌표
                           └──────────┬─────────────┘
                              overmax_core / overmax_cv
```

| crate | 새로/기존 | 담는 것 | 의존 |
|:---|:---|:---|:---|
| `overmax_frame` | 신규 | `CapturedFrame`, `ImageView`/`ImageRegion`/`crop_roi` 등 `frame_utils`, `RoiRect`, `atlas_layout`(슬롯 상수), `WindowRect` | core |
| `overmax_detect` | 신규 | `detector/` 전체 (pipeline, play_state, templates, roi, atlas_translator, hysteresis, telemetry) | frame, cv, core |
| `overmax_capture` | 신규 | `capture_engine`, `window_tracker`, `portal` — `windows/`·`linux/` 하위 모듈로 정리 | frame, core |
| `overmax_engine` | 축소 | `detection_worker` (추적→캡처→감지 루프, 설정 반영, 재연결) | capture, detect, data |
| `overmax_data` | 유지 | 자켓 인덱스 저장소는 그대로. 감지가 요구하는 trait(§4.1) 구현 제공 | core |

- 감지만 쓰는 사람: `overmax_detect` + `overmax_frame` (+ 자켓 인덱스 구현 하나)
- 플랫폼 기여자: `overmax_capture`의 해당 플랫폼 디렉터리 + app `platform/` 해당 디렉터리

---

## 4. 핵심 설계

### 4.1 자켓 매칭 경계

`DetectionPipeline`은 "자켓 이미지 → 곡 ID 후보"만 필요하다. 이를 trait으로 받는다.

```rust
// overmax_detect
pub trait JacketIndex {
    fn find_best(&self, jacket: &ImageView) -> Option<JacketMatch>;
    // + 현재 쓰는 범위만 (search_index_by_hashes 등 실제 호출을 조사해 확정)
}
```

- `overmax_data`가 기존 `JacketMatcher` + `ImageIndexDb`로 구현한다 (DB 형식 불변).
- 파이프라인은 `DetectionPipeline<J: JacketIndex>` 제네릭으로 받아 인게임 경로에 동적 디스패치를 넣지 않는다.
- 대안은 §6-1.

### 4.2 설정 경계

워커가 `overmax_data::Settings`를 직접 읽는 대신, 감지·캡처에 필요한 값만 담은 `DetectConfig`/`CaptureConfig`(engine이 Settings에서 변환)를 받는다. detect/capture는 data를 모른다.

### 4.3 app → 하위 crate 참조 정리

app이 쓰는 engine 내부 경로(약 40곳)를 각 crate의 루트 re-export(`overmax_detect::{RuntimeTelemetry, ...}`)로 모은다. `pub mod` 깊은 경로는 `pub(crate)`로 내려 공개 API를 module-contracts.md에 적는다.

### 4.4 플랫폼 분리 규칙

- 플랫폼 차이는 **디렉터리**로: `capture/{windows,linux}/`, `app/ui/platform/{windows,linux}/`. 파일 안 `#[cfg]` 분기는 모듈 선택(`mod` 선언)에만 둔다.
- 공통 trait(`CaptureEngine`, `WindowTracker`, 오버레이 호스트)을 기준으로 각 플랫폼이 구현한다. 지금 동작하는 구현을 그대로 옮기고, trait은 현재 호출 모양에 맞춘다(새 기능 없음).
- `detection_worker`의 portal 분기는 `CaptureSession` 같은 플랫폼 구현 뒤로 옮긴다.

---

## 5. 단계

각 단계는 여러 커밋으로 나누고, 단계 끝 게이트를 통과해야 다음으로 간다.

| 단계 | 내용 | 게이트 |
|:---|:---|:---|
| **0. 기준선** | 현재 `main`에서 감지 117개 테스트셋(`verify_pipeline`)·캡처 지연(P50/P95)·CPU 사용률 측정값 기록. Windows 수동 확인 체크리스트 작성 | 기준 수치 문서화 ([기준선](2026-11-19-modularization-baseline.md)) |
| **1. engine 내부 순환 끊기** | `frame` 관련 타입·`atlas_layout`·`RoiRect`를 engine 안 `frame` 모듈로 모음 (아직 같은 crate). capture ↔ detector가 서로 대신 `frame`만 보게 함 | 테스트·clippy |
| **2. 감지 ↔ data 분리** | `JacketIndex` trait 도입, `DetectConfig`로 Settings 의존 제거 (변경 커밋). data가 trait 구현 | 117 테스트셋 결과 동일 |
| **3. crate 분리** | `overmax_frame` → `overmax_detect` → `overmax_capture` 순으로 이동 커밋 (내용 불변). engine은 워커만 남음 | 테스트·clippy·WSL 빌드 |
| **4. 공개 API 정리** | 루트 re-export, 깊은 `pub mod` 축소, app 참조 경로 교체, module-contracts.md 갱신 | 문서 = 실제 API |
| **5. 플랫폼 분리** | capture `windows/`·`linux/` 정리, worker portal 분기 이동, app `platform/` 정리 (`linux_layer_overlay.rs` 분할 포함) | Windows 수동 확인, Xvfb+Openbox lifecycle 게이트 |
| **6. 큰 파일 분할** | §2-5 목록을 책임 단위로 (이동 커밋 위주) | 테스트·clippy |
| **7. 최종 검증** | 단계 0과 같은 측정 반복, 수치 비교 | 성능·인식률 기준선 이내 |

- 순서 이유: 순환을 먼저 끊어야(1) crate를 나눌 수 있고(3), 로직 변경(2)은 이동(3) 전에 끝내야 이동 커밋이 순수해진다. 플랫폼·파일 분할(5·6)은 crate가 확정된 뒤에 해야 두 번 옮기지 않는다.
- 단계 5·6은 서로 독립인 파일이 많아 병렬 진행 가능.

---

## 6. 결정 (2026-11-19 확정)

| # | 항목 | 결정 | 이유 |
|:---|:---|:---|:---|
| 1 | 자켓 매칭 위치 | `JacketIndex` trait을 detect에 두고 data가 기존 `JacketMatcher`+`ImageIndexDb`로 구현 | detect에 SQLite가 들어가지 않고, DB 코드 이동이 없음. 제네릭으로 받아 인게임 비용 없음 |
| 2 | `overmax_engine` 이름 | 유지 | 이름 변경은 모든 `use`·문서를 건드리는 별도 변경이라 이번 목표에 기여하지 않음 |
| 3 | 브랜치 운영 | `refactor/modularization`에서 단계별로 진행하고, 단계 게이트 통과 시마다 main에 머지 | 장기 브랜치 충돌 방지, 단계 단위 revert 가능 |
| 4 | `overmax_frame` | 별도 crate | core가 이미지·아틀라스 타입을 알지 않게 유지 |

## 7. 위험

| 위험 | 대응 |
|:---|:---|
| 인게임 성능 회귀 (trait·경계 비용) | 제네릭 정적 디스패치, 단계 0/7 실측 비교 |
| 인식률 회귀 | 단계 2·3·7에서 117 테스트셋 결과 완전 일치 확인 |
| Linux 빌드·동작 깨짐 (개발 환경은 WSL뿐) | 매 커밋 WSL 빌드, 단계 5 Xvfb 게이트, 실환경은 사용자 확인 |
| 이동과 수정이 섞여 리뷰 불가 | 이동 커밋 규칙, 커밋 메시지에 `move-only` 표기 |
| 장기 브랜치 충돌 | 단계 단위로 짧게 머지 (§6-3) |

## 8. 단계별 기록

### 단계 1 — engine 내부 capture ↔ detector 순환 끊기 (2026-11-19)

`overmax_engine` 안에 새 `frame` 모듈을 만들고, capture와 detector가 함께 쓰던 타입을 그쪽으로 옮겼다. 옛 경로(`capture::frame`, `capture::frame_utils`, `detector::atlas_layout`, `detector::roi::RoiRect`, `detector::roi_config::RawRoiRect`, `capture::window_tracker::{WindowRect, WindowSnapshot}`)는 `pub use` re-export로 남겨 `overmax_app`과 외부 테스트는 바뀌지 않았다 (경로 정리는 단계 4).

| 옮긴 것 | 전 → 후 | 이유 |
| :--- | :--- | :--- |
| `CapturedFrame` | `capture/frame.rs` → `frame/captured_frame.rs` | 캡처 출력이자 감지 입력. 양쪽이 공유 |
| `RoiRect`, `RawRoiRect` | `detector/roi.rs`, `detector/roi_config.rs` → `frame/roi_rect.rs` (타입 분리) | `frame_utils::crop_roi`와 GPU 아틀라스가 쓰는 단순 사각형. `RoiManager`·`GlobalRoiConfig`는 detector에 남김 |
| `frame_utils` (`ImageView`, `ImageRegion`, `crop_roi`, `region_mean_bgr` 등) | `capture/frame_utils.rs` → `frame/utils.rs` | 감지 쪽이 주로 쓰는 프레임 연산 |
| 아틀라스 슬롯 표 (`ATLAS_SLOTS` 등, `build_virtual_atlas`) | `detector/atlas_layout.rs` → `frame/atlas_layout.rs` | DXGI GPU 아틀라스(capture)와 아틀라스 해석(detector)이 공유 |
| `WindowRect`, `WindowSnapshot` | `capture/window_tracker.rs` → `frame/window.rs` (타입 분리) | `detection_pipeline`이 필요로 하는 단순 기하 타입. 추적 로직(`WindowTracker` 등)은 capture에 남김 |

남긴 것:

- `detection_worker.rs`는 캡처와 감지를 묶는 조립 지점이라 `capture::capture_engine`, `capture::portal`, `capture::window_tracker`를 계속 직접 쓴다. 단계 3에서 crate를 나눌 때 worker의 위치(runtime 쪽 등)를 다시 정한다.
- `frame/atlas_layout.rs`의 `#[cfg(test)]` 테스트 일부가 `detector::{roi_config, roi, atlas_translator}`를 쓴다 (아틀라스와 ROI 정합성 검사). 제품 코드 의존은 아니지만, 단계 3에서 `overmax_frame`을 별도 crate로 뺄 때 이 테스트들은 detector 쪽으로 옮겨야 한다.

결과: 제품 코드 기준 `capture → detector` 참조 0, `detector → capture` 참조는 `detection_worker.rs`에만 남음. 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0), WSL `cargo clippy -p overmax-engine --all-targets --features telemetry` (경고 0) 통과. WSL에 PipeWire 개발 패키지가 없어 `portal-capture` 기능 빌드는 확인하지 못했다. 단계 게이트: `verify_pipeline` 109건 결과가 단계 0 기준선과 바이트 단위로 같음.

### 단계 2 — 감지 ↔ data 분리 (2026-11-19)

**`JacketIndex` (§4.1)**: 실제 호출부를 조사한 결과 `DetectionPipeline`이 `ImageIndexDb` + `JacketMatcher`에서 쓰는 것은 `is_ready`, `similarity_threshold`, `check_centroid_kernel`, `match_jacket` 네 개뿐이었다. trait은 이 네 개만 담는다 (§4.1 초안의 `find_best(&ImageView)` 대신 기존 시그니처 `(&[u8], width, height, channels)`를 유지해 호출부 변경과 복사를 없앴다).

| 항목 | 위치 | 내용 |
| :--- | :--- | :--- |
| `JacketIndex` trait, `JacketHit` | `detector/jacket_index.rs` | 감지 쪽 경계. `JacketHit { image_id, similarity }`는 `ImageMatch`와 같은 모양 |
| `ImageIndexJacket` | `image_index_jacket.rs` (engine 루트, detector 밖) | `ImageIndexDb` + `JacketMatcher`를 감싼 구현. `ImageMatch`의 `image_id`를 옮겨 담아 추가 할당·복사 없음. DB 형식 불변 |
| `DetectionPipeline<J: JacketIndex = ImageIndexJacket>` | `detector/detection_pipeline.rs` | 정적 디스패치. 장면 판정 헬퍼는 `&impl JacketIndex` |
| `new(ImageIndexDb)`, `replace_image_db` | `image_index_jacket.rs`의 `impl DetectionPipeline<ImageIndexJacket>` | 기존 시그니처 유지 → 워커·`overmax_app` 변경 없음 |

- 구현 위치: 결정 1은 "data가 구현"이지만, 지금은 engine이 data에 의존하므로 data가 engine의 trait을 구현하면 순환이 된다. 그래서 단계 2에서는 engine 루트의 어댑터가 구현하고, 단계 3에서 `overmax_detect` crate가 생기면 data(또는 runtime) 쪽으로 옮긴다.
- 기본 타입 매개변수(`= ImageIndexJacket`)는 `overmax_app`이 `DetectionPipeline`을 매개변수 없이 쓰기 때문에 두었다. detector가 `crate::image_index_jacket`을 이름으로 참조하는 유일한 지점이며, 단계 3/4에서 app이 구체 타입을 명시하게 되면 없앤다.
- `tests/hdr_replay_test.rs`는 `detect_static_scene`에 `JacketMatcher` 대신 어댑터를 넘기도록 바꿨다 (`JacketHit` 필드가 같아 나머지 코드는 그대로).

**`DetectConfig` (§4.2)**: 조사 결과 detector(워커 제외)와 capture 코드는 이미 `overmax_data::Settings`를 읽지 않는다. `Settings`는 `detection_worker.rs`만 읽고, 캡처 엔진에는 아틀라스 여부·HDR 화이트 레벨 같은 값을, 파이프라인에는 `ImageIndexDb`(임계값 포함)를 넘긴다. 실시간 설정 반영도 워커 안에서 끝난다. 따라서 이번 단계에서는 설정 구조체를 새로 만들지 않았다 (쓸 곳 없는 타입 추가 방지). 단계 3에서 워커가 detect crate 밖으로 나가는지에 따라 다시 판단한다.

**결과 (grep)**: `detector/`(워커 제외)와 `frame/`의 제품 코드에서 `overmax_data` import 0. 남은 것은 `detection_pipeline.rs`의 `#[cfg(test)]` 모듈(`ImageIndexDb::new("missing.db")` 픽스처)과 `jacket_index.rs`의 주석뿐이다. 테스트 픽스처는 단계 3에서 crate를 나눌 때 정리한다.

**게이트**: 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0), WSL engine clippy (경고 0, `portal-capture` 제외). `verify_pipeline` 109건 결과 기준선과 바이트 단위 동일. 성능 A/B (같은 PC, 번갈아 실행, release):

| 측정 | 단계 1 (`0d779fd`) | 단계 2 | 판단 |
| :--- | :--- | :--- | :--- |
| `verify_pipeline` 109건 전체 (6회, 첫 회 제외 중앙값) | 3.98 s | 3.89 s | 차이 없음 (노이즈 범위) |
| `measure_breakdown` 자켓 매칭 파이프라인 (3회) | 0.93 / 0.93 / 0.94 ms | 0.94 / 0.94 / 0.95 ms | 차이 없음 (이 벤치는 바뀐 코드를 거치지 않음) |
| `measure_breakdown` 512×512 아틀라스 전송 (3회) | 0.20 / 0.17 / 0.17 ms | 0.16 / 0.23 / 0.24 ms | 노이즈 (실행마다 ±0.05 ms) |

### 단계 3 — crate 분리 (2026-11-19)

`overmax_engine`을 네 crate로 나눴다. 기존 `overmax_engine::{frame, detector, capture}::...` 경로는 모두 re-export로 남아 `overmax_app`·`overmax_runtime`과 engine 통합 테스트는 바뀌지 않았다.

| crate | 내용 | 의존 |
| :--- | :--- | :--- |
| `overmax_frame` | 단계 1의 `frame` 모듈 (`CapturedFrame`, `frame_utils`, `RoiRect`, 아틀라스 슬롯 표, `WindowRect`) | `overmax_core`, `overmax_cv` |
| `overmax_detect` | `detector/` 중 워커를 뺀 전부 + `JacketIndex` trait | `overmax_core`, `overmax_frame`, `overmax_cv` (테스트 전용 `image`) |
| `overmax_capture` | `capture/` (캡처 엔진, 창 추적, Portal) | `overmax_frame`, 플랫폼 의존성 |
| `overmax_engine` | `detection_worker.rs` + `ImageIndexJacket` 어댑터 + re-export | 위 셋, `overmax_data`, `overmax_core` |

결정:

- **`JacketIndex` 구현 위치는 engine**(`image_index_jacket.rs`). data가 구현하려면 data → detect 의존이 생기는데, 저장소 crate가 감지 crate에 의존하는 방향이 되고 data만 쓰는 crate(runtime 등)까지 감지 코드를 끌어오게 된다. engine은 이미 data와 detect를 둘 다 알고 있으므로 어댑터를 두기에 가장 자연스럽다. 결정 1("data가 구현")은 이 사유로 engine 구현으로 바꿨다.
- **`DetectionPipeline`의 기본 타입 매개변수 제거**: detect가 engine 어댑터 이름을 알 수 없다. 대신 engine의 `detector::detection_pipeline::DetectionPipeline`을 `DetectionPipeline<ImageIndexJacket>` 타입 별칭으로 두어 app 코드를 유지했다. 이를 위해 분리 전에 `DetectionPipeline::new`를 제네릭 impl(`impl Into<J>`)로 옮기고, 어댑터는 `From<ImageIndexDb>`를 제공하도록 했다 (inherent impl은 타입의 crate 밖에 둘 수 없음).
- **기능 전달**: `overmax_engine/telemetry` → `overmax-detect/telemetry` + `overmax-capture/telemetry`, `overmax_engine/portal-capture` → `overmax-capture/portal-capture`. Linux 캡처 의존성(memmap2, x11rb, 선택 의존성 pipewire 등)은 capture로 옮겼다.
- **사전 정리 커밋 (이동 전, 동작 불변)**: detector 타입이 필요한 아틀라스 테스트 3개를 detector 쪽으로 이동 / `detection_pipeline` 테스트의 `ImageIndexDb::new("missing.db")` 픽스처를 같은 동작의 `StubJacketIndex`로 교체 (detect 테스트가 data에 의존하지 않도록).
- **가시성만 넓힌 항목**: `frame::utils::mean_abs_diff`, `detect::telemetry::current_timestamp_str`, (Linux) `WindowFocusTelemetry`, `WindowTracker::game_snapshot_with_telemetry` — crate 경계를 넘어 쓰이게 되어 `pub(crate)` → `pub`. 단계 4 공개 API 정리에서 다시 본다.
- `debug_println!` 매크로를 새 crate 루트에 복제했다 (`overmax_data`도 이미 같은 방식).
- Linux Xvfb CI 스크립트(`.github/scripts/linux-vertical-slice-lifecycle.sh`)는 해당 ignored 테스트를 `-p overmax-capture`와 crate 기준 경로로 실행하도록 고쳤다. `build.bat`, `scripts/package-rust.ps1`, `scripts/package-linux.sh`는 `overmax-app` / `overmax-rs`만 참조하므로 변경 없음.

게이트:

- 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0), WSL `cargo clippy -p overmax-engine -p overmax-frame -p overmax-detect -p overmax-capture --all-targets --features overmax-engine/telemetry` (경고 0) 통과. 이동한 Xvfb 테스트 3개는 WSL에서 `-p overmax-capture` 목록에 있는 것을 확인했다.
- **`portal-capture`는 WSL에서 빌드하지 못했다**: `libpipewire-0.3-dev`가 없어 `libspa-sys` 빌드 스크립트가 실패한다 (패키지 설치는 sudo가 필요해 하지 않음). CI의 Linux 작업은 `--features overmax-app/portal-capture`로 테스트하므로 push 후 CI에서 확인된다.
- `verify_pipeline` 109건 결과 기준선과 바이트 단위 동일. `cargo build --release -p overmax-app` 성공.
- 성능 A/B (`e3d2c3a` 대비, 같은 PC, 번갈아 실행, release): `verify_pipeline` 109건 전체 중앙값 3.60 s → 3.59 s (첫 회 제외), `measure_breakdown` 자켓 매칭 0.86 ms → 0.86~0.87 ms, 아틀라스 전송 0.14~0.15 ms → 0.12~0.16 ms. 모두 노이즈 범위.

알게 된 기존 문제 (이번 변경과 무관, 고치지 않음): Linux에서 `--release` + telemetry 기능 없이 테스트를 빌드하면 `window_tracker/linux.rs` 테스트가 `cfg(any(debug_assertions, feature = "telemetry"))`로 빠진 `active_window_membership`을 import해 컴파일되지 않는다. `e3d2c3a`에도 같은 코드가 있다.

### 단계 4 — 공개 API 정리 (2026-11-19)

원칙: 각 항목은 소유 crate의 루트에서 가져오고, crate 안의 모듈은 비공개로 둔다. 실제 사용처(`overmax_app`, engine 워커·테스트·예제, crate 사이 참조)를 조사해 루트 export를 정했다. 전체 목록은 [module-contracts.md §2](../guides/module-contracts.md#2-공개-api).

| crate | 루트 공개 | 비공개로 바꾼 모듈 | 남긴 공개 모듈 |
| :--- | :--- | :--- | :--- |
| `overmax_frame` | 모든 공개 항목 | `atlas_layout`, `captured_frame`, `roi_rect`, `utils`, `window` | 없음 |
| `overmax_detect` | 파이프라인, `JacketIndex`, 판독 함수, ROI 표, 텔레메트리 | `atlas_translator`, `detection_pipeline`, `hysteresis`, `jacket_index`, `play_state`, `roi`, `roi_config`, `telemetry` | `templates` (판독 함수 묶음, 테스트·도구가 이름공간으로 사용) |
| `overmax_capture` | 캡처 엔진, 창 추적, Linux 위치 소스, Portal (Linux+`portal-capture`) | `capture_engine`, `portal`, `window_tracker` | `hdr_pipeline` (Windows, 루트 re-export. HDR 재생 테스트가 함수 묶음으로 사용) |
| `overmax_engine` | `detection_worker` (모듈), `DetectionPipeline` 별칭, `ImageIndexJacket` | `image_index_jacket` | `detection_worker` |

결정:

- 단계 3의 호환 re-export(`overmax_engine::{frame, capture, detector}`, `overmax_capture::{frame, frame_utils}`, `overmax_detect::atlas_layout`·`ATLAS_*`)는 모두 지웠다. `WindowRect`/`WindowSnapshot`은 소유자인 `overmax_frame`에서만 공개한다.
- `overmax_app`은 `overmax_frame`·`overmax_detect`·`overmax_capture`에 직접 의존한다 (기능은 여전히 engine이 전달).
- 워커 파일을 `overmax_engine/src/detector/detection_worker.rs` → `src/detection_worker.rs`로 옮겼다 (`detector/`가 비게 되어). 앱 경로는 `overmax_engine::detection_worker`.
- 모듈을 비공개로 바꾸자 드러난 항목 처리:
  - `GlobalRoiConfig`/`SceneRoiConfig`는 루트에 공개했다 (ROI 표는 감지 라이브러리 사용자가 다루는 설정이고, 숨기면 읽지 않는 `logo` 필드가 dead-code 경고가 된다).
  - Linux 위치 소스 API(`GameGeometrySource`, `NoGeometrySource`, `select_geometry_source`, `GeometryChangedCallback`)는 module-contracts §3.2의 확장 지점이라 루트에 공개했다.
  - Windows `D3d11Normalizer::new`는 호출처가 없는 래퍼(`new_with_format`만 사용)라 별도 커밋으로 지웠다. 동작 변경 없음.
- 호출부 경로 변경은 crate별로 나누지 않고 한 커밋(`99a4b62`)으로 했다. 경로 치환만 있는 기계적 변경이라 crate별로 쪼개도 검증 단위가 달라지지 않는다.
- module-contracts.md: 계층 설명, §2 공개 API 표(새 crate 4개), §3.3 `JacketIndex` 계약을 실제 코드에 맞게 갱신했다.

게이트:

- 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0), WSL `cargo clippy -p overmax-engine -p overmax-frame -p overmax-detect -p overmax-capture -p overmax-app --all-targets --features overmax-engine/telemetry` 통과. 이번부터 WSL 검사에 `overmax-app`을 넣었는데, app에는 Linux에서만 보이는 기존 경고 5건이 있다 (`native_app_viewports.rs` 미사용 `Mutex`·`style_applied`, `native_app.rs` 테스트의 `assert_eq!` 불필요한 참조 2건, `measure_breakdown.rs` 미사용 함수). `870bd11`에도 같은 코드이며 이번 변경과 무관하다.
- `portal-capture` 기능은 WSL에 PipeWire 개발 패키지가 없어 이번에도 빌드하지 못했다. 해당 경로(`overmax_capture` 루트의 Portal export, `portal_capture_ui.rs`, `portal_capture_probe` 예제)는 CI Linux 작업에서 확인된다.
- `verify_pipeline` 109건 결과 기준선과 바이트 단위 동일. `cargo build --release -p overmax-app` 성공. 감지 경로에 변경이 없어 A/B는 생략했다.

알게 된 기존 문제 (고치지 않음): `overmax_app`의 `ui::native_app::tests::upload_outcome_maps_to_channel_message`가 간헐적으로 실패한다. `LOCALE_TEST_LOCK` 없이 현재 언어를 읽는데, 같은 바이너리의 `overlay_header_detail` 테스트가 언어를 바꾸기 때문이다 (재실행하면 통과).

### 단계 5 — 플랫폼 분리 (2026-11-19)

#### 목표 파일 지도

`overmax_capture` (완료, `45ea071`·`d4b38aa`):

| 이전 | 이후 |
| :--- | :--- |
| `capture_engine/windows/{mod,dxgi,gdi,hdr_pipeline,normalizer,shader_bytes}.rs` | `windows/capture.rs`, `windows/capture/*.rs` |
| `window_tracker/windows.rs` | `windows/window_tracker.rs` |
| `capture_engine/linux.rs` | `linux/capture_engine.rs` |
| `window_tracker/linux.rs`, `window_tracker/geometry_source{.rs,/}` | `linux/window_tracker.rs`, `linux/geometry_source{.rs,/}` |
| `portal{.rs,/}` | `linux/portal{.rs,/}` (`portal-capture` 기능) |
| `window_tracker.rs`의 Linux 전용 `FocusSource`·`FocusObservation` | `linux/window_tracker.rs` |

공통으로 남는 것: `capture_engine.rs` (`CaptureEngine` trait, `CaptureErrorAction`), `window_tracker.rs` (`FocusState`, `PresentationObservation`). `#[cfg]`는 `lib.rs`의 `mod`/`pub use`와 `CaptureEngine`의 Windows 전용 설정 메서드 3개(`set_preferred_engine` 등)에만 남는다. 이 3개는 워커가 `Box<dyn CaptureEngine>`에 대고 부르므로, 없애려면 워커 구조가 바뀐다 (아래 워커 결정에 묶음).

`overmax_engine` 워커 (완료, `d0df83b` 이동 · `e3d4742` 구조):

| 이전 | 이후 |
| :--- | :--- |
| `detection_worker.rs` (1,702줄, `#[cfg]` 82개) | `detection_worker/mod.rs` (공통 루프·출력·로그, 약 760줄), `linux.rs` (포커스 정책, `tick_linux`·Portal 경로, Linux 테스트), `windows.rs` (`tick`, `on_window_missing`, WinRT 초기화, 캡처 설정 동기화) |

`overmax_app` (완료, `3a76e30` · `bfa305e` · `3a43e8e`):

| 이전 | 이후 |
| :--- | :--- |
| `ui/platform/windows.rs` | `ui/platform/windows/mod.rs` |
| `ui/tray_icon.rs` | `ui/platform/windows/tray_icon.rs` |
| `ui/platform/linux.rs` | `ui/platform/linux/mod.rs` |
| `ui/portal_capture_ui.rs` | `ui/platform/linux/portal_capture_ui.rs` |
| `ui/linux_layer_overlay.rs` (2,574줄) | `ui/platform/linux/layer_overlay/` — `mod.rs` (스냅샷·핸들·`spawn`/`run`), `foreign_toplevel.rs` (상태 파싱·대상 선택), `output.rs` (출력 기하·배율), `panel.rs` (패널 크기·여백·표시 판단·degraded 그리기), `backend.rs` (`Backend` 상태·렌더 루프), `backend/handlers.rs` (sctk handler·Dispatch 구현), `tests.rs` |
| `native_app_viewports.rs`의 Windows 오버레이 경로 (`render_windows_overlay`, `update_overlay_geometry`, `render_overlay_panel`, `determine_active_state`, `game_hwnd_cached`) | `ui/platform/windows/overlay_host.rs` |
| `native_app_viewports.rs`의 `publish_linux_overlay` | `ui/platform/linux/overlay_host.rs` |

#### 결정

- **워커는 trait 없이 같은 이름의 메서드로 나눈다 (옵션 A, 사용자 결정).** 플랫폼마다 `PlatformState` 구조체 하나(Linux: 창 스냅샷·포커스 관찰·포커스 정책·Portal 연결, Windows: 캐시한 창 ID)와 `impl DetectionWorker` 블록을 두고, `run()`은 `configure_capture`, `sync_capture_settings`, `initial_detecting_output`, `tick_platform`을 부른다. 두 플랫폼이 이 네 메서드를 같은 시그니처로 정의하는 것이 계약이다 (컴파일러가 강제하는 trait이 아님). 이유: 안정화된 `tick_linux`·Portal 코드를 다시 쓰지 않고 그대로 옮길 수 있고, 여기서 Portal 경로를 실행해 볼 수 없어 동작 보존이 우선이다. 프레임마다 지나는 경로에 `dyn`이나 할당은 늘지 않았다 (Portal `Option`을 tick 앞뒤로 꺼냈다 되돌리는 이동만 추가). plan §4.4의 `CaptureSession` trait은 이번 단계에서 만들지 않았다.
- Linux 재연결(1초 대기 후 tracker·캡처 엔진 재생성)과 중단 처리는 `linux::tick_platform` 안으로 옮기고, 루프에는 `TickFlow { Sleep, Skip, Stop }`만 돌려준다. Windows는 항상 `Sleep`이라 `Skip`/`Stop`에는 Windows에서만 `dead_code` 허용을 붙였다.
- `CaptureEngine` trait의 Windows 전용 설정 메서드 3개(`set_preferred_engine`, `set_enable_gpu_atlas`, `set_hdr_sdr_white_level`)에는 `#[cfg]`가 남는다. 워커가 `Box<dyn CaptureEngine>`에 대고 부르기 때문에, 없애려면 워커가 구체 타입을 들고 있어야 한다. 이번 단계 범위 밖으로 두었다.
- `#[cfg]`가 아직 남은 곳 (이동만으로는 없앨 수 없는 것): `detection_worker/mod.rs`의 `WorkerControl.portal`(공개 API)·`spawn`의 Portal 연결, `WindowQueryScheduler`의 Windows 필드, `native_app_viewports.rs`의 디버그 스냅샷·보조 viewport 안 Windows 분기(29개), `native_app.rs`의 Portal 상태. 6단계(큰 파일 분할)에서 다시 본다.
- `layer_overlay`의 foreign-toplevel `Dispatch` 구현은 계획과 달리 `foreign_toplevel.rs`가 아니라 `backend/handlers.rs`에 두었다. `Backend`의 비공개 필드를 쓰므로 `backend`의 하위 모듈이어야 필드를 공개하지 않아도 된다. 형제 모듈끼리 쓰는 항목과 필드에는 `pub(super)`만 붙였다.
- 분할 중 찾은 기존 빌드 오류 2건을 따로 고쳤다. 둘 다 `--features portal-capture`에서만 드러나 지금까지 확인되지 않았다: `d4b38aa` (Portal 모듈이 단계 3 이전 경로 `crate::frame` 사용), `2611fb8` (워커가 단계 4에서 비공개가 된 `overmax_capture::connection` 경로 사용).

#### 게이트

- 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0), WSL `cargo clippy` (engine·frame·detect·capture·app, `telemetry`) 통과. app의 Linux 기존 경고 5건은 그대로.
- 이번 단계부터 WSL에 PipeWire·libclang 개발 패키지와 Xvfb·Openbox가 있다. `--features overmax-engine/portal-capture,overmax-engine/telemetry`로 clippy·테스트(engine 9개, capture), app debug·release 빌드 통과. `linux-vertical-slice-lifecycle.sh` (Xvfb + Openbox, 새 테스트 경로 `linux::window_tracker::tests::…`, `linux::capture_engine::tests::…`) 3건 통과. 분할 전 브랜치 끝(`2611fb8`)에서도 같은 게이트를 먼저 돌려 통과를 확인했다.
- Portal 경로는 빌드와 단위 테스트까지만 확인했다. 실제 xdg-desktop-portal 세션(화면 선택·PipeWire 프레임 수신)은 실행하지 못했다. 이동한 파일: `overmax_capture/src/linux/portal/*`, `overmax_engine/src/detection_worker/linux.rs`의 Portal 분기, `overmax_app/src/ui/platform/linux/portal_capture_ui.rs`.
- `verify_pipeline` 109건 결과 기준선과 바이트 단위 동일. `cargo build --release -p overmax-app` 성공. 감지 경로에 변경이 없어 A/B는 생략했다.

### 단계 6 — 큰 파일 분할 (2026-11-19)

모든 커밋은 이동 커밋이다. `X.rs`는 `X/mod.rs`로 바꾸고, 인라인 `mod tests { … }`는 `X/tests.rs`로 옮겼다. 함수 본문은 바꾸지 않았고, 커밋마다 공백을 뺀 원본과 새 파일 묶음을 비교해 같음을 확인했다.

| 파일 (분할 전 줄 수) | 분할 결과 (제품 코드 기준 최대 파일) |
|:---|:---|
| `overmax_detect/src/detection_pipeline.rs` (1,745) | `mod.rs` 542 · `scene.rs` 389 (씬 판별 자유 함수) · `tests.rs` |
| `overmax_detect/src/play_state.rs` (1,355) | `mod.rs` 816 · `tests.rs` |
| `overmax_data/src/store/record_db/mod.rs` (1,189) | `mod.rs` 524 · `tests.rs` |
| `overmax_cv/src/image.rs` (1,296) | `mod.rs` 922 · `tests.rs` — §2 목록 이후 1,000줄을 넘어 추가 |
| `overmax_capture/src/linux/capture_engine.rs` (1,046) | `mod.rs` 829 · `tests.rs` |
| `overmax_data/src/config/settings.rs` (1,017) | `mod.rs` 815 · `tests.rs` (테스트 뒤의 설정 구조체는 그대로 `mod.rs`) |
| `overmax_app/src/ui/settings_ui.rs` (1,233) | `mod.rs` 203 (컨텍스트·저장·draft 헬퍼) · `general.rs` · `recommend.rs` · `varchive.rs` · `advanced.rs` (탭별) · `tests.rs` |
| `overmax_app/src/ui/native_app.rs` (1,179) | `mod.rs` 671 · `sync.rs` 466 (V-Archive 스캔·업로드·삭제·조회 요청 처리, 자동 업로드·자동 갱신, 두 번째 `impl NativeApp`) · `tests.rs` |
| `overmax_app/src/ui/i18n.rs` (1,483) | `mod.rs` 1,382 · `tests.rs` — **예외** (아래) |

#### 결정

- **`i18n/mod.rs`는 1,000줄을 넘은 채로 둔다.** 본문이 `macro_rules! t` 하나의 번역 표다. 매크로 하나는 여러 파일에 나눌 수 없고, 나누려면 키 영역별 하위 매크로로 위임하는 구조 변경(변경 커밋)이 필요하다. 표 자체는 키 단위로 읽고 고치므로 파일 길이가 기여를 막지 않는다고 판단했다.
- 가시성 변경은 분할에 필요한 만큼만: `parse_static_scene`, 설정 탭 진입 함수 4개, `upload_result_message`(기존 테스트가 사용)가 `pub(super)`. 자식 모듈은 부모의 비공개 항목(`NativeApp` 필드, draft 헬퍼)을 그대로 쓴다.
- `play_state`의 테스트 픽스처 `include_bytes!` 경로는 디렉터리가 한 단계 깊어져 `../test_fixtures/…`로 바뀌었다.
- `settings_ui` 탭 분할 뒤 `advanced.rs`의 `segmented_row`·`CornerRadius`·`Stroke`는 Windows 캡처 엔진 선택에만 쓰여 Linux에서 미사용 import 경고가 났다. 같은 `#[cfg(target_os = "windows")]`로 묶었다 (`ada026f`).

#### 게이트

- 커밋마다 `cargo test --workspace` (249 통과 · 2 ignored), `cargo clippy --all-targets` (경고 0).
- WSL `--features overmax-engine/portal-capture,overmax-engine/telemetry`로 workspace clippy·테스트(297 통과) 통과. clippy 경고 목록을 단계 5 머지 커밋(`7d502fb`)과 비교해 같음을 확인 (app의 기존 경고 5건, 그중 2건은 `native_app/tests.rs`로 위치만 이동). `linux-vertical-slice-lifecycle.sh` 3건 통과.
- 감지 코드는 위치만 바뀌어 `verify_pipeline`·A/B는 단계 7 최종 검증에서 함께 잰다.
