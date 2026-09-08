# Overmax v0.4.1 릴리즈 노트

> v0.4.0 이후 변경 사항 (v0.4.1)

---

## 🎮 새로워진 점 및 개선 사항

### 📡 1. 실시간 외부 방송 연동 및 도구 확장 지원 (로컬 IPC 프로토콜 도입)
* **[체감 변화]**: OBS 방송 화면 오버레이, 외부 선곡 위젯, 서드파티 봇 프로그램에서 현재 플레이 중인 곡과 추천 목록 정보를 실시간으로 손쉽게 연동할 수 있습니다.
* **[개선 세부 요약]**:
  * **실시간 이벤트 스트리밍 (SSE)**: 게임 중 화면이 바뀌거나 선곡 화면에서 다른 곡으로 이동할 때, 결과창에서 점수가 확정되는 순간의 모든 플레이 정보를 0.001초 단위의 실시간 스트림(`GET /events`)으로 외부에 브로드캐스트합니다.
  * **외부 원격 제어 인터페이스 (JSON-RPC 2.0)**: 외부 도구에서 Overmax에 접속하여 현재 곡 정보 조회(`game.current_song`), 맞춤 추천 곡 목록 요청(`recommend.get_candidates`), 오버레이 표시 여부 전환(`overlay.set_visibility`) 등을 호출할 수 있는 표준 원격 호출 규격을 지원합니다.
  * **충돌 없는 안전한 단일 포트 통신**: 외부 인터넷 연결 없이 플레이어 PC 내부(Localhost `127.0.0.1`)에서만 동작하며, 포트가 중복될 경우 안전한 대역(30100~30199)을 자동으로 탐색하여 다른 프로그램과의 충돌을 방지합니다.
* 💡 **이용 팁**: `설정(⚙) > 고급 > 로컬 IPC 통신` 섹션에서 원클릭으로 켜고 끌 수 있으며, 연결된 클라이언트 수와 실제 할당된 포트 번호를 실시간 뱃지로 확인할 수 있습니다. 기본 설정은 꺼짐(OFF) 상태로 안전하게 유지됩니다.

### 📦 2. Windows 스토어(MSIX) 및 설치형 환경 지원
* **[체감 변화]**: 포터블 압축 파일(Zip) 외에도 향후 Microsoft Store 및 Windows 공식 앱 패키지(MSIX) 환경에서 시작 메뉴 검색과 원클릭 설치로 편리하게 게임에 진입할 수 있습니다.
* **[개선 세부 요약]**:
  * **스토어 런타임 환경 자동 감지**: 앱이 Windows Store/MSIX 패키지 환경에서 실행 중인지 자동으로 감지하여, 스토어 전용 보안 저장소(`%LOCALAPPDATA%\Overmax`)로 플레이 기록과 설정을 안전하게 격리·보존합니다.
  * **초기 캐시 자동 마이그레이션**: 앱을 새로 설치하더라도 번들된 기본 곡 정보와 커버 이미지 캐시를 자동으로 동기화하여 첫 실행 시의 지연 시간(Cold Start) 없이 즉시 오버레이가 작동합니다.
  * **포터블/설치형 완벽 호환**: 기존 포터블 압축본 사용자는 이전과 동일하게 실행 파일이 위치한 폴더에 모든 설정과 기록이 보존되는 포터블 모드로 그대로 유지됩니다.

### 🛡️ 3. 스토어 환경 자가 업데이터 스마트 분기
* **[체감 변화]**: 스토어 패키지로 실행할 때는 외부 파일 교체로 인한 충돌이나 보안 경고 없이 안전하게 Microsoft Store의 정식 업데이트를 안내받을 수 있습니다.
* **[개선 세부 요약]**:
  * 스토어 모드에서는 GitHub Releases 바이너리 교체 방식의 인앱 업데이터가 자동으로 비활성화되며, `설정(⚙) > 일반` 탭에 "Microsoft Store 패키지(스토어를 통해 자동으로 최신 버전이 유지됩니다)"라는 안내 뱃지가 표시되어 혼선을 방지합니다.

### 🐧 4. Linux(Steam Deck 및 Wayland) 환경 오버레이 안정성 대폭 개선
* **[체감 변화]**: Steam Deck이나 Linux(Wayland/Proton) 환경에서 화면 전환 시 오버레이가 검은색으로 남거나 게임 뒤로 가려지던 현상을 해결하여 한층 부드럽게 플레이할 수 있습니다.
* **[개선 세부 요약]**:
  * **표시 상태와 캡처 경로 분리**: 화면 표시 채널과 디텍션 분석 채널을 분리하여, 곡 선택이나 로딩 중에 창 포커스가 순간적으로 흔들려도 오버레이가 안정적으로 유지됩니다.
  * **다중 모니터 자동 감지**: 멀티 모니터 환경에서도 게임 창이 실행 중인 화면을 자동으로 감지하여 올바른 모니터에 오버레이가 정확하게 위치합니다.
  * **백그라운드 기동 대응**: 게임 창이 백그라운드에 있는 상태로 켜지더라도 오버레이가 창 위치와 크기를 즉시 파악하여 대기합니다.

### 🪟 5. 화면 전환 시 오버레이 깜빡임 및 인게임 순간 끊김(Stuttering) 완화
* **[체감 변화]**: 곡 목록에서 연주로 진입하거나 결과창으로 넘어갈 때, 오버레이가 숨겨졌다가 다시 나타나는 과정에서 발생하던 화면 깜빡임과 순간적인 프레임 드랍을 완전히 없앴습니다.
* **[개선 세부 요약]**:
  * 오버레이를 가릴 때 창 크기를 1x1로 축소하던 구형 방식을 제거하고 뷰포트 크기를 그대로 보존한 채 투명 숨김 처리하도록 최적화했습니다.
  * 화면 전환 시마다 그래픽 디바이스 리소스(DXGI 스왑체인)가 해제되고 재생성되던 GPU 부하를 원천 차단하여 부드러운 전환을 보장합니다.

### 🖥️ 6. Windows HDR(scRGB) 화면 캡처 지원(베타) 및 듀얼 GPU 화면 캡처 안정성 강화
* **[체감 변화]**: Windows HDR(High Dynamic Range) 기능이 켜진 게이밍 모니터(OLED, Mini LED 등)에서도 화면이 하얗게 날아가는(과노출) 현상을 줄이고 인식이 가능하도록 실시간 색상 복원 기술을 도입했습니다. (※ 현재 일부 버튼 모드 및 테마 환경에 대한 추가 보정 작업이 순차 진행 중입니다)
* **[개선 세부 요약]**:
  * **Windows HDR(scRGB) 실시간 역변환 LUT 도입**: Windows 디스플레이 설정의 'SDR 콘텐츠 밝기' 슬라이더 상태를 자동으로 감지하여 모니터 밝기에 맞는 색역 복원(역변환 LUT)을 0.38ms 만에 실시간 적용하는 파이프라인을 구축했습니다.
  * **SDR / HDR 스마트 듀얼 트랙 자동 분기**: 모니터의 실제 색역(ColorSpace)을 0ms 만에 감지하여 일반 SDR 모니터는 네이티브 8비트 sRGB로 1:1 직행하고, HDR 모니터는 고정밀 scRGB(FP16)로 수신하여 색상 왜곡을 완화합니다.
  * **듀얼 그래픽(iGPU + 외장 dGPU) 충돌 방지**: 고성능 외장 그래픽카드와 CPU 내장 그래픽이 함께 활성화된 PC에서 게임 화면이 실제로 출력되는 그래픽카드를 자동으로 인식하여, 화면 복사 지연 없이 즉각적으로 오버레이가 반응합니다.
  * **첫 프레임 감지 안정화**: 게임 실행 직후 캡처 타이밍 차이로 인해 구형 호환 방식(GDI)으로 잘못 전환되는 현상을 방지하여 일관된 고성능 인식을 보장합니다.
* 💡 **이용 팁**: HDR 모니터 환경에서 오버레이를 이용하실 때는 `설정(⚙) > 고급 > 성능 및 캡처 설정`에서 화면 캡처 방식을 반드시 **`DXGI`**로 설정해 주셔야 정상적인 색상 복원 파이프라인이 적용됩니다.

### ⚡ 7. 게임플레이 끊김(Stuttering) 없는 초저지연 화면 인식 (GPU 아틀라스 & 더블 버퍼링)
* **[체감 변화]**: 빠른 연타나 격렬한 연주 중에도 게임 내 프레임 드랍이나 렉 없이 화면을 부드럽게 유지하면서, 오버레이 인식 반응 속도를 기존 대비 7배 이상 끌어올렸습니다.
* **[개선 세부 요약]**:
  * **초저지연 0.62ms 캡처**: 매 프레임 게임 전체 화면(8.3MB)을 무겁게 복사하던 방식을 전면 개편하여, 화면 판독에 꼭 필요한 영역만 그래픽카드 메모리(VRAM) 안에서 압축 모아담는 512×512 아틀라스 기술과 핑퐁 더블 버퍼링을 결합했습니다.
  * **인게임 끊김 완전 차단**: 화면 캡처 지연 시간이 기존 4.5ms에서 **0.62ms(P50: 0.63ms, 86.2% 단축)** 로 수직 단축되어 1ms 미만의 서브밀리초 인식을 실현했습니다. 화면 복사로 인한 그래픽카드 동기화 지연을 0ms로 소거하여 게임 플레이 흐름을 방해하지 않습니다.
* 💡 **이용 팁**: 기본적으로 자동 활성화되어 있어 별도 조작 없이 바로 최적화 혜택을 누릴 수 있으며, `설정(⚙) > 고급 > 성능 및 캡처 설정`의 `GPU ROI Atlas 가속` 옵션으로 자유롭게 켜고 끌 수 있습니다.

### 🎯 8. 결과창 판정률(Rate) 및 스코어 인식 정확도 향상
* **[체감 변화]**: 결과창에서 특정 곡의 판정률(Rate)이나 스코어의 특정 자릿수가 비정상적으로 인식되거나 누락되던 문제를 해결하여 100% 온전한 기록 저장을 보장합니다.
* **[개선 세부 요약]**:
  * 결과창 점수 및 판정률 글자 분할 과정에서 숫자 '1'의 하단 받침대가 잘려 인식에 실패하던 엣지 케이스를 바로잡았습니다.
  * 19개 결과창 스크린샷 전수 테스트 및 실전 게임플레이에서 100.0%의 무결점 인식률을 검증 완료했습니다.

### 🖥️ 9. 1440p / 4K / 울트라와이드 모니터 고해상도 지원 강화
* **[체감 변화]**: QHD(1440p), 4K(2160p), 21:9 울트라와이드 모니터 환경에서도 고해상도로 인한 메모리 병목이나 오버레이 지연 없이 쾌적하게 동작합니다.
* **[개선 세부 요약]**:
  * 비-1080p 고해상도 환경에서는 그래픽카드 하드웨어(Direct3D 11)를 이용해 1회 단일 패스로 부드럽게 정규화(Bilinear)한 후 아틀라스로 전달합니다.
  * 4K 환경(33MB 전송 부하)에서도 CPU로 전송되는 메모리 양을 단 1MB로 엄격하게 고정하여 인게임 성능을 철저히 보호합니다.

### 🌐 10. OS 기본 언어 자동 감지 및 직관적 다국어 UX
* **[체감 변화]**: 첫 실행 시 수동으로 언어를 변경할 필요 없이, 플레이어가 사용 중인 Windows나 Linux의 언어(한국어, English, 日本語)에 맞춰 앱이 자동으로 실행됩니다.
* **[개선 세부 요약]**:
  * **OS UI 언어 자동 감지**: 앱 최초 실행 시 플레이어의 시스템 UI 표시 언어를 자동으로 판별하여 기본 언어로 설정합니다. 글로벌 플레이어도 첫 실행 시 언어 장벽 없이 즉시 쾌적하게 이용할 수 있습니다.
  * **혼란 없는 직관적 3버튼 설정**: 설정 화면의 언어 옵션에서 불필요한 '자동' 버튼의 중복 혼란을 없애고 `[한국어 | English | 日本語]` 3개 세그먼트 버튼 중 현재 시스템에 맞춰 감지된 언어에 즉시 불이 켜지도록 하여 인지 부하를 없앴습니다.
* 💡 **이용 팁**: 다른 언어를 사용하고 싶을 때는 `설정(⚙) > 일반`에서 원하는 언어 버튼을 클릭하면 즉시 변경 및 영구 저장됩니다.

### 📝 11. 커뮤니티 서열표 비고(Note) 메타 렌더링 최적화
* **[체감 변화]**: 영어 및 일본어 환경에서 번역되지 않은 한국어 서열표 메모가 오버레이 화면에 노출되어 시각적 노이즈를 유발하던 문제를 깔끔하게 해결했습니다.
* **[개선 세부 요약]**:
  * 한국어 커뮤니티 서열표의 비정형 비고 텍스트(예: "무리배치 주의", "개인차")는 한국어 환경에서만 표출되도록 분리하여, 영미권/일본 플레이어 환경에서는 황배 및 보조키 뱃지만 간결하고 깔끔하게 표출됩니다.

---

## 🛠️ 엔지니어링 & 내부 아키텍처 변경점

### 🌐 1. std-only 무의존성 로컬 IPC 서버 및 SSE/JSON-RPC 2.0 통합 아키텍처
* **[Zero-Dependency 경량 Loopback HTTP 서버]**:
  * 외부 HTTP 웹 프레임워크(actix, axum 등) 추가 없이 순수 Rust 표준 라이브러리(`std::net::TcpListener`, `std::net::TcpStream`)만으로 구현된 경량 non-blocking IPC 서버(`overmax_app::system::ipc_server`) 구축 (추가 바이너리 크기 증가 0KB).
  * `127.0.0.1` 단일 인터페이스 강제 바인딩으로 외부 네트워크 접근을 원천 차단. 기본 포트 30110 기준, 이미 사용 중일 경우 30100~30199 대역 내 순차 탐색 후 `cache/ipc_endpoint.json` 파일에 확정 포트를 원자적으로(tempfile + atomic rename) 기록.
* **[Dual Transport: Server-Sent Events (SSE) & JSON-RPC 2.0]**:
  * **SSE 스트리밍 (`GET /events`)**: `overmax-ipc/1` 규격의 단방향 이벤트 스트림. 멀티스레드 크로스 채널을 통해 `SceneChangeEvent`, `SongChangeEvent`, `VerifiedPlayEvent`를 JSON 직렬화하여 연결된 모든 클라이언트에 브로드캐스트.
  * **JSON-RPC 2.0 (`POST /rpc`)**:
    * `system.version`, `system.status`: 런타임 상태 및 버전 질의
    * `game.current_song`: 현재 인식된 곡명, 난이도, 버튼 모드 조회
    * `recommend.get_candidates`: 현재 실력 기준 스마트 추천 엔트리 목록 질의
    * `overlay.set_visibility`: 오버레이 창 표시/숨김 원격 제어

### 📂 2. 데이터 경로 추상화 레이어 (`AppPaths`) & 듀얼 모드 분기
* **[바이너리-상대 경로 vs 시스템 로컬 앱데이터 격리]**:
  * `overmax_data::AppPaths` 구조체를 신설하여 `bundle_dir`(읽기 전용 번들 에셋)과 `data_dir`(가변 사용자 데이터)을 추상화.
  * Win32 `GetCurrentPackageFullName` C-API 동적 바인딩을 통해 런타임 오버헤드 0의 정적 캐싱(`OnceLock<bool>`)으로 MSIX 패키지 실행 여부를 무비용 감지.
  * **모드 확정 우선순위 엔진**:
    1. 환경변수 `OVERMAX_DATA_DIR` 강제 오버라이드
    2. 환경변수 `OVERMAX_PORTABLE=1` 또는 `.portable` 마커 파일 확인
    3. `is_running_in_msix_package()` 감지 ➔ Installed 모드 (`%LOCALAPPDATA%\Overmax\`)
    4. 실행 디렉터리 쓰기 가능성(`is_dir_writable`) 검사 ➔ 쓰기 가능 시 Portable 모드 기본 유지
    5. 쓰기 불가(Program Files 등) 시 ➔ Installed 모드 자동 폴백
  * **Cold Start 단축 시딩 (`ensure_dirs_and_seed`)**: Installed 모드 최초 기동 시 번들된 `songs.json`, `dlcs.json`, `image_index.db`, `pattern_meta.json`을 사용자 로컬 캐시 디렉터리로 원자적 복사.

### 📦 3. Desktop Bridge(Centennial) 패키징 파이프라인 & 버전 매핑 정책
* **[Desktop Bridge AppxManifest 및 Visual Assets 규격화]**:
  * `AppxManifest.xml.template` 작성: `runFullTrust` 권한, Win32 `Windows.FullTrustApplication` 엔트리포인트 및 타일 에셋 매핑.
  * 1254×1254 고해상도 로고로부터 Bicubic 보간을 적용한 MSIX 필수 비주얼 에셋 6종 일괄 생성 (`Square44x44`, `Square150x150`, `Wide310x150`, `Square310x310`, `StoreLogo`, `SplashScreen`).
* **[원스톱 빌드/서명/사이드로딩 자동화 (`package-msix.ps1`)]**:
  * Windows 10/11 SDK 도구(`MakeAppx.exe`, `SignTool.exe`) 자동 탐색.
  * 개발용 자체 서명 인증서(`OvermaxDev.pfx`, `OvermaxDev.cer`) 자동 생성 및 `LocalMachine\TrustedPeople` 신뢰 체인 안내 로직 탑재.
  * XML 특수문자(`&` 등) 자동 이스케이프 방어.
* **[SemVer ➔ MSIX 4단위 버전 규격화 (Major.Minor.Build.0)]**:
  * Microsoft Store 패키지 승인 정책상 4번째 자리(수정 번호, Revision)는 스토어 내부 관리용으로 예약되어 있어 반드시 `0`이어야 하는 제약 반영:
    * **프리뷰/정식 공통**: `X.Y.Z(-preview<N>)` ➔ `X.Y.Z.0` (예: `0.4.1-preview1` ➔ `0.4.1.0`)
    * `scripts/package-msix.ps1`에서 `Major.Minor.Build.0`으로 자동 정규화.
* **[스토어 정책 및 CI 연동]**:
  * Microsoft Partner Center 심사용 영/한 앱 설명, 서드파티 면책 조항(Disclaimer), `runFullTrust` 소명서, 오픈소스 개인정보처리방침([`docs/store/PRIVACY.md`](../store/PRIVACY.md)) 완비.
  * GitHub Actions Windows 워크플로우에 `dist/*.msix` 아티팩트 빌드 단계 연동.

### 🐧 4. Linux Wayland Layer-Shell 오버레이 상태와 디텍션 파이프라인 계약 분리 (PR #25)
* **[오버레이 표시 상태와 캡처 상태 전달 경로 분리]**:
  * Wayland `wlr-layer-shell` 기반 오버레이 렌더 루프와 X11/XSHM 기반 화면 캡처 워커 간의 결합도를 낮추고 상태 전달 경로 분리.
  * `foreign-toplevel` 프로토콜을 통해 정확한 포커스 전이를 감지하며, 프로토콜 미지원 환경을 위한 오버랩 기반 활성 모니터(`active_output`) 자동 폴백 구축.
  * 검증된 Linux 런타임 환경 사양 문서화 ([`docs/environments/verified_linux.md`](../environments/verified_linux.md)).

### 🪟 5. 오버레이 뷰포트 크기 보존을 통한 DXGI 스왑체인 재생성 방지
* **[1x1 Resize 임시방편 소거 및 무손실 윈도우 은닉]**:
  * 오버레이 숨김 시 뷰포트를 `InnerSize(1.0, 1.0)`으로 축소하던 레거시 코드를 영구 제거.
  * 숨김 시 `SWP_HIDEWINDOW`만 발행하고 뷰포트 지오메트리를 보존함으로써, D3D12/DXGI 스왑체인 해제 및 재할당으로 인한 GPU 스파이크와 깜빡임(Flashing)을 완전히 차단.

### 🔄 6. 플랫폼 간 워커/텔레메트리/상태 전이 생명주기 일원화
* **[크로스 플랫폼 단일 규격 인터페이스 구축]**:
  * `RepaintFingerprint`: 플랫폼 분기를 제거하고 세션 컨텍스트 기반의 단일 구조체로 통합.
  * `DetectionWorker::spawn` 및 `PlatformState::new`: 생성자 시그니처를 100% 일원화하여 `native_app.rs`의 조건부 컴파일 분기 대폭 축소.
  * `detecting_output` 공통 팩토리: 기본 출력 생성을 단일화하고 Windows `tick()`에서도 치명적 캡처 에러(`CaptureErrorAction::Stop`) 전파 연동.
  * Windows 캡처 텔레메트리 연동: 캡처 소요시간(`cap_elapsed`)을 측정하여 `telemetry.log`에 p95 지연시간 및 성공률 정량 집계.
  * 씬 전이 로깅: `check_and_log_scene_transition`을 공통화하여 Linux 틱 루프에서도 동일한 진단 로그 제공.

### 🎮 7. DXGI Desktop Duplication HDR 역변환 파이프라인 및 DXGI 1.6 색역 사전 분기
* **[DXGI 1.6 IDXGIOutput6 기반 실시간 모니터 색역(ColorSpace) 사전 판별]**:
  * `DuplicateOutput1`이 단일 포맷 요청 시 DWM 레벨에서 강제 포맷 변환을 수행하여 SDR 모니터가 FP16으로 수신되거나 HDR 모니터가 B8G8R8A8로 톤매핑되는 딜레마를 원천 해소.
  * DXGI 1.6 `IDXGIOutput6::GetDesc1()`을 통해 디스플레이의 실시간 `ColorSpace`(G2084/G10/G22) 및 `BitsPerColor`를 0ms만에 사전 감지하여, SDR 환경은 네이티브 `DXGI_FORMAT_B8G8R8A8_UNORM`, HDR 환경은 scRGB `DXGI_FORMAT_R16G16B16A16_FLOAT`를 각각 1순위로 협상하도록 듀얼 트랙 구축.
* **[Win32 CCD API 기반 SDR White Level 동적 자동 감지 및 64KB 고속 LUT ($O(1)$)]**:
  * Windows DWM의 scRGB 버퍼에서 SDR 100% 백색(255)이 OS 'SDR 콘텐츠 밝기' 슬라이더에 따라 비례 증폭(1.0=80 nits ~ 7.5=600 nits)됨을 실측 규명.
  * Win32 Connecting and Configuring Displays (CCD) API(`DisplayConfigGetDeviceInfo(DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL)`)로 출력 모니터의 실제 SDR 화이트 레벨을 자동 감지하고, 단 0.05ms만에 64KB `[u8; 65536]` LUT를 동적 생성.
  * 비선형 Reinhard 톤매핑의 계조 왜곡(중간톤 폭발로 인한 자켓/씬 인식 실패)을 배제하고, 순수 선형 정규화($C_{lin} = \text{clamp}(C_{raw} / W, 0, 1)$) 및 표준 sRGB OETF 감마 역변환을 유지하여 핫루프 내 0.38ms 만에 sRGB BGRA8로 고속 변환 파이프라인 구축.
* **[디스플레이 출력 연결 어댑터 우선 바인딩 (`CreateDXGIFactory1`)]**:
  * `CreateDXGIFactory1` 및 `EnumAdapters1` 순회를 통해 실제 디스플레이 출력(`EnumOutputs(0).is_ok()`)을 소유한 하드웨어 어댑터를 우선 탐색하여 `D3D_DRIVER_TYPE_UNKNOWN`으로 D3D11 장치 생성.
  * 듀얼 GPU(iGPU vs dGPU) 환경에서 발생하던 PCI-e 버스 경유 Cross-Adapter 복사 병목 및 캡처 디바이스 불일치 방지.
* **[적응형 초기 프레임 수신 타임아웃 (`timeout_ms`)]**:
  * 최초 프레임 수신 시 `50ms` 타임아웃을 적용해 초기화 단계의 `DXGI_ERROR_WAIT_TIMEOUT(0x887A0027)` 조기 반환으로 인한 불필요한 GDI 폴백 방지.
  * 이후 프레임은 `0ms` 논블로킹 폴링을 유지하여 평균 캡처 지연시간 4.9ms 수준의 저지연 파이프라인 유지.

### 🎯 8. D3D11 GPU ROI Atlas (512×512) & 제로 드로우 콜(Zero Draw Call) 패스트패스
* **[43개 ROI 정적 2D 패킹 및 컴파일 타임 슬롯 테이블 (`atlas_layout.rs`)]**:
  * 1080p 전체 화면(8.3MB) 전송 병목을 해소하기 위해 43개 핵심 ROI(240,098 px)를 $512 \times 512$(1MB) 텍스처 내에 100% 무손실 1:1 패킹한 `pub const ATLAS_SLOTS: [AtlasSlot; 43]` 베이킹.
  * $O(1)$ 정적 점프 테이블 트랜슬레이터(`atlas_translator.rs`)를 구축하여 런타임 힙 할당(0회) 및 해시맵 조회 없이 무비용 어댑터 연동.
  * 카테고리 띠(64×60) 및 마진(22×96) 슬롯을 확장하여 씬 판독부터 결과창까지 풀 프레임 캡처 없이 512×512 아틀라스 단독으로 100% 자립 동작 완성.
* **[Zero Draw Call 1080p 하드웨어 직접 복사]**:
  * 1080p 16:9 환경에서는 셰이더 및 RTT(Render Target Texture)를 100% 바이패스하여, 백버퍼에서 VRAM Staging 텍스처로 `CopySubresourceRegion`을 43회 직행 (< 50 µs).

### ⚡ 9. 핑퐁 더블 버퍼링(Double-Buffered Staging Textures) 및 실측 기여도 분리 (A/B/C 실전 검증)
* **[Staging 핑퐁 파이프라인 및 context.Flush() 비동기화]**:
  * 5.12ms 캡처 지연의 85% 이상(~4.5ms)이 동기식 `context.Map` 호출에 따른 GPU 파이프라인 스톨(Wait for GPU)임을 프로파일링으로 규명.
  * 2개의 512×512 Staging 텍스처를 교대로 운용하고 새 프레임 복사 후 `context.Flush()`로 비동기 DMA 전송을 시작하며, CPU는 이전 틱에 완료된 버퍼를 즉시 `Map`하여 GPU 대기 시간을 0ms로 소거.
* **[3단계 실전 플레이 정량 기여도 완전 분리 판정 (The Definitive Attribution)]**:
  * **[A] `main` (단일 버퍼 1080p, 4.50ms)** ➔ **[B] `fullframe-db` (더블버퍼 1080p, 3.17ms)**: 더블버퍼링의 순수 효과는 **-1.33ms (34.3% 기여)** 로 GPU 스톨을 소거하지만, 8.3MB 풀프레임 복사 비용으로 인해 3.17ms가 잔존 한계임.
  * **[B] `fullframe-db` (더블버퍼 1080p, 3.17ms)** ➔ **[C] `atlas-db` (더블버퍼 512×512, 0.62ms)**: 아틀라스의 순수 효과는 **-2.55ms 추가 절감 (65.7% 기여)** 로 메모리 복사량을 87.5%(8.3MB ➔ 1.0MB) 절감하여 비로소 **0.62ms(P50: 0.63ms, P95: 0.80ms)** 의 서브밀리초 진입 달성.
  * 아틀라스의 기여도가 더블버퍼링보다 약 2배 더 결정적이며, 4K(33MB) 등 고해상도 대역폭 방어를 위해 아틀라스 아키텍처가 필수적임을 실측으로 완전 입증.

### 📐 10. 조건부 GPU Normalizer (비-1080p 단일 패스 Draw Quad) 및 Bilinear 리샘플러
* **[고해상도 메모리 폭탄 차단 파이프라인 (`normalizer.rs`)]**:
  * 1440p, 4K, 16:10(Steam Deck), 21:9(울트라와이드) 환경에서 발생하는 14MB~33MB PCIe 대역폭 폭탄을 차단하기 위해 D3D11 기반 풀스크린 트라이앵글 셰이더(VS/PS 바이트코드 내장) 및 Bilinear Sampler 구축.
  * 1080p 환경은 0ms Fast Path로 직행하고, 비-1080p 환경만 1회 단일 패스(Draw 3)로 16:9 영역을 $1920 \times 1080$ Render Target으로 정규화한 뒤 43개 아틀라스 슬롯으로 고속 복사하여 4K에서도 CPU 전송량을 단 1MB로 엄격 고정.

### 🔬 11. 360p/540p 다운샘플링 한계 탐색 및 결과창 글자 분할 버그 수정
* **[다운샘플링 실측 벤치마크 (`benchmark_lowres.rs`)]**:
  * 19개 실전 결과창 스크린샷 전수 벤치마크 결과, 540p(89.5%) 및 360p(63.2%) 극단적 다운샘플링 시 Rate 소수점(`.`) 증발로 인한 인식 실패 규명. 동일한 1MB 전송량을 유지하면서 100% 무손실 1:1 정확도를 보존하는 512×512 GPU ROI Atlas의 기술적 우월성 증명.
* **[결과창 Rate/Score 글자 분할 알고리즘 버그 수정 (`overmax_cv::image`)]**:
  * 숫자 '1'의 하단 받침대가 잘려 인식에 실패하던 글자 분할 임계치 및 세그멘테이션 로직을 수정하여 결과창 인식 정확도 100.0% 달성 (커밋 `8288bf1`).

### 🌐 12. Zero-Dependency 크로스 플랫폼 OS 언어 감지 및 로케일 격리 파이프라인
* **[Windows `GetUserDefaultUILanguage` FFI 연동 (`windows.rs`)]**:
  * 외부 크레이트 추가 없이 Win32 `kernel32::GetUserDefaultUILanguage()` C-API 바인딩을 통해 0-allocation(수 나노초)으로 사용자의 UI 표시 언어(`LANGID`) 감지.
  * Primary Language ID 비트마스킹(`langid & 0x03FF`)을 통해 `0x12`(Korean) ➔ `"ko"`, `0x11`(Japanese) ➔ `"ja"`, `0x09`(English) ➔ `"en"`, 그 외 언어 ➔ `"en"` 폴백.
* **[Linux POSIX 로케일 체인 분석 (`linux.rs`)]**:
  * POSIX/GNU gettext 표준 우선순위(`LANGUAGE` ➔ `LC_ALL` ➔ `LC_MESSAGES` ➔ `LANG`)를 순차 검사하고 콜론 구분자(`:`) 및 ISO 639-1 접두사를 파싱하는 `parse_posix_locale` 파이프라인 구축.
* **[i18n 로케일 해석 및 설정 정규화]**:
  * `resolve_locale` 함수를 통해 `"auto"`/미지정 시 OS 감지 언어로 매핑하고, `normalize_settings`에서 `"auto"` 상태 보존 및 유효하지 않은 언어 입력 시 `auto` 안전 폴백.
  * 비정형 한국어 데이터인 `PatternSheetMetaItem::note` 표출의 `Locale::Ko` 로케일 렌더링 가드 연동 (`overlay_header_detail.rs`).

---

## 🤝 기여자 (Contributors)

이번 v0.4.1 릴리즈에 기여해 주신 모든 분들께 진심으로 감사드립니다:

* **[@khanwul](https://github.com/khanwul)**: Linux(Wayland/KDE/Niri) 오버레이 표시 상태와 캡처 상태 전달 경로 분리 및 검증 환경 구축 ([#25](https://github.com/orphera/overmax/pull/25))