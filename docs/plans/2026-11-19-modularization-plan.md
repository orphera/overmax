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
| **0. 기준선** | 현재 `main`에서 감지 117개 테스트셋(`verify_pipeline`)·캡처 지연(P50/P95)·CPU 사용률 측정값 기록. Windows 수동 확인 체크리스트 작성 | 기준 수치 문서화 |
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
