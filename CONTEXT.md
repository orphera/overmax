# Context: Overmax Development

이 문서는 Overmax 프로젝트의 현재 상태, 설계 결정 사항, 그리고 시스템 스펙을 기록한다.

---

# System Overview

Overmax는 DJMAX RESPECT V의 화면을 실시간으로 분석하여, 현재 선택된 곡의 난이도별 정보를 오버레이로 보여주는 도구이다.

- **인식 방식**: 화면 캡처 + Rust 네이티브 CV 이미지 매칭 (`overmax_cv`) + OCR (Windows OCR)
  - *캡처 엔진*: GDI 캡처 엔진 및 DXGI Desktop Duplication 캡처 엔진을 감싸는 `AdaptiveCaptureEngine` Facade 구성. 게임이 전체화면(Borderless 포함)일 때만 DXGI 백엔드를 런타임에 동적으로 기동하여 CPU 부하를 해소하고, 창 모드에서는 GDI 캡처로 스위칭하며 불필요한 DXGI 리소스는 즉시 해제함.
- **UI**: egui / winit (하드웨어 가속 활용 멀티 뷰포트 네이티브 UI)
  - 전체화면 포커스 차단 및 Z-Order 유지: 게임 윈도우 최소화 방지를 위해 `WS_EX_NOACTIVATE` 및 `WS_EX_TOOLWINDOW` 스타일을 주입하고, 게임 창을 오버레이 창의 Owner(`GWL_HWNDPARENT`)로 연결하여 전체 창 모드(Borderless Fullscreen) 플레이 시 드래그 앤 드롭 후 포커스가 게임으로 복귀해도 오버레이가 항상 최상단에 물리도록 보장함. 비활성 시 topmost 해제로 인한 DWM 소유 관계 깜빡임을 막기 위해 `is_active` 상태 검증 캐싱을 정밀화하고, `cached_game_hwnd`를 이용해 매 프레임 `FindWindowW` 오버헤드를 차단함.
  - 오버레이 스냅과 드래그 제어: egui의 내장 네이티브 드래그 기능인 `ViewportCommand::StartDrag`를 도입하여 OS가 오버레이 창의 드래그를 네이티브로 처리하도록 위임함. 이로써 불필요한 마우스 절대 좌표 연산 및 임시 구조체 필드를 소멸시킴. 구석 고정(Snap) 시에는 `try_lock()`으로 백그라운드 스레드와의 락 경합을 방지하고, 기하 구조 캐시(`PREV_SNAP_GEOMETRY`)를 적용하여 좌표 변화가 없을 때는 `SetWindowPos` 호출을 생략(0회)함.
- **데이터**: V-Archive DB (JSON) 및 로컬 기록 DB (SQLite)

---

# Core Constraints

- 메모리 접근 / 프로세스 인젝션 금지 (화면 캡처 및 Win32 API 추적 방식 유지)
- 인게임 성능 영향 최소화 (최우선 과제)
- 자가 업데이트 및 락 제어: 업데이트 후 재시작 시 중복 실행 락(Named Mutex) 해제 지연으로 새 인스턴스가 조기 종료되는 것을 방지하기 위해, 부모 프로세스의 락 가드(`SingleInstanceGuard`)를 명시적으로 `drop()`한 후 새 프로세스를 spawn하고 기존 프로세스를 즉시 종료하는 안전한 재시작 워크플로우를 유지함.
- Python 레거시 코드 완전 제거 및 순수 Rust 코드베이스로 전환 완료 (`rust/` workspace)
- 스팀(Steam) 경로 탐색 및 계정 연동: V-Archive 연동 등을 위한 스팀 계정 정보(`loginusers.vdf`)를 탐색할 때, 하드코딩된 기본 경로 및 HKCU/HKLM 레지스트리를 먼저 조회합니다. 만약 검색에 실패할 경우 최종 폴백으로 실행 중인 `steam.exe` 프로세스를 Win32 Toolhelp 스냅샷 API로 스캔하여 실행 경로를 동적으로 검출합니다.
- Windows 10 (버전 1809) / 11 64-bit 환경 전용 (Windows OCR API 및 Win32 API 의존성). 단, Non-Windows 환경에서 빌드가 깨지지 않도록 `target_os` 조건부 컴파일 가드와 egui 폴백 코드를 적용하여 크로스 플랫폼 빌드 이식성을 확보함.
- 기존 사용자 파일과의 호환성 유지:
  - `settings.user.json` (사용자 설정 델타 저장)
  - `cache/record.db` (로컬 플레이 기록 SQLite DB)
  - `cache/songs.json` (V-Archive 곡 DB)
  - `cache/image_index.db` (곡 재킷 매칭용 DB)

---

# Current Architecture

## Workspace Crate 구조
- `overmax_app`: 메인 GUI 애플리케이션 (`egui/winit` 기반 멀티 뷰포트 오버레이 UI, 설정/동기화/디버그 창 및 윈도우 스타일 제어)
- `overmax_engine`: 화면 캡처 및 실시간 디텍션 핵심 엔진 (GDI/DXGI 캡처, OCR 디텍터, Hysteresis 버퍼, ROI 관리 및 템플릿 데이터)
- `overmax_core`: 공통 데이터 모델 및 핵심 상태 구조체 (`GameSessionState`, `PlayContext`, `SceneType`)
- `overmax_data`: 설정 파싱, SQLite DB (`RecordDB`), V-Archive API 클라이언트, 추천 정렬 로직 및 유사도 검색 알고리즘
- `overmax_cv`: 이미지 매칭(HOG, Perceptual Hash), OCR 전처리(Grayscale, Upscale, Otsu 이진화, 컬러 패스 등)

## 데이터 흐름 및 스레드 구조
```
[Main GUI Thread (egui/winit)]
   ├── Overlay Window (오버레이 정보 표시, 드래그 이동/스케일링)
   ├── Settings / Sync / Debug Windows (설정 변경, V-Archive 기록 동기화, 실시간 로그)
   └── Channel Receiver (디텍션 결과 수신 및 UI 상태 반영)
           ▲
           │ (mpsc channel)
           ▼
[Detection Worker Thread]
   ├── WindowTracker: DJMAX RESPECT V 창 추적 (Win32 API)
   ├── ScreenCapture: GDI 기반의 실시간 프레임 캡처
   └── DetectionPipeline
        ├── OcrDetector: Windows OCR 멀티패스 (Color / Grayscale / Binarized / Inverted) → logo SceneType 판별, rate f32 추출
        ├── ImageIndexDb: overmax_cv (HOG + Hash 매칭 -> song_id 탐색)
        └── PlayStateDetector: (버튼 모드, 난이도, 맥스콤보 감지)
```

---

# Detection Pipeline & State Handling

## 1. 프레임 제어 및 쿨다운 스케줄링 (Centralized Control & Cooldowns)
- **Window Tracker 동적 폴링**: DJMAX Respect V 창의 위치 및 포커스를 조회하는 Win32 시스템 콜 오버헤드를 막기 위해 `WindowQueryScheduler`가 주기적으로 호출을 차단합니다. 창 드래그 중인 경우 `16ms`(60FPS), 창이 정지 상태인 경우 `300ms`, 창 미발견 시 `1000ms`로 주기를 자동 변환합니다.
- **DXGI 재생성 쿨다운**: `AdaptiveCaptureEngine`이 DXGI 캡처에 실패하여 GDI로 폴백할 시, 매 프레임 재생성을 시도하지 않고 최소 **3초**의 쿨다운 간격을 보장하여 CPU 스팸 루프를 차단합니다.
- **OCR 픽셀 체크섬 조기 리턴 (Early Return)**: `PlayStateDetector`가 `rate` 영역을 인식할 때 매 프레임 `crop_roi` 및 썸네일을 힙에 생성하지 않고, 원본 버퍼 상에서 즉각 픽셀 값을 건너뛰어 합산하는 `compute_pixel_checksum`을 호출합니다. 이전 체크섬과 차이가 50 이하이고 캐시 강제 만료 시간(5초)이 지나지 않았다면 OCR API와 이미지 크롭 호출 자체를 바이패스합니다. 실제 OCR 분석은 값이 바뀌었을 때 최소 **200ms** 간격으로만 실행됩니다.

## 2. 씬 감지 및 동적 ROI (Scene-Aware ROI)
- **로고 OCR 감지**: `logo` ROI 영역에 대해 Windows OCR 멀티패스를 수행 (Color → Grayscale → Binarized → Binarized Inverted 순서로 시도, 첫 번째 매칭 성공 시 즉시 반환).
  - 키워드 매칭: `FREESTYLE` → `SceneType::Freestyle`, `ONLINE` → `SceneType::Online`, 전 패스 매칭 실패 → `SceneType::Unknown`.
- **동적 ROI 전환**: `RoiManager`가 감지된 씬(`SceneType`)에 따라 최적의 ROI 세트(Freestyle / Online)를 동적으로 전환.
  - `logo` ROI는 씬과 독립적으로 상단 고정 좌표를 가지며, 씬 판별의 트리거 역할을 수행.
- **히스테리시스 버퍼**: `HysteresisBuffer`를 통해 선곡 화면 진입/이탈 판정 및 신뢰도(Confidence) 계산.

## 3. 곡 인식 (Song Recognition)
- **재킷 이미지 매칭**: `ImageIndexDb`를 통해 캡처된 재킷 영역과 미리 색인된 곡 재킷의 유사도를 계산.
- **Rust Native CV**: `overmax_cv`를 통해 3종 Perceptual Hash(pHash, dHash, aHash)를 사용한 재킷 매칭 및 검색 지원. CPU 성능 최적화를 위한 HOG 특징 연산 비활성화 옵션(`disable_hog`, 기본값 `false`)을 지원합니다.

## 3. 원자적 상태 감지 및 안정화 (Atomic Play Context Sync)
- **PlayState 감지**:
  - **버튼 모드 (Button Mode)**: `btn_mode` ROI의 평균 BGR 색상과 미리 정의된 대표색(4B/5B/6B/8B)의 Euclidean 거리가 60 이하인 모드 중 최적 매칭값 선택.
  - **난이도 (Difficulty)**: 각 난이도 패널 ROI(NM/HD/MX/SC)의 평균 밝기를 계산. 상위 1위 밝기가 최소 밝기(45) 이상이고 2위와의 차이(margin)가 15.0 이상일 때 유효(confident)한 난이도로 판정.
  - **Max Combo**: 결과창 및 선곡창의 `max_combo_badge` ROI 영역에 대해 사전에 수집된 대표 뱃지 이미지 템플릿과의 이미지 해시(pHash, dHash, aHash) 비교를 수행. 가중 해밍 거리가 10.0 이하인 경우에 한해 True로 판정하여, 연출 그래픽 변화나 노이즈에 의한 Jitter 및 오인식을 완벽하게 차단.
  - **Rate**: `rate` ROI 영역의 Windows OCR 멀티패스(Color → Grayscale → Grayscale Inverted) 결과를 실수값(`f32`)으로 실시간 수집. 유효 파싱값이 나온 첫 번째 패스 결과를 채택.
  - **Score & Rate Cross-Validation**: 결과창 및 선곡창에서 `score` ROI 영역을 단일 패스 OCR로 추출하여 판정율을 역산(`Rate = Score / 10,000`)합니다. 두 OCR 결과(Rate vs. Score 역산값) 간에 불일치가 발생할 경우, 오차가 0.1% 이내이면 정밀한 스코어 역산값으로 보정하고, 오차가 클 경우 각 값의 정확도 범위(`90%~100%`, `70%~90%` 등)를 기준으로 타당성(Plausibility) 신뢰도를 평가해 더 상식적이고 가능성이 높은 값을 최종 채택합니다. 추가로 선곡창 자릿수 오인식에 대비해 신뢰 범위 가드(MIN_VALID_RATE인 80% ~ 100%)를 둡니다.
- **원자적 안정화**:
  - 곡 ID, 버튼 모드, 난이도, Rate, Max Combo 전체를 하나의 `PlayContext`로 묶어 관리.
  - `PlayStateDetector`에서 이 전체 필드가 연속으로 N 프레임(기본 3프레임) 동안 완벽히 동일하게 감지될 때만 `GameSessionState.is_stable = true` 상태로 commit.
  - 안정적으로 확정된 상태에 한해서만 로컬 SQLite DB(`cache/record.db`)에 플레이 기록을 자동 upsert 및 저장.

---

# UI & UX Features

- **egui/winit 멀티 뷰포트**: 네이티브 타이틀바가 없는 투명 오버레이 구현.
- **오버레이 드래그 & 스냅**: 마우스 드래그를 통한 위치 이동 및 모니터 경계 스냅 지원. 마우스 드래그 종료 시 자동으로 DJMAX RESPECT V 게임 창으로 포커스(foreground)를 복원하여 플레이 방해를 최소화. High-DPI 디스플레이 환경에서 DPI 대응 스케일링 보정 처리 반영.
- **라이트모드 (Lite Mode) 및 구석 스냅 고정**:
  - 추천 리스트 등 불필요한 레이아웃을 완전히 숨기고, 곡 제목, 버튼 모드, 난이도, 비공식 난이도, 실시간 Rate, 콤보 뱃지([M]/[P]), 그리고 sheet_meta 정보만 노출하는 극도로 축소된 레이아웃(세로 높이 `60.0 * scale`) 지원.
  - 라이트모드 동작 중에는 의도치 않은 드래그 이동을 차단하며, 설정에서 지정한 화면 구석 위치(좌상단, 우상단, 좌하단, 우하단)로 창이 흔들림 없이(Jitter-free) 자동 스냅 고정됨.
- **스케일 프리셋**: S / M / L / XL 4단계 스케일 프리셋 지원 및 `settings.user.json` 저장. egui 렌더링 시 버튼 크기 고정 및 패딩 보정을 통해 UI Jitter(흔들림)를 방지.
- **V-Archive 연동 및 기록 동기화**:
  - V-Archive API를 통한 플레이 데이터 패치/자동 갱신.
  - 로컬 DB에만 존재하는 갱신 후보 데이터를 스캔하여 V-Archive 웹서버에 일괄 등록/삭제 지원 (`SyncWindow`).
- **실시간 신기록 및 간편 업로드 알림**: 플레이 중 감지된 Rate가 V-Archive 기존 기록보다 높을 경우, 오버레이 헤더 내 단독 업로드 버튼(⬆) 활성화 및 상태 표시 램프 기능 지원.

---

# Debug Strategy

- **Debug UI**: `debug_ui.rs`를 통해 모듈별 실시간 디버그 로그 표시, 카테고리 필터링, 일시정지, 비우기 기능 제공. Rate OCR 텔레메트리(OCR에 전달된 실제 이미지 컬러/그레이스케일 미리보기, Threshold/BgMean/Invert 수치) 지원.

---

# Important Invariants (불변 조건)

1. **상태 기록 조건**: `is_stable = true` 일 때만 상태를 commit하고 기록을 저장한다.
2. **미플레이 구분**: `rate == 0.0`은 미플레이 상태를 의미하며 DB에 저장하지 않는다.
3. **명시적 Null 처리**: `rate` 수집값로 `Option<f32>`를 사용하여 미플레이(`0.0` 또는 `None` 처리)와 명시적으로 구분해야 한다.
4. **곡 ID 예외**: `song_id == 0`은 유효한 곡 ID로 처리한다. 곡 정보가 아예 없는 경우는 `Option::None`이어야 한다.
5. **설정값 유효성 검증**: 사용자 설정 저장 시 반드시 delta 형식을 유지하고 값의 범위를 normalize/clamp 처리한다.
6. **OCR 1-Pass 강제**: 모든 OCR(Rate, Score 등)은 단일 패스(1-Pass) 실행만 허용한다. 인게임 성능 보호를 위해 3-pass 등의 다중 패스 루프 생성을 절대 금지하며, 오인식 대응은 HysteresisBuffer 기반의 프레임 히스토리 다수결 안정화로 해결해야 한다.

---

# Future Focus

1. **감지 씬(Scene) 다양화**:
   - FREESTYLE 및 ONLINE 대기방 외에도 래더 매칭 씬이나 결과 화면 등 감지 가능 범위를 추가 확장 (`SceneType::LadderMatch` 등).
2. **전체화면(Fullscreen) 호환성 검증 (완료)**:
   - `AdaptiveCaptureEngine` 동적 위임 연동(DXGI 캡처 백엔드) 및 포커스 차단용 Win32 스타일 적용 완료. 전체 창 모드(Borderless Fullscreen)에서 드래그 종료 후 포커스 복원 시 Z-Order 밀림 현상을 Win32 Owner 윈도우 연동(`GWL_HWNDPARENT`)을 통해 해결 완료. 단, OS 설계 제약 및 안티치트 충돌로 인해 하드웨어 독점 전체화면(Exclusive Fullscreen) 모드에서의 오버레이 표시는 지원하지 않으며, 보더리스 모드 실행이 필수 권장 사항임.
3. **V-Archive 클라이언트 완전 대체 (장기 목표)**:
   - 공식 데스크톱 클라이언트의 도움 없이 Overmax 자체 앱 내에서 플레이 기록 수집부터 V-Archive 연동 및 백업 업로드까지 전담하는 올인원 클라이언트 구현.
4. **HOG 피처 데이터베이스 갱신 및 재빌드**:
   - 로컬 이미지 왜곡으로 인한 HOG 유사도 저하 근본 해결 및 매칭 임계치를 기존 값(`0.85`)으로 원복하기 위한 피처 일괄 갱신.
5. **마우스 호버 시 투명도 반응 지연, 커서 사라짐 및 깜빡임 개선 (완료 - v0.2.3)**:
   - egui의 MousePassthrough 전환 시 topmost 스타일 검증 캐시 오판독 버그를 해결하고 `ViewportCommand::StartDrag`와의 조화를 통해 마우스 호버 및 투명도 전환으로 인한 깜빡임 및 불투명 고착 현상을 안정화 완료함.
    - 마우스가 투명 오버레이로 진입해 passthrough가 풀렸을 때 마우스 포인터가 소실되는 문제를 방지하기 위해, 오버레이 위에 마우스가 있는 동안 하드웨어 시스템 커서를 완전히 숨기고 십자선 모양(Crosshair)의 소프트웨어 커서를 직접 그리도록 조치하여 Active/Deactive 상태 전환 시에도 일관된 마우스 커서의 일정성과 가시성을 확보함.

---

# Decision Log

주요 설계 결정의 배경(why)을 요약한다. 상세 분석은 참조 문서를 확인할 것.

| 날짜 | 결정 | 이유 | 참조 |
|------|------|------|------|
| 2026-05 | GDI 캡처 버퍼 인플레이스 재사용 | CPU 프로파일링 결과 memcpy 힙 할당이 주 병목 | [cpu-optimization-message-pump.md](docs/2026-05-24-cpu-optimization-message-pump.md) |
| 2026-05 | WindowTracker 동적 폴링 주기 | win32u 시스템 콜 오버헤드 해소 | [cpu-optimization-message-pump-review.md](docs/2026-05-24-cpu-optimization-message-pump-review.md) |
| 2026-05 | HysteresisBuffer 기반 씬 전이 안정화 | 단일 프레임 판단의 Jitter 방지 | [scene-detection-experiment.md](docs/2026-05-28-scene-detection-experiment.md) |
| 2026-06 | HOG 선택적 비활성화 (`disable_hog`) 지원 | HOG 특징 연산 스파이크 예방을 위해 필요 시 설정에서 비활성화할 수 있는 옵션 지원 | [detection-pipeline-architecture.md](docs/2026-06-23-detection-pipeline-architecture-and-recognition-logic.md) |
| 2026-06 | Image DB 빌드를 Rust CLI로 이전 | overmax_cv를 피처 연산 SSOT로 통일 | [image_db_redesign_plan.md](docs/2026-06-15-image_db_redesign_plan.md) |
| 2026-07 | OCR 1-Pass 강제, 다중 패스 루프 금지 | 3-pass OCR이 CPU 과부하 유발 → 오인식은 HysteresisBuffer 다수결로 해결 | [ocr-elimination-plan.md](docs/2026-07-01-ocr-elimination-and-template-matching-plan.md) |
| 2026-07 | 이진화 → 엣지 디텍션 전환 검토 | BGA 투과 시 고정 threshold 불안정, Sobel 엣지가 BGA에 강건 | [edge-detection-migration-plan.md](docs/2026-07-07-edge-detection-migration-plan.md) |
| 2026-07-08 | 결과창 괄호 및 선곡창 메타 텍스트 폰트 축소 | 뱃지 텍스트(9.0)와 비교/메타 텍스트(10.0) 간의 크기 불일치 시각적 불균형 일괄 해결 | [overlay_ui.rs](rust/overmax_app/src/ui/overlay_ui.rs) |
| 2026-07-09 | PlayStateDetector rate OCR 로직 플래튼화 | 중첩 if 제어 블록을 `process_rate_ocr` 헬퍼 함수로 추출하여 가독성 개선 | [play_state.rs](rust/overmax_engine/src/detector/play_state.rs) |
| 2026-07-09 | logo ROI 좌표 하드코딩 제거 | `GlobalRoiConfig`에 logo 설정을 통합하고 `RoiManager`가 참조하도록 구조화 | [roi.rs](rust/overmax_engine/src/detector/roi.rs) / [scene_config.rs](rust/overmax_data/src/scene_config.rs) |
| 2026-07-09 | NativeApp::update 330줄 분할 및 락 오버헤드 최적화 | 자이언트 함수를 3개 헬퍼 함수로 분해하고 `game_rect` 락 획득을 1회로 병합 | [native_app_viewports.rs](rust/overmax_app/src/ui/native_app_viewports.rs) |
| 2026-07-09 | 결과창 스코어용 얇은 폰트 템플릿 수집 및 추가 | 결과창의 얇은 폰트 조건에서 스코어 '8'이 '3'으로 오독되는 문제를 새로운 템플릿 마스크 추가로 완벽 해결 | [generate_templates_code.rs](rust/overmax_app/src/bin/generate_templates_code.rs) |
| 2026-07-09 | 결과창 모드 매칭에 Bradley-Roth 적응형 이진화 적용 | BGA 배경이 밝아져 '8B' 등 모드 글자가 배경에 묻히는 문제를 적분 이미지 기반 적응형 이진화 도입으로 완벽 해결 | [ocr_engine.rs](rust/overmax_engine/src/detector/ocr_engine.rs) |

