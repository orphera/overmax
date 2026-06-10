# Context: Overmax Development

이 문서는 Overmax 프로젝트의 현재 상태, 설계 결정 사항, 그리고 시스템 스펙을 기록한다.

---

# System Overview

Overmax는 DJMAX RESPECT V의 화면을 실시간으로 분석하여, 현재 선택된 곡의 난이도별 정보를 오버레이로 보여주는 도구이다.

- **인식 방식**: 화면 캡처 + Rust 네이티브 CV 이미지 매칭 (`overmax_cv`) + OCR (Windows OCR)
  - *캡처 엔진*: GDI 캡처 엔진 및 DXGI Desktop Duplication 캡처 엔진을 감싸는 `AdaptiveCaptureEngine` Facade 구성. 게임이 전체화면(Borderless 포함)일 때만 DXGI 백엔드를 런타임에 동적으로 기동하여 CPU 부하를 해소하고, 창 모드에서는 GDI 캡처로 스위칭하며 불필요한 DXGI 리소스는 즉시 해제함.
- **UI**: egui / winit (하드웨어 가속 활용 멀티 뷰포트 네이티브 UI)
  - 전체화면 포커스 차단 및 Z-Order 유지: 게임 윈도우 최소화 방지를 위해 `WS_EX_NOACTIVATE` 및 `WS_EX_TOOLWINDOW` 스타일을 주입하고, 게임 창을 오버레이 창의 Owner(`GWL_HWNDPARENT`)로 연결하여 전체 창 모드(Borderless Fullscreen) 플레이 시 드래그 앤 드롭 후 포커스가 게임으로 복귀해도 오버레이가 항상 최상단에 물리도록 보장함.
  - 오버레이 스냅과 드래그 제어: 오버레이 고정 스냅(좌상/우상/좌하/우하단) 정책과 드래그 활성화 여부는 라이트 모드와 무관하게 `overlay.position.snap` 설정에 의해 직교적으로 통제됨. 스냅이 활성화되면 드래그가 잠기고, 수동("manual") 상태일 때는 일반/라이트 모드 관계없이 자유로운 드래그 이동이 가능하며 기존 "position.x/y" 좌표로 파일에 보존됨.
- **데이터**: V-Archive DB (JSON) 및 로컬 기록 DB (SQLite)

---

# Core Constraints

- 메모리 접근 / 프로세스 인젝션 금지 (화면 캡처 및 Win32 API 추적 방식 유지)
- 인게임 성능 영향 최소화 (최우선 과제)
- 자가 업데이트 및 락 제어: 업데이트 후 재시작 시 중복 실행 락(Named Mutex) 해제 지연으로 새 인스턴스가 조기 종료되는 것을 방지하기 위해, 부모 프로세스의 락 가드(`SingleInstanceGuard`)를 명시적으로 `drop()`한 후 새 프로세스를 spawn하고 기존 프로세스를 즉시 종료하는 안전한 재시작 워크플로우를 유지함.
- Python 레거시 코드 완전 제거 및 순수 Rust 코드베이스로 전환 완료 (`rust/` workspace)
- Windows 10 (버전 1809) / 11 64-bit 환경 전용 (Windows OCR API 및 Win32 API 의존성)
- 기존 사용자 파일과의 호환성 유지:
  - `settings.user.json` (사용자 설정 델타 저장)
  - `cache/record.db` (로컬 플레이 기록 SQLite DB)
  - `cache/songs.json` (V-Archive 곡 DB)
  - `cache/image_index.db` (곡 재킷 매칭용 DB)

---

# Current Architecture

## Workspace Crate 구조
- `overmax_app`: 메인 애플리케이션 (`egui/winit` 기반 GUI, 화면 캡처 루프, 디텍션 워커, 앱/DB 자가 업데이트)
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

## 1. 씬 감지 및 동적 ROI (Scene-Aware ROI)
- **로고 OCR 감지**: `logo` ROI 영역에 대해 Windows OCR 멀티패스를 수행 (Color → Grayscale → Binarized → Binarized Inverted 순서로 시도, 첫 번째 매칭 성공 시 즉시 반환).
  - 키워드 매칭: `FREESTYLE` → `SceneType::Freestyle`, `ONLINE` → `SceneType::Online`, 전 패스 매칭 실패 → `SceneType::Unknown`.
- **동적 ROI 전환**: `RoiManager`가 감지된 씬(`SceneType`)에 따라 최적의 ROI 세트(Freestyle / Online)를 동적으로 전환.
  - `logo` ROI는 씬과 독립적으로 상단 고정 좌표를 가지며, 씬 판별의 트리거 역할을 수행.
- **히스테리시스 버퍼**: `HysteresisBuffer`를 통해 선곡 화면 진입/이탈 판정 및 신뢰도(Confidence) 계산.

## 2. 곡 인식 (Song Recognition)
- **재킷 이미지 매칭**: `ImageIndexDb`를 통해 캡처된 재킷 영역과 미리 색인된 곡 재킷의 유사도를 계산.
- **Rust Native CV**: `overmax_cv`를 통해 Perceptual Hash + HOG 방식을 사용한 재킷 매칭 및 검색 지원.

## 3. 원자적 상태 감지 및 안정화 (Atomic Play Context Sync)
- **PlayState 감지**:
  - **버튼 모드 (Button Mode)**: `btn_mode` ROI의 평균 BGR 색상과 미리 정의된 대표색(4B/5B/6B/8B)의 Euclidean 거리가 60 이하인 모드 중 최적 매칭값 선택.
  - **난이도 (Difficulty)**: 각 난이도 패널 ROI(NM/HD/MX/SC)의 평균 밝기를 계산. 상위 1위 밝기가 최소 밝기(45) 이상이고 2위와의 차이(margin)가 15.0 이상일 때 유효(confident)한 난이도로 판정.
  - **Max Combo**: `max_combo_badge` ROI의 평균 밝기가 160 이상일 때 True.
  - **Rate**: `rate` ROI 영역의 Windows OCR 멀티패스(Color → Grayscale → Grayscale Inverted) 결과를 실수값(`f32`)으로 실시간 수집. 유효 파싱값이 나온 첫 번째 패스 결과를 채택.
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
3. **명시적 Null 처리**: `rate` 수집값은 `Option<f32>`로 표현되어 미플레이(`0.0` 또는 `None` 처리)와 명시적으로 구분되어야 한다.
4. **곡 ID 예외**: `song_id == 0`은 유효한 곡 ID로 처리한다. 곡 정보가 아예 없는 경우는 `Option::None`이어야 한다.
5. **설정값 유효성 검증**: 사용자 설정 저장 시 반드시 delta 형식을 유지하고 값의 범위를 normalize/clamp 처리한다.

---

# Future Focus

1. **감지 씬(Scene) 다양화**:
   - FREESTYLE 및 ONLINE 대기방 외에도 래더 매칭 씬이나 결과 화면 등 감지 가능 범위를 추가 확장 (`SceneType::LadderMatch` 등).
2. **전체화면(Fullscreen) 호환성 검증 (완료)**:
   - `AdaptiveCaptureEngine` 동적 위임 연동(DXGI 캡처 백엔드) 및 포커스 차단용 Win32 스타일 적용 완료. 전체 창 모드(Borderless Fullscreen)에서 드래그 종료 후 포커스 복원 시 Z-Order 밀림 현상을 Win32 Owner 윈도우 연동(`GWL_HWNDPARENT`)을 통해 해결 완료. 단, OS 설계 제약 및 안티치트 충돌로 인해 하드웨어 독점 전체화면(Exclusive Fullscreen) 모드에서의 오버레이 표시는 지원하지 않으며, 보더리스 모드 실행이 필수 권장 사항임.
3. **OBS 방송 송출용 화면 모드 (OBS Mode)**:
   - 인터넷 방송 스트리머들을 위해 크로마키(Chroma key) 전용 스킨이나 OBS에서 캡처/배치가 편리한 방송 특화 레이아웃 모드 지원.
4. **V-Archive 클라이언트 완전 대체 (장기 목표)**:
   - 공식 데스크톱 클라이언트의 도움 없이 Overmax 자체 앱 내에서 플레이 기록 수집부터 V-Archive 연동 및 백업 업로드까지 전담하는 올인원 클라이언트 구현.
5. **HOG 피처 데이터베이스 갱신 및 재빌드**:
   - 로컬 이미지 왜곡으로 인한 HOG 유사도 저하 근본 해결 및 매칭 임계치를 기존 값(`0.85`)으로 원복하기 위한 피처 일괄 갱신.
6. **마우스 호버 시 투명도 반응 지연 및 OS 수준 깜빡임 개선**:
   - egui의 MousePassthrough 전환 시 Win32 EXSTYLE(WS_EX_TRANSPARENT) 비트 변경과 Win32 Layered Window Opacity 적용 간의 OS 수준 동기화 시차로 인한 깜빡임/불투명 고착 현상을 완전히 해결하기 위한 렌더 훅 기법 및 윈도우 스타일 동기화 로직 추가 튜닝.
