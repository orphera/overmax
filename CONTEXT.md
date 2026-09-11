# Context: Overmax Development

이 문서는 Overmax 프로젝트의 현재 상태, 설계 결정 사항, 그리고 시스템 스펙을 기록한다.

---

# System Overview

Overmax는 DJMAX RESPECT V의 화면을 실시간으로 분석하여, 현재 선택된 곡의 난이도별 정보를 오버레이로 보여주는 도구이다.

- **현재 Windows 인식 방식**: 화면 캡처 + Rust 네이티브 CV 이미지 매칭 (`overmax_cv`) + OCR (Windows OCR)
  - _Windows 캡처 엔진_: GDI 캡처 엔진 및 DXGI Desktop Duplication 캡처 엔진을 감싸는 `AdaptiveCaptureEngine` Facade 구성. GDI 백엔드가 안정성 기본값으로 작동하며 설정창에서 DXGI로 런타임 스위칭 가능. DXGI 캡처 시 DXGI 1.6 `IDXGIOutput6::GetDesc1()`을 통해 디스플레이의 실시간 색역(ColorSpace) 및 하드웨어 피크 휘도(`MaxLuminance`)를 사전 감지하여, SDR 환경에서는 네이티브 `B8G8R8A8_UNORM` 무변환 1:1 직행을 보장함. Windows HDR 활성화 디스플레이(scRGB `R16G16B16A16_FLOAT`)에서는 DJMAX 엔진의 내부 DCI-P3 (D65) 렌더링 색역에 맞춘 $\mathbf{M}_{709 \to \text{P3}}$ 역변환 행렬과 **$C^1$ 연속 2-Stage Rational Spline 역톤매핑 커브** 및 1,025-엔트리 Fast sRGB OETF 테이블(1KB L1 캐시 상주)을 결합함. 특히 단일 피크 비례식의 로컬 과적합을 방지하기 위해 Windows OS의 SDR 콘텐츠 백색 레벨(Anchor 1: Win32 CCD `detect_monitor_sdr_white_level`, 기본값 3.0 = 240 nits)과 패널 피크 휘도(Anchor 2: DXGI 1.6 `MaxLuminance`, 기본값 4.88)를 분리하는 **2-Anchor 일반화 파이프라인**을 구축함. 이를 통해 임의의 피크 패널(DisplayHDR 1000 등)에서도 중간톤 자켓 복원 기준($scale_{mid} = A_{\text{sdr}} \times \frac{4.0}{3.0}$, $V_{knee} = A_{\text{sdr}} \times \frac{2.2}{3.0}$)이 흔들리지 않고 고휘도 폰트(Score 1,000,000점 / Rate 100.00%) 주변 블룸을 점근 숄더로 매끄럽게 수렴 압축하여 scRGB 음수 채널 클리핑 방지와 100% 전수 인식을 완벽 달성함. 또한 자켓 매칭 파이프라인에 **동적 신뢰도 결합(Dynamic Confidence Fusion)** 및 현실화된 L1 정규화 분모(4096.0)를 도입하여, 단색 배경 로고형 자켓(메이플스토리2 등)에서 양자화 단층으로 인한 히스토그램 페널티를 방어하고 형태 지문(pHash/dHash/aHash) 신뢰도에 따라 자켓 평균 유사도를 0.9257로 극대화함. 또한 글로벌 콘트라스트 이진화 비율을 72%로 정밀 튜닝하여 1440p 다운샘플링 블러 시 '8'의 가로 허리선 보존과 100만 점/Rate 소수점 분리를 동시에 완벽 달성함. 프리스타일 결과창의 상단 `mode_colorbar` 일치성 우선 판정을 통해 PlayerPanel 엣지 간섭으로 인한 오픈매치(`ResultOpen2`) 거짓 양성을 원천 차단함. `CreateDXGIFactory1` 기반 활성 디스플레이 어댑터 자동 탐색을 통해 듀얼 GPU(iGPU + 외장 dGPU) 환경을 완벽 지원하며, multi-monitor Output 자동 탐색 및 가상 스크린 좌표 오프셋 변환을 통해 서브 모니터 구동을 완벽 지원함. 여기에 512×512 GPU ROI Atlas와 핑퐁 더블 버퍼링(Double-Buffered Staging Textures)을 구축하여 1080p(8.3MB) 복사 병목 및 GPU 스톨을 소거, 실측 캡처 지연을 4.50ms에서 0.62ms(86.2% 단축, 7.2배 고속화)로 서브밀리초화함. 비-1080p 환경(1440p, 4K, 울트라와이드)은 단일 패스 GPU Normalizer(Draw Quad)를 통해 CPU 전송량을 1MB로 고정함.
- **현재 Windows UI**: egui / winit (하드웨어 가속 활용 멀티 뷰포트 네이티브 UI)
  - _ODDS 다이얼로그 디자인 시스템 분리_: 240x160 인게임 HUD 전용 컴팩트 테마(`overlay_theme.rs`)와 독립된 데스크톱 다이얼로그 시스템(`dialog_theme.rs` - Overmax Desktop Dialog System)을 구축하여 14px/12px 타이포그래피, 32px 표준 컨트롤 높이, 카드 배경 및 2행 전폭 입력 폼(`field_row`), RTL 슬라이더(`rtl_slider`)를 표준화함.
  - _4대 탭 IA 구조화_: 설정창을 일반/추천/V-Archive/고급 4개 탭으로 분리하고, V-Archive 연결/업로드 섹션 분리, 세그먼트 버튼 1열 선택 등 깔끔한 데스크톱 사용자 경험을 제공함.
  - 전체화면 포커스 차단 및 DWM 비클라이언트 테두리/캡션 제거: 게임 윈도우 최소화 방지를 위해 `WS_EX_NOACTIVATE` 및 `WS_EX_TOOLWINDOW`, `WS_EX_LAYERED`, `WS_POPUP` 스타일을 적용하고, `SetWindowSubclass`로 `WM_NCCALCSIZE`를 가로채 비클라이언트 영역을 0으로 강제함으로써 Windows 11 DWM의 상단 1px 테두리 선 및 우상단 네이티브 캡션 버튼("- ㅁ X") 합성을 원천 차단함. 비활성 시 topmost 해제로 인한 깜빡임을 막기 위해 `is_active` 상태 검증 캐싱을 정밀화하고, `cached_game_hwnd`를 이용해 매 프레임 `FindWindowW` 오버헤드를 차단함.
  - 오버레이 스냅과 스크린 앵커 드래그 제어: 마우스 드래그 시 Win32 비클라이언트 메시지(`WM_NCLBUTTONDOWN`)나 비동기 뷰포트 델타를 쓰지 않고, 드래그 시작 시점의 스크린 절대 좌표와 창 위치를 앵커로 잡는 픽셀 완벽 스크린 앵커 드래그(`PlatformState::handle_screen_drag`)를 적용하여 1픽셀의 오차 없이 커서를 100% 추종하며 타이틀바 헤더 노출을 방지함. 구석 고정(Snap) 시에는 `try_lock()`으로 백그라운드 스레드와의 락 경합을 방지하고, 기하 구조 캐시(`prev_snap_geometry`)를 적용하여 좌표 변화가 없을 때는 `SetWindowPos` 호출을 생략(0회)함.
- **데이터 및 저장소 경로 추상화**: `overmax_data::AppPaths` — V-Archive DB (JSON) 및 로컬 기록 DB (SQLite). Portable 모드(바이너리 상대 경로)와 Installed/MSIX 모드(`%LOCALAPPDATA%\Overmax\` / Linux `XDG_DATA_HOME/overmax`) 듀얼 경로 지원. Win32 `GetCurrentPackageFullName` 무비용 패키지 감지, `.portable` 마커/디렉터리 권한 기반 자동 모드 판별, 초기 번들 에셋 시딩 및 포터블 데이터 마이그레이션(`ensure_dirs_and_seed`) 지원.
- **배포 및 패키징 (Desktop Bridge / MSIX)**: Windows Desktop Bridge(Centennial) 기반 `AppxManifest.xml.template` 및 규격 비주얼 에셋(`packaging/msix/Assets/`)과 자동화 스크립트(`scripts/package-msix.ps1`) 구축. `runFullTrust` 권한, Windows SDK `MakeAppx`/`SignTool` 연동, SemVer ➔ MSIX `Major.Minor.Build.0` 규격화(스토어 승인 정책에 따라 Revision=0 고정), Microsoft Store 등록 메타데이터 및 정책 대응 가이드(`docs/store/STORE_LISTING.md`, `docs/store/PRIVACY.md`) 완비 및 GitHub Actions CI 워크플로우(`ci.yml`) MSIX 아티팩트 빌드 연동.
- **외부 연동 (로컬 IPC)**: `overmax_app::system::ipc_server` — std-only 최소 HTTP 서버(의존성 0) 기반 단일 loopback 포트에서 SSE 이벤트 푸시(`GET /events`)와 JSON-RPC 2.0 RPC(`POST /rpc`)를 동시 제공. 프로토콜 ID는 `overmax-ipc/1`이며 기본 OFF(`ipc.enabled=false`), 기본 포트 30110(허용 대역 30100~30199, 바인딩 실패 시 대역 내 순차 스캔 → 전부 실패하면 fail-closed 비활성). 실제 확정 포트는 `cache/ipc_endpoint.json`(원자적 교체)으로 노출.


---

# Core Constraints

- 메모리 접근 / 프로세스 인젝션 금지 (화면 캡처와 OS 창 API 추적만 허용)
- 인게임 성능 영향 최소화 (최우선 과제)
- 자가 업데이트 및 락 제어: 업데이트 후 재시작 시 중복 실행 락(Named Mutex) 해제 지연으로 새 인스턴스가 조기 종료되는 것을 방지하기 위해, 부모 프로세스의 락 가드(`SingleInstanceGuard`)를 명시적으로 `drop()`한 후 새 프로세스를 spawn하고 기존 프로세스를 즉시 종료하는 안전한 재시작 워크플로우를 유지함.
- Python 레거시 코드 완전 제거 및 순수 Rust 코드베이스로 전환 완료 (`rust/` workspace)
- 스팀(Steam) 경로 탐색 및 계정 연동: V-Archive 연동 등을 위한 스팀 계정 정보(`loginusers.vdf`)를 탐색할 때 Windows는 기본 경로와 HKCU/HKLM 레지스트리, 실행 중인 `steam.exe` 순으로 조회한다. Linux는 `XDG_DATA_HOME`, native Steam 기본 경로, Flatpak Steam 데이터 경로 순으로 조회한다.
- 현재 Windows 릴리스 및 실동작 지원 기준은 Windows 10 (버전 1809) / 11 64-bit이다.
- 기존 사용자 파일과의 호환성 유지:
  - `settings.user.json` (사용자 설정 델타 저장)
  - `cache/record.db` (로컬 플레이 기록 SQLite DB)
  - `cache/songs.json` (V-Archive 곡 DB)
  - `cache/image_index.db` (곡 재킷 매칭용 DB)

---

# Linux Support

- **현재 상태**: Linux 핵심 실행 경로(창 추적 → 캡처 → 기존 디텍션 파이프라인 → native overlay)와 배포 번들 생성, hosted CI 검증이 연결되어 있다. Windows와 같은 범용 Linux 지원은 아니며 아래 범위만 지원 대상으로 본다.
- **지원 범위**: x86_64, glibc 2.35 이상, Wayland `wlr-layer-shell`과 fractional scale용 `wp_viewporter`/`wp_fractional_scale_v1`, XWayland의 XComposite 0.2 이상과 MIT-SHM 1.2 이상, Vulkan, fontconfig와 한글 글꼴이 필요하다. 게임과 앱은 같은 `DISPLAY`에서 실행한다. borderless fullscreen은 다중 출력과 fractional scaling을 지원한다. 창모드는 단일 출력에서 캡처와 인식을 지원하되 오버레이는 화면 기준 수동 배치만 지원한다.
- **추적 및 캡처**: EWMH exact-title로 단일 X11 window를 선택하고 XComposite redirect와 MIT-SHM buffer는 유지한다. XWayland의 backing pixmap 교체로 frozen frame이 발생하지 않도록 named pixmap은 매 캡처마다 재획득·해제한다. 창 위치 변경은 verified history를 유지하고 크기 변경은 즉시 pipeline을 reset하여 새 ROI scale에서 재안정화한다.
- **오버레이**: Linux 전용 layer-shell surface가 공용 overlay snapshot을 렌더링한다. exact-title foreign-toplevel의 committed entered-output으로 대상을 우선 선택하고, 확정할 수 없으면 게임 창과 Wayland output의 overlap 기반 선택으로 fallback한다. 선택 후 배치와 margin에는 output-local logical geometry만 사용한다. fractional scale은 compositor preferred scale의 120분율 값을 `wp_viewporter` destination과 실제 wgpu buffer 크기에 반영한다. borderless fullscreen에서는 기존 snap 설정을 적용하고, 창모드에서는 XWayland 좌표를 margin에 사용하지 않고 저장된 화면 기준 수동 위치만 적용한다. background window와 `SceneType::Unknown`에서는 투명 버퍼와 empty input region으로 숨기며, egui의 즉시·지연 repaint 요청을 Wayland frame callback과 poll timeout으로 처리한다. foreign-toplevel의 activated/fullscreen 상태를 우선 사용하되, protocol 부재·중복 title·target close·manager 종료 시 기존 X11/EWMH 3-state 판정으로 fallback한다.
- **오류 처리**: 일시적 window/pixmap 오류는 다음 tick에 재시도하고, X11 transport 오류만 tracker와 capturer를 재연결한다. 지원하지 않는 extension·pixel layout 같은 영구 capability 오류는 fail closed 상태를 표시하고 재연결 루프를 중단한다.
- **단일 인스턴스**: `XDG_RUNTIME_DIR/overmax.lock`을 프로세스 수명 동안 보유하여 캡처 워커, overlay, 설정 및 SQLite 캐시의 중복 실행을 방지한다.
- **배포 및 CI**: 공식 x86_64 Linux tarball은 Ubuntu 22.04/glibc 2.35 ABI 기준의 고정 CI 환경에서 생성한다. checksum, 번들 레이아웃, 실행 권한, headless `--version`, 동적 라이브러리와 GLIBC symbol 상한을 패키징 직후 검사하고, 실제 updater archive 계약으로 실행 파일 교체 및 사용자 설정·캐시 보존을 테스트한다. Linux/Windows build·test와 Linux clippy를 `--locked`로 실행하고, Xvfb+Openbox에서 window tracker, XComposite/MIT-SHM lifecycle 및 extension 부재 fail-closed 경로를 검증한다. 앱 시작 시 GitHub Releases의 Linux tarball에서 실행 파일만 원자적으로 교체하며, 사용자 승인 후 단일 인스턴스 락을 해제하고 재시작한다.
- **실행 편의성**: 설정의 System 탭에서 현재 실행 파일과 실행 디렉터리를 사용하는 XDG 사용자 앱 메뉴용 `overmax.desktop`을 생성한다. Steam 시작 옵션은 Overmax를 백그라운드로 실행하되 `%command%` 게임 프로세스를 전면에 유지한다.
- **미지원 범위**: Gamescope/Steam Deck Gaming Mode, native Wayland 게임 surface, XWayland 또는 `wlr-layer-shell`이 없는 세션, 다중 출력의 창모드, 창모드 자동 추적, non-SHM 캡처 fallback과 시스템 트레이는 현재 지원하지 않는다. X11 overlay fallback은 기존 eframe/winit X11 backend로 구현 가능하지만 event loop 생성 전 capability probe와 별도 root-overlay 경로가 필요하므로 지원 대상이 생길 때까지 보류한다.
- **호환 원칙**: Linux 구현은 플랫폼 전용 코드와 공용 계약의 최소 확장만 허용한다. 공용 인식 로직, history 기반 안정화, 사용자 설정과 DB 구조의 기존 호환성을 Linux 검증 목적으로 변경하지 않는다.

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
   ├── Overlay (Windows eframe viewport / Linux native layer-shell surface)
   ├── Settings / Sync / Debug Windows (설정 변경, V-Archive 기록 동기화, 실시간 로그)
   ├── Channel Receiver (디텍션 결과 수신 및 UI 상태 반영)
   │      └── drain_detection_results(): IPC 이벤트 발행 관찰자 (§6.2 — 파이프라인 무변경)
   │              └─ try_send(bounded 64) → [system::transport::sse] → SSE 클라이언트별 writer fan-out
   ├── IPC Command Receiver (set_overlay_visibility 등 GUI 제어 명령 수신)
   └── IpcPublisher / IpcServerHandle (shutdown) / BoundPortSlot (런타임 상태)
            ▲
            │ (mpsc channel)
            ▼
[Detection Worker Thread]
   ├── WindowTracker: Win32 또는 X11/XWayland exact-title 추적
   ├── ScreenCapture: Windows GDI/DXGI 또는 Linux XComposite + MIT-SHM
   └── DetectionPipeline
        ├── OcrDetector: 1-pass OCR fallback/검증 → rate 등 제한된 텍스트 값 추출
        ├── ImageIndexDb: overmax_cv (HOG + Hash 매칭 -> song_id 탐색)
        └── PlayStateDetector: (버튼 모드, 난이도, 맥스콤보 감지)

[Inbound Transport Subsystem (`overmax_app::system::transport`)]
   ├── [Loopback HTTP Manager Thread] ← std-only TCP 리스너 (30100~30199 대역), DNS Rebinding 가드, 엔드포인트 파일 동기화, 라우팅
   ├── [SSE Hub Thread]              ← 도메인 이벤트 SSE 프레이밍 직렬화, 연결별 단조 seq 할당, 15초 하트비트
   └── [SSE Client Threads]          ← 16개 한도, 논블로킹 fan-out, dead connection 자동 회수

[Outbound Gateway Subsystem (`overmax_data::gateway`)]
   ├── GatewayHttpClient: reqwest 커넥션 풀링, User-Agent 및 타임아웃 프로파일 일원화
   ├── VArchiveGateway: V-Archive API 점수 업로드 및 기록 조회 Facade
   ├── AssetDownloadGateway: 캐시/릴리즈 다운로드 Facade (BOM 제거)
   └── RecommendProviderGateway: 외부 추천 프로바이더 Facade
```

---

# Detection Pipeline & State Handling

## 1. 초저지연 화면 캡처 및 아틀라스 파이프라인 (Sub-millisecond GPU ROI Atlas & Double Buffering)

- **$512 \times 512$ GPU ROI Atlas**: 1080p 전체 화면(8.3MB)을 CPU RAM으로 전송하고 크롭하던 대역폭 병목을 원천 해소하기 위해, 디텍션에 필요한 43개 ROI(240,098 px)를 $512 \times 512$(1MB) 텍스처 내에 100% 무손실 1:1 패킹한 정적 슬롯 테이블(`atlas_layout.rs`) 및 $O(1)$ 정적 점프 테이블 트랜슬레이터(`atlas_translator.rs`)를 도입했습니다.
- **D3D11 하드웨어 직접 복사 (Zero Draw Call)**: 1080p 16:9 환경에서는 셰이더 및 Render Target 없이, 백버퍼에서 Staging 텍스처로 `CopySubresourceRegion`을 43회 직행(< 50 µs)하여 VRAM 내부 복사 비용을 극소화했습니다.
- **핑퐁 더블 버퍼링 (Double-Buffered Staging Textures) & GPU 스톨 0ms 소거**: 동기식 `context.Map`에 의한 4~5ms의 GPU 파이프라인 대기 스톨을 소거하기 위해 2개의 Staging 텍스처를 교대로 운용합니다. 새 프레임 획득 시 GPU 복사 후 `context.Flush()`로 비동기 DMA 전송을 시작하고, CPU는 이전 틱에 이미 복사가 완료된 Staging 버퍼를 즉시 `Map`하여 GPU 대기 시간을 0ms로 소거했습니다.
- **3단계 정량 기여도 완전 분리 판정 (Attribution Analysis)**:
  - **[A] main (단일 1080p, 4.50ms) ➔ [B] fullframe-db (더블 1080p, 3.17ms)**: 더블버퍼링의 순수 기여도는 **-1.33ms (34.3% 비중)** 로 GPU 동기화 대기를 소거하지만, 8.3MB 풀프레임 복사 비용으로 인해 3.17ms 잔존.
  - **[B] fullframe-db (더블 1080p, 3.17ms) ➔ [C] atlas-db (더블 512×512, 0.62ms)**: 아틀라스의 순수 기여도는 **-2.55ms (65.7% 비중)** 로 메모리 복사량을 87.5%(8.3MB ➔ 1.0MB) 절감하여 비로소 **0.62ms(P50: 0.63ms, P95: 0.80ms)** 의 서브밀리초 진입 달성.
  - 아틀라스의 기여도가 더블버퍼링보다 약 2배 크며, 더블버퍼링 단독으로는 3ms 한계를 넘을 수 없으므로 4K(33MB) 대역폭 방어 및 극저지연을 위해 아틀라스가 필수불가결함을 실측으로 완전 입증했습니다.
- **조건부 GPU Normalizer (비-1080p 안전망)**: 1080p에서는 0-Cost 바이패스하며, 1440p, 4K, 21:9 울트라와이드 등 비-1080p 환경 감지 시에만 단일 패스 Fullscreen Triangle 셰이더(`normalizer.rs`)와 Bilinear Sampler를 가동하여 1080p로 렌더한 후 아틀라스로 공급함으로써 4K 환경에서도 CPU 전송량을 1MB로 엄격 고정합니다.
- **DXGI HDR (scRGB) 역변환 파이프라인 및 Win32 CCD API 동적 화이트 레벨 감지 (64KB LUT)**: Windows DWM이 scRGB(FP16) 버퍼에서 SDR 100% 백색(255)을 모니터 SDR 밝기 슬라이더에 따라 비례 증폭함을 실측 규명했습니다. Win32 Connecting and Configuring Displays (CCD) API(`DisplayConfigGetDeviceInfo(DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL)`)를 통해 출력 모니터의 실제 SDR 화이트 레벨(1.0=80 nits ~ 7.5=600 nits)을 자동 감지하고, 단 0.05ms만에 이에 맞춤화된 64KB `[u8; 65536]` LUT를 동적 생성합니다. 순수 선형 정규화($C_{lin} = \text{clamp}(C_{raw} / W, 0, 1)$) 및 표준 sRGB OETF 감마 역변환을 유지하여 비선형 Reinhard 톤매핑의 계조 왜곡(중간톤 폭발로 인한 자켓/씬 인식 실패)을 배제하고, 무설정(Zero-Config) 자동 감지 + `settings.json` 수동 오버라이드 + 5.168 안전 폴백 체계로 HDR 모니터 환경에서의 색상 복원 및 0.38ms 초고속 변환 지연을 지원합니다. (※ 일부 모드 및 테마 환경에 대한 실기 검증 진행 중)

## 2. 프레임 제어 및 쿨다운 스케줄링 (Centralized Control & Cooldowns)

- **Window Tracker 동적 폴링**: DJMAX Respect V 창의 위치 및 포커스를 조회하는 Win32 시스템 콜 오버헤드를 막기 위해 `WindowQueryScheduler`가 주기적으로 호출을 차단합니다. 창 드래그 중인 경우 `16ms`(60FPS), 창이 정지 상태인 경우 `300ms`, 창 미발견 시 `1000ms`로 주기를 자동 변환합니다.
- **DXGI 재생성 쿨다운**: `AdaptiveCaptureEngine`이 DXGI 캡처에 실패하여 GDI로 폴백할 시, 매 프레임 재생성을 시도하지 않고 최소 **3초**의 쿨다운 간격을 보장하여 CPU 스팸 루프를 차단합니다.
- **템플릿 매칭 픽셀 체크섬 조기 리턴 (Early Return)**: `PlayStateDetector`가 `rate` 영역을 인식할 때 매 프레임 `crop_roi` 및 썸네일을 힙에 생성하지 않고, 원본 버퍼 상에서 즉각 픽셀 값을 건너뛰어 합산하는 `compute_pixel_checksum`을 호출합니다. 이전 체크섬과 차이가 50 이하이고 캐시 강제 만료 시간(5초)이 지나지 않았다면 템플릿 매칭과 이미지 크롭 호출 자체를 바이패스합니다. 실제 템플릿 매칭 분석은 값이 바뀌었을 때 최소 **200ms** 간격으로만 실행됩니다.
- **Zero-Copy `ImageView` 기반 ROI 크롭**: 2D 이미지의 Sub-ROI를 자를 때마다 매 프레임 발생하던 불필요한 `Vec` 힙 할당(`malloc`) 및 픽셀 데이터 복사(`memcpy`) 오버헤드를 소거하기 위해 Stride 기반의 Zero-Copy `ImageView` 인터페이스를 도입했습니다. ROI 산술 연산을 $O(1)$로 처리하고, 소유권이 영속적으로 필요한 시점(썸네일/캐싱/저장)에만 명시적으로 `.to_image_region()`을 구하도록 통일하여 CPU 부하와 메모리 핑퐁을 차단했습니다.
- **분석 루프 Sleep 제어 및 설정 연동**: `DetectionWorker` 분석 스레드는 활성 송셀렉트 시 기본 `120ms` (`active_sleep_ms`), 백그라운드 시 기본 `500ms` (`background_sleep_ms`) 동안 sleep하도록 설정에 연동되어 조율됩니다.
- **egui 마우스 호버 렌더링 스팸 억제**: 비활성 창 상태에서 십자선 소프트웨어 커서 렌더링을 위해 마우스 호버 시 매 프레임 `request_repaint()`를 스팸하던 문제를 해결하여, 마우스 이동 또는 드래그가 감지된 경우에만 repainting하도록 억제했습니다.

## 3. 씬 감지 및 동적 ROI (Scene-Aware ROI)

- **재킷 엣지/유사도 기반 씬 우선 판독**: 결과창(Result), 오픈매치(OpenMatch), 프리스타일(Freestyle) 씬의 경우, 재킷 영역의 엣지 강도(JACKET_EDGE_THRESHOLD = 15.0) 또는 우측의 곡 카테고리 띠(5x60) 영역의 단색 감지(check_category_band_solid)가 활성화되는 경우에 한해 재킷 이미지 매칭을 시도합니다. 이때 사용되는 재킷 매칭 임계값은 설정 파일의 `similarity_threshold` 값을 모든 씬에서 오프셋 없이 100% 동일하게 연동하여 사용합니다. 매칭에 성공하면 즉시 해당 씬과 곡 ID를 확정하여 씬 감지 반응성을 대폭 개선하고 CPU 부하를 경감합니다.
- **100% Pure Rust CV 템플릿 매칭 (Windows OCR 완전 제거)**: Windows OCR 및 WinRT COM 의존성을 전면 삭제하고 Pure Rust Native 템플릿 매칭(`detector::templates`)으로 로고 및 씬 판별을 단일화하여 무의미한 OS 의존성과 오버헤드를 완전히 차단했습니다.
- **동적 ROI 전환**: `RoiManager`가 감지된 씬(`SceneType`)에 따라 최적의 ROI 세트(Freestyle / Online)를 동적으로 전환.
  - `logo` ROI는 씬과 독립적으로 상단 고정 좌표를 가지며, 씬 판별의 트리거 역할을 수행.
- **히스테리시스 버퍼**: `HysteresisBuffer`를 통해 선곡 화면 진입/이탈 판정 및 신뢰도(Confidence) 계산.

## 4. 곡 인식 (Song Recognition)

- **재킷 이미지 매칭**: `ImageIndexDb`를 통해 캡처된 재킷 영역과 미리 색인된 곡 재킷의 유사도를 계산.
- **Rust Native CV**: `overmax_cv`를 통해 1차 u64 해시 Early Exit (Hamming <= 42) + 2차 2x2 분할 그리드 히스토그램 L1 벌점 WTA 방식의 고속 이미지 매칭 연산을 지원합니다. 무거운 HOG 코사인 유사도 매칭을 100% 제거하고 싱글 스레드 순차 최적화를 실현하여 종합 122배(루프 연산 457배) 고속화를 달성했습니다.
- **하위 호환성 및 데이터 영속화**: 기존 DB 구조 호환성을 위해 2x2 그리드 히스토그램 데이터를 images 테이블의 metadata TEXT 컬럼에 JSON 직렬화하여 적재 및 파싱하며, 히스토그램이 없는 레거시 DB에서도 정상적으로 해시 유사도로 스위칭 동작합니다.

## 5. 원자적 상태 감지 및 안정화 (Atomic Play Context Sync)

- **PlayState 감지**:
  - **버튼 모드 (Button Mode)**: `Mode` enum (`B4`, `B5`, `B6`, `B8`) 활용. 선곡창에서는 `btn_mode` ROI의 평균 BGR 색상과 대표색의 Euclidean 거리가 60 이하인 모드를 선택하고, 결과창에서는 독립적인 모드 템플릿 매칭을 수행합니다.
  - **난이도 (Difficulty)**: `Difficulty` enum (`Normal`, `Hard`, `Maximum`, `SC`) 활용. 선곡창에서는 난이도 패널 ROI의 상대 밝기를 판정하고, 결과창에서는 독립적인 난이도 패널 템플릿 매칭을 수행합니다.
  - **Max Combo**: 결과창 및 선곡창의 `max_combo_badge` ROI 영역에 대해 사전에 수집된 대표 뱃지 이미지 템플릿과의 이미지 해시(pHash, dHash, ahash) 비교를 수행. 결과창과 선곡창 모두 가중 해밍 거리가 20.0 이하(`BADGE_MATCH_THRESHOLD`)인 경우에 한해 True로 일관되게 판정하여, 연출 그래픽 변화나 1440p 다운스케일링 픽셀 편차에 의한 오인식/누락을 완벽하게 차단 (빈 배경은 23.5 이상이므로 오탐 없음).
  - **Score & Rate Cross-Validation, MaxRGB 이진화 & ZNCC 소프트 템플릿 매칭**: 결과창 및 선곡창에서 `score` 및 `rate` ROI 영역을 추출하여 상호 교차 검증(`(Rate - Score/10,000).abs() <= 0.02`)합니다. 불일치 시 자릿수가 적고 굵은 폰트인 직접 인식 레이트를 우선하여 얇은 스코어 1자리 오독에 의한 왜곡을 차단합니다. 1) 비MaxCombo 등 빨간색 폰트의 $(R+G+B)/3$ 1/3 감쇠를 방지하는 `MaxRGB` 이진화로 대비 220+ 확보, 2) 세그먼트 경계 오염을 방지하는 정밀 단일 세그먼트 분할, 3) 원본 휘도 보존형 ZNCC 소프트 템플릿 매칭을 결합하여 QHD(1440p) 다운스케일링 환경에서도 100만 점/100.00% 및 99.95% 등 전 구간 무결점 1-Pass 인식을 보장합니다.

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
- **업로드 피드백 토스트 알림 (Toast Notification)**: ⬆ 버튼 클릭을 통한 V-Archive 단독 패턴 기록 업로드 시, 완료 및 에러 피드백을 오버레이 내부의 Detail 영역(메타 정보 줄)에 3초 동안 일시적으로 보여주는 경량 토스트 시스템 지원.

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
6. **템플릿 매칭 1-Pass 최적화**: Rate, Score 등 모든 템플릿 매칭은 단일 패스(1-Pass) 실행만 허용한다. 인게임 성능 보호를 위해 다중 패스 루프 생성을 금지하며, 오인식 대응은 HysteresisBuffer 기반의 프레임 히스토리 다수결 안정화로 해결해야 한다. (Windows OCR 의존성은 완전히 제거됨)
7. **추천 엔진 실력 모델 통계 일관성**: 추천 엔진은 Top 50 기반 통계적 실력 분포(`SkillProfile`, SC/Pad 2-Track)를 기반으로 작동하며, 선곡 화면의 커서 위치(`current_floor` 및 `use_official`)에 의해 전체 권장 레벨 및 타깃 난이도 앵커가 오염되어서는 안 된다. 회복/손풀기(`REST`) 뱃지는 물리적 안전 난이도(`Floor <= μ - 0.8σ`) 구간에서만 허용된다.

---

# Future Focus

1. **외부 연동 및 IPC 프로토콜 고도화** — **완료 (v0.5.0)**:
   - SSE 이벤트 스트리밍(`scene_detected`/`song_detected`/`play_verified` + 접속 시 `state_snapshot`)과 JSON-RPC 2.0 메서드(`get_current_context`, `get_recommendations`, `set_overlay_visibility`, `list_methods`) 구현 완료.
   - Provider 규격(`overmax-recommend/1`)과 IPC 규격(`overmax-ipc/1`)은 역할이 다른 별개 프로토콜로 유지하되, `song_id`/`mode`/`diff` 공통 와이어 어휘와 `x/1` 버저닝 문화를 공유하도록 단일화 완료 (`RecommendEntry` serde rename + 계약 고정 테스트, 프로토콜 ID 상수화).
2. **플레이어 편의성 및 인게임 유틸리티**:
   - 글로벌 단축키(Hotkeys) 지원 및 연습용 노트 레인 임시 가림막(Lane Blind) 오버레이 구현.
3. **기록 수집 및 V-Archive 자동 연동**:
   - 결과창 씬 확정 시 V-Archive API 백그라운드 자동 업로드 파이프라인 구축.
4. **감지 씬(Scene) 다양화**:
   - FREESTYLE 및 ONLINE 대기방 외에도 래더 매칭 씬이나 결과 화면 등 감지 가능 범위를 추가 확장 (`SceneType::LadderMatch` 등).

---

# Decision Logs (설계 결정 이력)

설계 결정의 배경(Why)과 상세 이력은 영역별 전문 문서로 분리하여 영속화하고 관리한다.

- 🎯 **디텍션 & CV 파이프라인**: [docs/decisions/detection_pipeline.md](docs/decisions/detection_pipeline.md)
  - 씬 감지, 자켓 매칭, 1-Pass Score 매칭, Centroid Kernel 게이트, 템플릿 매칭 등
- 🖥️ **화면 캡처 & 윈도우 추적**: [docs/decisions/capture_and_window.md](docs/decisions/capture_and_window.md)
  - DXGI/GDI 적응형 캡처 백엔드, DWM Z-Order, 네이티브 드래그, 다중 모니터, 텔레메트리 피처 등
- 💾 **데이터 저장소, 캐시 & 동기화**: [docs/decisions/data_and_sync.md](docs/decisions/data_and_sync.md)
  - SQLite RecordDB, V-Archive 증분 동기화(`since`), StartupCacheManager 비동기 기동, Provider Protocol 등
- 🎨 **UI 컴포넌트 & 다국어 (i18n)**: [docs/decisions/ui_and_i18n.md](docs/decisions/ui_and_i18n.md)
  - 0-Cost 단일 `t!` i18n 매크로, Auto-Fit 레이아웃, 오버레이 테마/투명도, 모듈형 컴포넌트 등
- 🐧 **Linux 플랫폼 지원**: [docs/decisions/linux_support.md](docs/decisions/linux_support.md) ([사용자 가이드: docs/guides/linux-support.md](docs/guides/linux-support.md))
- 🔌 **외부 연동 & IPC 서비스**: [docs/plans/2026-08-26-ipc-service-architecture.md](docs/plans/2026-08-26-ipc-service-architecture.md)
  - SSE + JSON-RPC 2.0 트랜스포트, 포트 대역 정책, 이벤트 봉투 규격, 스레드 모델 등
