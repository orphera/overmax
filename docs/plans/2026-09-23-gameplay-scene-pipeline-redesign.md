# Gameplay 씬 감지 Pipeline 재설계 계획

> PR #27의 Gameplay/Paused 씬 감지를 기존 인식 파이프라인에 자연스럽게 통합하기 위한 재설계 계획입니다.

**Goal:** Gameplay/Paused의 픽셀 판독은 유지하되, 별도 씬 감지·상태 전이 경계를 최소화하고 기존 DetectionPipeline의 관측 및 확정 흐름을 재사용한다.

**Architecture:** 먼저 PR 원본(`pr-27`)과 현재 Windows 지원 브랜치의 차이를 기준선으로 고정한다. PR이 새로 만든 코드와 기존 기능을 하나씩 대조해 같은 일을 하는 기존 helper, 상태, 스케줄러, 테스트 경로가 있는지 확인하고, 재사용 가능한 것은 기존 시스템에 통합한다. Gameplay 픽셀 판독만 도메인상 고유한 최소 로직으로 남기며, 독립 detector/pipeline처럼 보이는 구조는 실제 불가피성이 입증될 때만 유지한다.

**Tech Stack:** Rust workspace, `overmax_engine::detector`, Cargo tests, DXGI ROI atlas / GDI full-frame capture.

---

## 목표와 호환성 기준

- 씬 판독은 실제 검사 시점에만 수행한다. cached tick으로 후보 연속 횟수를 증가시키지 않는다.
- 정적 씬의 기존 parser, 히스테리시스, verified play event 및 기록 저장 경로의 동작을 보존한다.
- Gameplay/Paused 신호가 곡·기록 처리로 유입되지 않도록 기존 `is_record_scene()` 경계를 유지한다.
- Windows DXGI atlas와 GDI full-frame 모두 동일한 씬 후보 계약을 사용한다.
- 캡처 방식 자동 전환, OCR 추가, 다중 패스, 새 history 계층을 도입하지 않는다.
- 성능 개선을 주장하려면 측정한다. 정적 parser를 건너뛰는 최적화는 정확도/비용 근거와 회귀 테스트가 있을 때만 유지한다.
- 새로 추가된 구현을 기본적으로 정당화하지 않는다. 기존 시스템에 같은 책임을 수행하는 코드가 있으면 먼저 재사용·확장 가능성을 검토하고, 새 helper/state/type는 기존 구현으로 해결할 수 없다는 근거가 있을 때만 둔다.

## 변경 범위 원칙: main 공통 조상 기준 검토

- 구현 전 `git merge-base main HEAD`를 확인하고, 해당 기준점에서 현재 브랜치까지의 변경을 기능별로 분류한다.
- 이번 재설계와 무관한 기존 브랜치 변경은 보존한다. Gameplay 기능을 위해 추가된 코드도 현재 요구와 기존 구조에 비춰 검토하고, 단순화할 수 있는 부분은 정리한다.
- 제거 여부는 파일이 새로 추가됐는지가 아니라, 공통 조상 대비 변경이 현재 계약에 필요한지로 판단한다. atlas packing/translator의 기존 최적화 등 관련 없는 동작은 자동으로 되돌리지 않는다.
- 계획된 범위 밖의 기존 사용자 변경은 덮어쓰거나 정리하지 않는다. 모든 제거는 diff와 테스트 근거를 확인한다.
- **완료 기준:** 최종 diff를 공통 조상 기준으로 재검토해 각 유지된 변경의 이유와 각 제거의 영향이 설명 가능해야 한다.

## 기준선에서 확인된 상태

- PR 원본 `pr-27`은 `parse_static_scene()`을 먼저 실행하고 성공하면 종료했으며, 실패한 경우에만 `GameplaySceneReader`를 호출했다.
- PR 원본 Gameplay reader는 완전한 1920×1080 BGRA 프레임만 지원했다. 따라서 그 당시 DXGI atlas 프레임에서는 새 판독을 사용할 수 없었다.
- 현재 작업 브랜치에는 atlas ROI 매핑과 가상 atlas 테스트가 이미 추가돼 있다. 이는 PR 원본 동작과 혼동하지 않는다.
- 현재 코드에는 `SceneObservation::{InGame, Static, Unknown}`, `observe_scene()`의 Gameplay 우선 검사, 그리고 공용 `commit_scene()`이 있다. 재설계는 이 현재 상태를 대상으로 하되, PR 원본과의 차이를 보존한다.
- `docs/plans/2026-09-23-atlas-optimization-game-cycle-plan.md`는 ROI atlas 통합의 작업 기록이다. 완료 체크박스만으로 Windows 실캡처 검증이 끝났다고 간주하지 않는다.

## 기존 구조와의 조화

- 공통 조상과 PR 원본을 비교해 추가된 함수·필드·타입·테스트를 살펴보고, 기존 pipeline의 helper/state/contract와 겹치는 책임이 있는지 확인한다.
- 기존 `parse_static_scene`, `SceneType` 분류 helper, scene commitment/hysteresis, `RoiManager`/`AtlasTranslator`, worker reset 및 공통 출력 경로를 우선 확장·재사용한다. 실제 정의와 사용처를 확인하지 않은 채 유사 helper를 새로 만들지 않는다.
- 기존 코드로 자연스럽게 처리할 수 있는 동작은 해당 경로에 통합한다. 별도 코드가 더 명확하거나 필요한 경우에는 그 책임과 이유를 설계 기록에 설명한다.
- 리팩터링은 이름 변경이나 타입 추가 자체를 목표로 하지 않는다. `SceneObservation`처럼 한 곳에서 즉시 분기되는 표현은 공통 흐름을 더 단순하게 만들지 못하면 제거한다.
- **완료 기준:** 기존 구조와 새 판독 로직의 책임이 명확하고, 중복 구현 없이 씬 감지 흐름을 설명할 수 있다.

## 설계 질문 및 결정 기준

1. **판독기 책임:** `GameplaySceneReader`는 픽셀 근거에서 후보 씬만 반환하는 순수 판독기로 제한한다. scene history, cooldown, commit, pipeline 출력은 소유하지 않는다.
2. **공용 후보 계약:** 정적 parser와 Gameplay reader의 결과를 공통 후보 형태로 표현할 수 있는지 확인한다. 현재 `SceneObservation`이 그 계약을 단순히 나타내는지, 불필요한 별도 흐름을 만드는지 비교한다.
3. **우선순위:** Gameplay 판독 성공 시 정적 parser를 생략할 이유가 있는지 확인한다. 이를 정당화할 오분류 사례, 불필요한 비용 회피, 또는 데이터 의존성 증거가 없다면 두 감지기의 우선순위를 새로 만들지 않는다. 두 판독을 모두 할 경우의 비용과 후보 충돌 처리를 먼저 측정/명시한다.
4. **상태 확정:** 모든 후보는 기존 pipeline의 단일 실제 검사 시점과 공용 연속성/commit 규칙을 통과한다. 결과 씬 기존 확정 규칙은 회귀 없이 유지한다.
5. **미검출 의미:** `Unknown` 후보가 현재 확정 씬을 언제 지우는지, pending 후보 streak를 어떻게 끊는지는 기존 verified flow와 새 인게임 전환 사례를 표로 정의한 후 구현한다.
6. **프레임 표현:** atlas frame인지 full frame인지 캡처 계층의 실제 계약을 확인한다. 크기 숫자만으로 프레임 종류를 추측하지 말고 기존 타입/호출 경로에 식별 수단이 있는지 검색한다. 없다면 기존 코드만으로 안전한 판별이 불가능함을 먼저 보고하고 우회책 동의를 구한다.

## Tasks

### Task 1: 기준선과 변경 범위 확인

- [ ] `main`과 현재 HEAD의 merge-base를 확인하고, 공통 조상 대비 현재 diff를 기능별로 분류한다.
- [ ] `pr-27` 원본과 현재 브랜치의 씬 관련 변경을 비교해 PR 당시 동작과 Windows/atlas 후속 변경을 구분한다.
- [ ] PR에서 추가된 코드와 기존 main 공통 조상의 대응 경로를 확인해, 기존 구현을 확장하는 편이 자연스러운 부분을 정리한다.
- **완료 기준:** 이번 재설계 대상과 보존할 선행 변경이 구분되고, 관련 diff 범위가 설명 가능하다.

### Task 2: 현재 동작과 회귀 기준 고정

- [ ] 주요 전이에 대한 기대 입력/출력 표를 만든다: 선곡→Gameplay, Gameplay→Paused→Gameplay, Gameplay→결과, 결과→선곡, Gameplay 판독 miss, atlas/full-frame 전환, 창/포커스/캡처 reset.
- [ ] 내부 함수를 직접 호출하는 테스트와 실제 `detect()` 경로를 타는 테스트를 구분한다.
- [ ] 기존 parser/확정/기록 경로와 Gameplay 동작을 검증할 최소 회귀 테스트 목록을 확정한다.
- **완료 기준:** 주요 상태 전이의 기대값과 이를 검증할 테스트가 연결돼 있다.

### Task 3: 캡처 및 atlas 계약 추적

- [ ] `CapturedFrame` 생성 지점과 Windows GDI/DXGI worker 분기를 추적한다.
- [ ] Gameplay ROI의 atlas 슬롯 생성·복사·translator 경로가 런타임까지 연결되는지 확인한다.
- [ ] Linux 및 full-frame 경로에서 reader 입력의 좌표·크기·stride 계약을 확인한다.
- [ ] 프레임 종류를 기존 타입/호출 계약으로 안전하게 구분할 수 있는지 확인한다.
- **완료 기준:** 지원 프레임별 호출 흐름을 코드 위치와 테스트로 설명하고, 미확인 영역을 가정으로 메우지 않는다.

### Task 4: 기존 구조와의 중복 및 통합 지점 검토

- [ ] PR 추가 코드와 기존 helper/state/contract의 책임 중복 여부를 확인한다.
- [ ] `parse_static_scene`, `SceneType` 분류, scene commitment/hysteresis, ROI/atlas 변환, worker reset, 공용 출력 경로의 재사용 가능성을 확인한다.
- [ ] `SceneObservation`/`observe_scene()`이 흐름을 단순화하는지 검토하고, 그렇지 않으면 제거 대상으로 둔다.
- **완료 기준:** 기존 경로로 통합할 부분과 도메인상 고유한 픽셀 판독 책임이 구분된다.

### Task 5: 최소 통합 설계 결정

- [ ] 공용 후보 수집, 기존 parser 확장, 별도 경계 유지의 세 방안을 실제 계약·비용으로 비교한다.
- [ ] 별도 경계는 검증 가능한 동작 또는 비용상 요구가 있을 때만 선택한다.
- [ ] history/cooldown/commit은 기존 pipeline이 소유하도록 하고, 기존 타입/helper 재사용 가능성을 우선 검토한다.
- [ ] 선택 이유, 제외한 대안, 비용·정확도 영향, 후보 충돌 처리를 기록한다.
- **완료 기준:** 가장 단순한 구조가 기존 동작과 검증 근거에 부합한다.

### Task 6: 회귀 테스트 작성

- [ ] 실제 pipeline 경로에서 Gameplay/Paused 및 정적 씬 전이를 검증한다.
- [ ] 후보 miss, 연속 후보 변경, cached tick, 후보 확정 및 출력 의미를 검증한다.
- [ ] 기존 결과 씬 확정 및 verified event/record 회귀 테스트를 보존한다.
- **완료 기준:** 주요 성공·실패·전환 사례를 재현하는 테스트가 준비돼 있다.

### Task 7: 기존 pipeline으로 통합 리팩터링

- [ ] 테스트가 정한 구조에 따라 기존 pipeline helper/state/output 흐름을 확장한다.
- [ ] 중복 helper/state/type과 불필요한 `SceneObservation`/별도 분기를 통합 또는 제거한다.
- [ ] verified flow, 기록 씬, 설정·DB 호환성을 유지한다.
- [ ] 공통 조상 기준 diff를 재검토해 무관한 선행 Windows/atlas 변경을 보존한다.
- **완료 기준:** 공용 씬 감지·확정·출력 흐름을 사용하고 대상 테스트가 통과한다.

### Task 8: 빌드, 테스트 및 플랫폼 검증

- [ ] `cargo fmt --check`, workspace 테스트 및 관련 Clippy를 실행한다.
- [ ] Windows/Linux 빌드·테스트를 실행 가능한 환경에서 확인한다.
- [ ] 동일 픽셀 fixture로 atlas/full-frame 판독 결과를 대조한다.
- [ ] 가능한 경우 Windows GDI/DXGI 실앱 상태 전이를 확인하고, 미실행 항목은 미검증으로 기록한다.
- **완료 기준:** 실행 결과와 플랫폼별 미검증 항목이 구분돼 있다.

### Task 9: 문서 및 최종 diff 정리

- [ ] 계약이나 제약 변경이 있을 때만 `CONTEXT.md`와 해당 decision log를 갱신한다.
- [ ] 검증을 마친 항목만 완료 처리한다.
- [ ] 공통 조상 기준 최종 diff와 변경·제거 이유를 검토한다.
- **완료 기준:** 문서가 구현 및 실제 검증 상태를 정확히 반영한다.

## 우선 조사할 파일

- `rust/overmax_engine/src/detector/detection_pipeline.rs`
- `rust/overmax_engine/src/detector/gameplay_scene.rs`
- `rust/overmax_engine/src/detector/gameplay_scene/reader.rs`
- `rust/overmax_engine/src/detector/atlas_layout.rs`
- `rust/overmax_engine/src/detector/atlas_translator.rs`
- `rust/overmax_engine/src/capture/frame.rs`
- `rust/overmax_engine/src/detector/detection_worker.rs`
- `rust/overmax_engine/src/capture/capture_engine/windows/` 내 GDI/DXGI 캡처 경로
- 관련 pipeline 및 atlas 단위/통합 테스트

## 완료 시 보고할 것

- 선택한 관측 구조와 별도 경계를 유지/제거한 이유
- 바뀐 파일 및 공용 동작 계약
- 실행한 테스트/빌드의 실제 결과
- Windows GDI/DXGI 실앱 검증 여부와 남은 미검증 항목
