# Overmax Documentation Hub (문서 허브)

Overmax 프로젝트의 설계 문서, 아키텍처 다이어그램, 의사결정 기록(ADR), 기획서 및 릴리즈 노트를 체계적으로 관리하는 인덱스입니다.

---

## 🗺️ 디렉토리 맵 & 생명주기(Lifecycle) 분류

| 디렉토리 | 성격 / 수명주기 | 설명 | 주요 대상 |
|---|---|---|---|
| [`architecture/`](architecture/) | **Living (영구 유지)** | 현재 시스템의 최신 아키텍처 및 파이프라인 구조 | 개발자, AI 에이전트 |
| [`decisions/`](decisions/) | **ADR (누적 기록)** | 도메인별 핵심 설계 결정 및 트레이드오프 기록 | 개발자, AI 에이전트 |
| [`guides/`](guides/) | **Living (영구 유지)** | 환경별 설정 가이드 및 통신/추천 프로토콜 규격 | 기여자, 외부 연동자 |
| [`plans/`](plans/) | **Historical (시점 기록)** | 기능별 기술 기획서, 분석서, 실험 로그 및 포팅 계획 | 개발자, AI 에이전트 |
| [`releasenotes/`](releasenotes/) | **Releases (버전 기록)** | 버전별 릴리즈 노트 (플레이어 개선점 + 엔지니어링 세부사항) | 플레이어, 기여자 |
| [`archive/`](archive/) | **Archive (보관소)** | 이전 버전 마일스톤 태스크 목록 및 과거 아카이브 | 프로젝트 관리 |

---

## 📂 세부 문서 목록

### 1. Architecture (`docs/architecture/`)
현재 시스템의 핵심 모듈 구조 및 데이터 파이프라인을 설명합니다.
* [`system_overview.md`](architecture/system_overview.md): 전체 워크스페이스 구조, 크레이트 계층 의존성, 런타임 스레드 모델 및 메시지 버스.
* [`capture_and_window.md`](architecture/capture_and_window.md): DXGI/GDI 적응형 캡처, Per-Monitor DPI V2 좌표 매핑, 윈도우 스냅 및 추적 아키텍처.
* [`detection_pipeline.md`](architecture/detection_pipeline.md): CV 디텍션 파이프라인 레이어 구조, ROI 템플릿 매칭 & 1-Pass 엔진, 프레임 원자성 보장.
* [`data_storage_and_sync.md`](architecture/data_storage_and_sync.md): StartupCacheManager 무중단 기동, SQLite DB 스키마, V-Archive 증분 동기화 및 추천 시스템.
* [`ui_and_overlay_runtime.md`](architecture/ui_and_overlay_runtime.md): egui 멀티 뷰포트, 투명 오버레이 렌더링, 0-Cost i18n 및 Linux Layer Overlay.

### 2. Decisions (`docs/decisions/`) — ADR (Architectural Decision Records)
각 도메인별 주요 설계 결정과 대안, 트레이드오프를 누적 기록합니다.
* [`capture_and_window.md`](decisions/capture_and_window.md): Win32/DXGI 캡처, DPI V2 좌표계, 윈도우 스냅 및 멀티모니터 결정 사항.
* [`detection_pipeline.md`](decisions/detection_pipeline.md): CV 디텍션, 1-Pass 매칭, Centroid Kernel 사전 게이트, ROI 체크섬 캐시 결정 사항.
* [`data_and_sync.md`](decisions/data_and_sync.md): V-Archive 동기화, SQLite 캐시, Steam 계정 감지 결정 사항.
* [`ui_and_i18n.md`](decisions/ui_and_i18n.md): egui 렌더링, 한국어/영어 i18n, 설정창 UI 결정 사항.
* [`linux_support.md`](decisions/linux_support.md): X11/Wayland 캡처 및 Layer Overlay 지원 결정 사항.

### 3. Guides & Specifications (`docs/guides/`)
OS별 구동 가이드 및 외부 연동 프로토콜 규격을 보관합니다.
* [`linux-support.md`](guides/linux-support.md) / [`linux-support.en.md`](guides/linux-support.en.md): Linux 환경 빌드 및 실행 가이드 (한/영).
* [`recommend-provider-protocol.md`](guides/recommend-provider-protocol.md): 외부 추천 엔진 연동 프로토콜 v1 규격.
* [`ipc-protocol-guide.md`](guides/ipc-protocol-guide.md): 외부 도구/위젯 연동을 위한 Overmax IPC 프로토콜 (`overmax-ipc/1`) 규격 및 Python 예제 가이드.
* [`STORE_LISTING.md`](store/STORE_LISTING.md) / [`PRIVACY.md`](store/PRIVACY.md): Microsoft Store 등록 메타데이터, 심사 정책 준수 및 개인정보처리방침.

### 4. Plans & RFCs (`docs/plans/`)
과거 및 진행 중인 기술 기획서, 분석 보고서, 실험 결과를 보존합니다.
* **디텍션 & CV 파이프라인**:
  * [`2026-09-04-gpu-roi-atlas-capture-architecture.md`](plans/2026-09-04-gpu-roi-atlas-capture-architecture.md): GPU ROI Atlas 패킹 및 다중 해상도(720p~4K) 정규화 캡처 아키텍처.
  * [`2026-08-15-detection-pipeline-next-steps-plan.md`](plans/2026-08-15-detection-pipeline-next-steps-plan.md): 디텍션 파이프라인 잔여 작업 및 실측 계획.
  * [`2026-08-11-scene-detection-lightweight-plan.md`](plans/2026-08-11-scene-detection-lightweight-plan.md): 씬 감지기 경량화 및 Hysteresis 도입 계획.
  * [`2026-07-21-category-band-analysis-and-detection-spec.md`](plans/2026-07-21-category-band-analysis-and-detection-spec.md): 카테고리 띠 4px 통계 감지 사양.
  * [`2026-07-07-edge-detection-migration-plan.md`](plans/2026-07-07-edge-detection-migration-plan.md): 엣지 검출 기반 매칭 이식 계획.
  * [`2026-07-01-ocr-elimination-and-template-matching-plan.md`](plans/2026-07-01-ocr-elimination-and-template-matching-plan.md): OCR 제거 및 템플릿 매칭 1-Pass 전환 계획.
  * [`2026-06-23-detection-pipeline-architecture-and-recognition-logic.md`](plans/2026-06-23-detection-pipeline-architecture-and-recognition-logic.md): 디텍션 파이프라인 초기 구조 설계.
  * [`2026-06-15-image_db_redesign_plan.md`](plans/2026-06-15-image_db_redesign_plan.md): 이미지 DB 재설계 계획.
  * [`2026-05-28-scene-detection-experiment.md`](plans/2026-05-28-scene-detection-experiment.md): 씬 감지 실험 결과.
  * [`2026-05-21-roi-refactoring.md`](plans/2026-05-21-roi-refactoring.md) / [`2026-05-19-song-detection-context-refactoring.md`](plans/2026-05-19-song-detection-context-refactoring.md): 초기 ROI 및 컨텍스트 리팩토링.
* **캡처 & 시스템 최적화**:
  * [`2026-06-05-overlay-fullscreen-technical-analysis.md`](plans/2026-06-05-overlay-fullscreen-technical-analysis.md): 전체화면 오버레이 기술 분석.
  * [`2026-05-24-cpu-optimization-message-pump.md`](plans/2026-05-24-cpu-optimization-message-pump.md) / [`review.md`](plans/2026-05-24-cpu-optimization-message-pump-review.md): 메시지 펌프 CPU 최적화 분석.
* **데이터 & 아키텍처 안정성**:
  * [`2026-08-20-data-first-roi-cache-plan.md`](plans/2026-08-20-data-first-roi-cache-plan.md): Data-First RoiCache 및 ResultModeDiffLatch 캡슐화 계획.
  * [`2026-08-18-architecture-robustness-plan.md`](plans/2026-08-18-architecture-robustness-plan.md): SQLite 동시성 가드, 설정 I/O 큐, 캐시 전파 및 Repaint 스케줄링 계획.
  * [`2026-07-16-varchive-db-cache-design.md`](plans/2026-07-16-varchive-db-cache-design.md): V-Archive SQLite DB 캐시 설계.
  * [`2026-07-10-recommend-provider-protocol-design.md`](plans/2026-07-10-recommend-provider-protocol-design.md): 추천 제공자 인터페이스 및 프로토콜 설계.
  * [`2026-06-16-daily-missions-and-grid-ideas.md`](plans/2026-06-16-daily-missions-and-grid-ideas.md): 일일 미션 및 그리드 아이디어.
  * [`2026-06-16-result-screen-auto-capture-idea.md`](plans/2026-06-16-result-screen-auto-capture-idea.md): 결과창 자동 캡처 아이디어.
  * [`2026-06-05-result-screen-and-ladder-ban-analysis.md`](plans/2026-06-05-result-screen-and-ladder-ban-analysis.md): 결과창 및 래더 밴픽 분석.
  * [`2026-06-01-discord-ipc-mitm-security-review.md`](plans/2026-06-01-discord-ipc-mitm-security-review.md): Discord IPC 보안 리뷰.
* **포팅 & 초기 리팩토링**:
  * [`2026-05-11-opencv-to-rust-plan.md`](plans/2026-05-11-opencv-to-rust-plan.md) / [`2026-05-13-rust-native-port-plan.md`](plans/2026-05-13-rust-native-port-plan.md): OpenCV 제거 및 Rust 네이티브 포팅 계획.
  * [`2026-05-20-atomic-play-context-plan.md`](plans/2026-05-20-atomic-play-context-plan.md): 원자적 플레이 컨텍스트 설계.
  * [`2026-05-19-play-state-refactoring.md`](plans/2026-05-19-play-state-refactoring.md): PlayState 상태 머신 리팩토링.
  * [`2026-05-19-settings-ux-porting.md`](plans/2026-05-19-settings-ux-porting.md) / [`design.md`](plans/2026-05-19-settings-ux-porting-design.md): 설정 UI 포팅 설계.

### 5. Release Notes (`docs/releasenotes/`)
버전별 배포 내역입니다. (작성 규칙은 [`AGENTS.md`](../AGENTS.md)의 Release Protocol 준수)
* [`RELEASE_NOTES_v0.4.1.md`](releasenotes/RELEASE_NOTES_v0.4.1.md): 실시간 로컬 IPC 이벤트 스트리밍(SSE) 및 원격 RPC 제어, Windows Store(MSIX) 패키징 파이프라인, Windows HDR(scRGB) 무설정 감지 및 역변환 LUT, Linux(Wayland/KDE/Niri) 오버레이 상태 분리, 장면 전환 DXGI 스왑체인 재생성 방지 및 플랫폼 간 워커/텔레메트리 일원화.
* [`RELEASE_NOTES_v0.4.0.md`](releasenotes/RELEASE_NOTES_v0.4.0.md): 지능형 다차원 선곡 추천 엔진, 미니멀 사유 뱃지/툴팁, VerifiedPlayEvent 이벤트 아키텍처, ODDS 설정창 개편, 일본어(JA) 지원, 템플릿 매칭 Zero-Allocation 및 MAX COMBO 해시 캐싱.
* [`RELEASE_NOTES_v0.3.3.md`](releasenotes/RELEASE_NOTES_v0.3.3.md): 다중 모니터 DPI 인식, 1-Pass 매칭, Centroid 사전 게이트, 영어 UI 지원.
* [`RELEASE_NOTES_v0.3.2.md`](releasenotes/RELEASE_NOTES_v0.3.2.md): 다국어 i18n 시스템 및 UI 설정 개선.
* [`RELEASE_NOTES_v0.3.1.md`](releasenotes/RELEASE_NOTES_v0.3.1.md): V-Archive 증분 동기화, 동기화 필터, Favorite 마스킹, Linux 1차 포팅.
* [`RELEASE_NOTES_v0.3.0.md`](releasenotes/RELEASE_NOTES_v0.3.0.md) ~ [`RELEASE_NOTES_v0.2.0.md`](releasenotes/RELEASE_NOTES_v0.2.0.md): 과거 릴리즈 기록.

### 6. Archive (`docs/archive/tasks/`)
완료된 이전 버전의 마일스톤 태스크 목록입니다.
* [`TASKS_v0.4.0_archive.md`](archive/tasks/TASKS_v0.4.0_archive.md): v0.4.0 마일스톤 완료 작업 (스마트 추천 엔진, TrueSkill 실력 모델, ODDS 다이얼로그, RecordDB 분할, 일본어 i18n 등).
* [`TASKS_v0.3.0_archive.md`](archive/tasks/TASKS_v0.3.0_archive.md): v0.3.0 마일스톤 완료 작업.
* [`TASKS_v0.2.0_archive.md`](archive/tasks/TASKS_v0.2.0_archive.md): v0.2.0 마일스톤 완료 작업.

---

## 📝 문서 작성 및 관리 원칙

1. **단일 진실 공급원(SSOT)**: 시스템의 현재 사양 및 제약 조건은 프로젝트 루트의 [`CONTEXT.md`](../CONTEXT.md)가 최우선 기준입니다.
2. **아키텍처 결정(ADR)**: 영구적인 설계 결정이나 트레이드오프가 발생한 경우 [`docs/decisions/`](decisions/)의 해당 도메인 문서에 추가합니다.
3. **신규 기획 및 분석서**: 새로운 기술 기획이나 분석 문서는 `docs/plans/YYYY-MM-DD-주제명.md` 형식으로 작성합니다.
4. **릴리즈 노트**: 릴리즈 시 플레이어 중심 변경점과 엔지니어링 세부사항을 2-Track으로 분리하여 `docs/releasenotes/RELEASE_NOTES_vX.Y.Z.md`에 작성합니다.
