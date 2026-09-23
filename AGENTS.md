# Agent Overview

이 에이전트는 DJMAX RESPECT V 오버레이 기반 추천 시스템의
정확도 개선, 성능 최적화, 안정성 향상을 목표로 한다.

---

# Primary Goals

- 인식 정확도 향상 (song / mode / difficulty / rate)
- 인게임 성능 영향 최소화
- 안정적인 상태 전이 (verified pipeline 유지)

---

# Context Usage Policy

- context.md를 현재 시스템 상태의 단일 source of truth로 사용한다
- context.md에 명시된 제약 조건을 절대 위반하지 않는다
- context.md에 없는 시스템은 존재한다고 가정하지 않는다

---

# Decision Policy

## 성능 vs 정확도

- 인게임 성능 영향이 있는 경우:
  → 정확도보다 성능을 우선한다

- 선곡 화면에서만 실행되는 로직:
  → 정확도 우선

---

## 인식 로직 수정

- 기존 파이프라인 (verified flow)을 깨지 않는 선에서 개선
- 단일 프레임 판단보다 history 기반 접근 우선
- OCR은 fallback 또는 검증 용도로만 사용

---

## 추천 시스템

- 현재 구조 (floor 기반)는 유지
- 새로운 기준 추가 시:
  → 기존 정렬 기준을 깨지 않도록 보완 방식으로 적용

---

# Diff & Commit Discipline (변경 단위 원칙)

## 핵심 원칙

- **하나의 diff/커밋은 하나의 검증 가능한 주장만 담는다.** ("이 변경이 X 버그를 고친다" 혹은 "이 변경이 Y를 개선한다"처럼, 실패했을 때 무엇이 틀렸는지 특정할 수 있는 단위로 좁힌다.)
- 이 원칙에서 파생되는 실행 규칙:
  - **가능한 한 작은 커밋으로 나눈다.** 여러 결정을 하나의 diff에 뒤섞지 않는다 — `git blame`이 나중에 의미 있는 신호로 남으려면 커밋 단위가 이미 작아야 한다.
  - **임팩트가 큰 diff를 지양한다.** 문제가 생겼을 때 revert 범위가 국소적이어야 하며, 여러 파일/모듈에 걸친 변경은 각 모듈 단위로 쪼갤 수 있는지 먼저 검토한다.
  - 무관한 포맷팅/정리/리팩토링은 기능 변경과 같은 커밋에 섞지 않고 별도로 분리한다.

## 안정화된 코드에 대한 git blame 게이트

- 코드를 수정하기 전에 해당 라인/블록의 `git blame`(또는 `git log -p`)을 확인한다.
- 다음에 해당하면 **안정화된 코드**로 간주하고, 근거 없이 수정하지 않는다:
  - 마지막 커밋으로부터 3개월 이상 경과
  - 반복적으로 수정된 이력이 있는 라인 (여러 번 다듬어져 지금 형태에 도달한 코드)
- 안정화된 코드를 수정해야 하는 경우, 제안에 다음을 반드시 포함한다:
  1. 해당 라인의 `git blame` 정보 (커밋 해시, 마지막 수정 시점)
  2. 수정이 필요한 구체적 근거 — 아래 중 하나에 해당해야 하며, "더 깔끔해 보여서" / "이 패턴이 더 낫다고 판단해서" 같은 취향 판단은 단독 근거로 불충분하다:
     - 버그가 실제로 재현됨
     - 측정된 회귀(regression)가 확인됨
     - 현재 작업이 명시적으로 해당 코드 변경을 요구함
- 위 근거 없이 안정화된 코드를 건드리는 제안은 아래 "대규모 재작성 및 임의 리팩토링 금지"와 동일하게 취급한다.

---

# Key Constraints (핵심 제약 및 절대 금지 사항)

- **메모리 접근 및 프로세스 인젝션 금지**: 화면 캡처 및 Win32 API 추적 방식만 사용해야 한다.
- **성능 저하 야기 금지 (최우선)**: 특히 OCR 1-Pass를 강제하며, 다중 패스 루프 생성을 절대 금지한다.
- **땜질식(대증요법) 코드 지양**: 근본 원인을 분석하지 않고 일회성 헬퍼나 임시방편 코드를 덧대는 행위를 엄격히 금지한다. 만약 프레임워크나 시스템의 한계로 인해 우회책/땜질이 불가피하다면 **반드시 사용자에게 이유와 대안을 설명하고 사전 동의를 구해야 한다.**
- **대규모 재작성 및 임의 리팩토링 금지**: 강력한 이유 없이는 작동 중인 코드를 재작성하거나 관련 없는 주변 코드를 리팩토링하지 않는다. (판단 기준은 위 "안정화된 코드에 대한 git blame 게이트" 참조.)
- **절대 경로 사용 금지**: 문서와 코드에서 절대경로(예: `D:\dev\...`) 사용을 금지하며, 항상 프로젝트 루트 기준 상대경로를 사용한다.
- **기존 호환성 파괴 금지**: 사용자 설정(`settings.user.json`) 및 DB 구조 등 기존 사용자 파일과의 호환성을 유지해야 한다.

---

# Failure Handling

- **사용자의 의도 파악이 불확실한 경우**:
  → 자의적으로 추측하여 작업을 진행하지 말고 **반드시 작업을 멈추고 사용자에게 다시 질문하여 의도를 명확히 확인할 것.**

- 확실하지 않은 시스템 상태:
  → 결과를 보류하거나 verified=False 유지

- 복수 해석 가능:
  → 조건별로 분리해서 제시

- 정보 부족:
  → 최소 질문만 생성 (1~2개)

---

# Output Format

기술 제안 시 다음 구조를 따른다. 단, 이 5단계 구조는 복잡하거나 트레이드오프가 있는 결정에만 조건부로 적용하며, 단순 버그 fix류는 면제한다.

1. 문제 정의
2. 원인 분석
3. 해결 방법 (옵션별)
4. 트레이드오프
5. 추천안

---

# Prohibited Actions

- **의도 미파악 상태에서의 자의적 작업 진행 금지** (반드시 질문할 것)
- **사전 동의 없는 땜질/우회 코드 삽입 금지** (이유 설명 및 동의 필수)
- 근거 없는 성능 개선 주장 금지
- 전체 리팩토링 제안 금지 (요청 시 제외)
- 기존 파이프라인 무시 금지
- **`git blame` 확인 및 근거 제시 없는 안정화된 코드 수정 금지** (위 "Diff & Commit Discipline" 참조)
- **하나의 diff에 여러 개의 독립된 결정을 뒤섞는 행위 금지**

---

# Reference Documents (필요시 참조)

- **문서 허브 (인덱스)**: 전체 문서 분류 및 맵은 [docs/README.md](docs/README.md)를 참고한다.
- **상세 제약 조건**: 상세한 시스템 스펙 및 제약 조건은 단일 Source of Truth인 [CONTEXT.md](CONTEXT.md)를 필요할 때 참고한다.
- **도메인별 설계 결정 (Decision Logs)**:
  - 디텍션 & CV: [docs/decisions/detection_pipeline.md](docs/decisions/detection_pipeline.md)
  - 캡처 & 윈도우: [docs/decisions/capture_and_window.md](docs/decisions/capture_and_window.md)
  - 데이터 & 동기화: [docs/decisions/data_and_sync.md](docs/decisions/data_and_sync.md)
  - UI & 다국어(i18n): [docs/decisions/ui_and_i18n.md](docs/decisions/ui_and_i18n.md)
  - Linux 지원: [docs/decisions/linux_support.md](docs/decisions/linux_support.md)
- **엔지니어링 취향**: 설계 및 코드 변경 시 소유자의 엔지니어링 취향을 반영하기 위해 [ENGINEERING_TASTE.md](ENGINEERING_TASTE.md)를 필요할 때 참고한다.

---

# Session Handoff Protocol

의미 있는 변경(작업 완료, 제약 조건 변경, 설계 결정 등)이 있었을 때만 세션 종료 직전에 다음을 수행한다:

1. `cargo fmt` 및 `cargo clippy --fix`를 실행하여 코드를 정리하고 경고를 수정한다
2. `TASKS.md`의 완료 항목을 `[x]`로 갱신한다
3. 새로운 제약 조건이나 아키텍처 변경이 있었다면 `CONTEXT.md`를 갱신한다
4. 중요한 설계 결정이 있었다면 해당하는 도메인의 `docs/decisions/<domain>.md` 문서에 Decision Log 요약 행을 추가한다

---

# Release Protocol (릴리즈 노트 작성 및 배포 규격)

새로운 버전(vX.Y.Z) 릴리즈 준비 시 에이전트는 다음 규칙과 절차를 준수하여 문서를 작성/갱신한다.

## 1. 릴리즈 노트 파일 경로
- `docs/releasenotes/RELEASE_NOTES_vX.Y.Z.md`

## 2. 2-Track 서술 원칙 & 작성 템플릿
릴리즈 노트는 **플레이어 중심 섹션**과 **엔지니어링 세부 사항 섹션**으로 엄격히 분리하여 작성한다.

> **💡 플레이어 중심 서술 철학**:
> - '일반 사용자', '유저', '엔드 유저(End-user)' 등 공급자 중심 메타 용어를 일체 사용하지 않는다.
> - 기능 구현명이 아닌 **플레이 경험, 화면 동작, 해결된 불편 상황** 관점으로 서술한다. (예: "Centroid Gate 도입" → "곡 목록 화면에서 불필요한 연산을 줄여 게임 중 버벅임을 완화했습니다")

```markdown
# Overmax vX.Y.Z 릴리즈 노트

> vX.Y.(Z-1) 이후 변경 사항 (vX.Y.Z)

---

## 🎮 새로워진 점 및 개선 사항

### 🖥️ 1. [기능명 또는 해결된 문제 상황]
* **[체감 변화]**: 인게임 플레이 중 발생하던 끊김을 줄이고 화면 인식을 더욱 부드럽게 개선했습니다.
* **[개선 세부 요약]**: 어떤 상황에서 어떤 점이 편리해졌는지 명확하게 설명합니다.
* 💡 **이용 팁**: 설정 변경 방법이나 권장 옵션이 있다면 함께 안내합니다.

---

## 🛠️ 엔지니어링 & 내부 아키텍처 변경점

### 🎯 1. [모듈명 및 기술 주제]
* **[핵심 기술/아키텍처 명칭]**:
  * 내부 메커니즘 변경 내용, 수학적 공식($\text{Rate} = \text{Score} / 10000.0$), 성능 벤치마크 수치(ms/CPU)
  * 변경된 주요 모듈, 구조체, Win32/DXGI/Rust API 호출 흐름 기술
```

## 3. 릴리즈 동반 작업 체크리스트
릴리즈 노트 작성 후 반드시 다음 동기화 작업을 수행한다:
1. `Cargo.toml`의 패키지 `version` 번호 갱신
2. `TASKS.md`의 완료된 마일스톤 항목을 `docs/archive/tasks/TASKS_vX.Y.Z_archive.md`로 이동 및 신규 버전 로드맵 구성
3. `README.md` 및 `README.en.md`의 버전 표기 및 주요 기능 소개 갱신
4. `CONTEXT.md`의 변경 이력 및 최신 제약사항 반영
5. `docs/README.md`의 릴리즈 노트 링크 목록 갱신

---

# Quick Reference

## 빌드 & 검증
- 전체 빌드: `cargo build --workspace`
- 테스트: `cargo test --workspace`
- Clippy: `cargo clippy --all-targets`
- 릴리스 빌드: `build.bat`

## 주요 진입점
- 메인 앱: `rust/overmax_app/src/main.rs`
- 디텍션 파이프라인: `rust/overmax_engine/src/detector/detection_pipeline.rs`
- 디텍션 워커: `rust/overmax_engine/src/detector/detection_worker.rs`
- PlayState 감지: `rust/overmax_engine/src/detector/play_state.rs`
- 템플릿 매칭 엔진: `rust/overmax_engine/src/detector/templates/`
- CV 코어: `rust/overmax_cv/src/lib.rs`

## 설정 파일
- 기본 설정: `settings.json`
- 사용자 설정: `settings.user.json` (delta 형식, 기본값과 다른 항목만 저장)
- 곡 DB: `cache/songs.json`
- 기록 DB: `cache/record.db` (SQLite)
- 이미지 인덱스: `cache/image_index.db`