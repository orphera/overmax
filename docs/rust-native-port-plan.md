# Rust Native Port Plan

이 문서는 Overmax를 Python/PyQt6 앱에서 Rust 네이티브 앱으로 전환하기 위한
기준 계획이다. 기존 Python 앱은 포팅 완료 전까지 reference implementation으로
유지한다.

## 목표

- Rust 네이티브 앱으로 전체 런타임 전환
- 화면 캡처 기반 처리 유지
- verified pipeline 동작 유지
- 기존 사용자 파일과 DB 호환 유지
- 인게임 성능 영향 최소화

## 기준 스택

- UI / window loop: `egui/winit`
- Core state / detection contracts: `overmax-core`
- V-Archive, record DB, sync data contracts: `overmax-data`
- Image feature backend: 기존 `rust/overmax_cv`
- App entrypoint / Windows integration: `overmax-app`

## 호환 정책

Rust 앱은 다음 파일과 흐름을 기존 형식 그대로 읽고 써야 한다.

- `settings.json`
- `settings.user.json`
- `cache/record.db`
- `cache/image_index.db`
- `cache/songs.json`
- GitHub Releases `overmax.zip`
- GitHub Releases `release_manifest.json`
- `cache/update/stage` 업데이트 staging 흐름

## Verified Pipeline Invariants

- `is_stable = true`일 때만 상태를 commit한다.
- detection -> verification -> commit 흐름을 유지한다.
- 단일 프레임 결과에 의존하지 않는다.
- 동일 `(song_id, mode, diff)` 조합 Rate 수집은 중복 저장하지 않는다.
- `rate == 0.0`은 저장하지 않는다.
- `rate == None`과 `rate == 0.0`은 서로 다른 의미다.
- `song_id == 0`은 유효한 ID일 수 있으므로 없음은 `Option`/`None`으로만 표현한다.
- image DB 추가/삭제는 overlay 프로그램 런타임에서 수행하지 않는다.
- 모든 사용자 설정 변경은 `settings.user.json`에 저장한다.
- 설정 저장 시 유효 범위 normalize/clamp/snap을 적용한다.

## Phase 0: Workspace Foundation

- Rust workspace를 repo root에 둔다.
- 기존 PyO3 backend인 `rust/overmax_cv`는 workspace member로 유지한다.
- `overmax-core`, `overmax-data`, `overmax-app`를 추가한다.
- 비-Windows 실행은 명확한 unsupported 에러를 반환한다.

## Phase 1: Core Equivalence

- `GameSessionState`를 Rust 타입으로 포팅한다.
- Python 기준 fixture와 Rust 결과를 비교하는 golden test를 만든다.
- `song_id == 0`, `rate == None`, `rate == 0.0` 경계를 우선 검증한다.

## Phase 2: Data Compatibility

- settings merge/delta save를 Rust에서 재현한다.
- `record.db`와 `image_index.db` schema를 그대로 읽는다.
- V-Archive songs JSON과 floor 기반 추천 결과를 Python 기준과 비교한다.

## Phase 3: Runtime Pipeline

- Win32 window tracking과 capture를 Rust로 구현한다.
- ROI 변환, hysteresis, play-state detection, OCR 연동을 순서대로 포팅한다.
- 각 단계는 Python reference와 fixture 기반 결과가 일치해야 프로덕션 경로로 연결한다.

## Phase 4: Native UI and Release

- `egui/winit` overlay를 구현한다.
- 설정, 동기화, 디버그 창을 Rust UI로 구현한다.
- updater와 packaging을 Rust 앱 기준으로 옮긴다.
- 한 릴리스 동안 Python 앱을 fallback/reference로 유지한 뒤 제거한다.

## Acceptance Tests

- Core state golden tests
- Settings normalize/delta save tests
- SQLite schema compatibility tests
- Image DB top-1 regression: `795/795`
- OCR preprocess + Windows OCR smoke test
- Detection fixture equivalence test
- Real app smoke test: window tracking, capture, overlay display, settings save, V-Archive sync, updater dry run
