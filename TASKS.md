# TASKS

Overmax 활성 작업 목록 및 마일스톤 로드맵입니다.  
(이전 완료 작업 목록은 [`docs/archive/tasks/TASKS_v0.4.0_archive.md`](docs/archive/tasks/TASKS_v0.4.0_archive.md)를 참조)

---

# 🚀 [Active] Milestone v0.4.1 — 외부 연동, 배포 채널 확장 & 캡처 파이프라인 혁신

## 1. 외부 연동 및 IPC 프로토콜 고도화 (IPC & Extensibility Protocol)

- [x] **1.1 내부 ➔ 외부 이벤트 스트리밍 (Event Stream Broadcast)**
  - [x] 씬 전환, 곡 변경, 플레이 상태 및 결과 확정 이벤트의 실시간 IPC 브로드캐스트 (SSE 단일 포트 트랜스포트)
- [x] **1.2 외부 ➔ 내부 호출 인터페이스 (Inbound RPC / MCP 지원)**
  - [x] 외부 도구 및 AI 에이전트 연동을 위한 호출 프로토콜 설계 (JSON-RPC 2.0 및 MCP 연동 기반)
  - [x] 현재 곡 정보 조회, 추천 목록 요청, 오버레이 상태 제어 RPC 엔드포인트 정의
- [x] **1.3 Recommend-Provider 프로토콜 통합 및 정리**
  - [x] 기존 Provider Fetch 규격(`recommend-provider-protocol.md`)을 신규 IPC/RPC 아키텍처와 일관되게 단일화 및 정리

---

## 2. 데이터 저장소 및 런타임 환경 분기 (Storage & Runtime Environment)

- [x] **2.1 데이터 경로 추상화 레이어 구축**
  - [x] Portable 모드(바이너리 상대 경로) 및 Installed/MSIX 모드(`%LOCALAPPDATA%\Overmax\`) 듀얼 경로 지원
  - [x] 사용자 설정(`settings.user.json`), 플레이 기록 DB(`cache/record.db`), 곡 메타 DB(`cache/songs.json`), 자켓 인덱스(`cache/image_index.db`)의 안전한 로드/마이그레이션 지원
- [x] **2.2 스토어 환경 자가 업데이터 분기 처리**
  - [x] MSIX 패키지 런타임 환경 감지(Win32 `GetCurrentPackageFullName` 또는 `store` feature flag)
  - [x] 스토어 패키지 실행 시 인앱 자가 업데이터(GitHub Releases 바이너리 교체) 비활성화 및 UI 처리

---

## 3. MSIX 패키징 및 배포 파이프라인 (MSIX Packaging & Store CI)

- [x] **3.1 Desktop Bridge 매니페스트 및 에셋 구성**
  - [x] `AppxManifest.xml` 작성 (`runFullTrust` 권한, 패키지 Identity, 타일/로고 44x44/150x150 에셋 매핑)
  - [x] MSIX 빌드/패키징 스크립트(`scripts/package-msix.ps1` 또는 `cargo-msix` / `MakeAppx` 연동) 구축 및 로컬 사이드로딩 검증
- [x] **3.2 Microsoft Store 등록 및 CI 자동화**
  - [x] Microsoft Partner Center 앱 등록 및 정책 대응 (서드파티 유틸리티 Disclaimer, 영/한 앱 설명, 스토어 스크린샷)
  - [x] GitHub Actions 릴리즈 워크플로우에 MSIX 패키지 생성 및 Store 업로드/검수 파이프라인 연동

---

## 4. 캡처 파이프라인 초고속화 (GPU ROI Atlas & Adaptive Normalization)

- [x] **4.1 컴파일 타임 아틀라스 레이아웃 & 트랜슬레이터 베이킹 (Step 1)**
  - [x] 43개 ROI(240,098 px)의 $512 \times 512$ 아틀라스 상수 배열(`pub const ATLAS_SLOTS: [AtlasSlot; 43]`) 베이킹 (`atlas_layout.rs`)
  - [x] `const fn get_roi_for_scene` 기반 $O(1)$ 정적 점프 테이블 트랜슬레이터 구현 (`atlas_translator.rs`)
  - [x] 기하학적 완전성 단위 테스트: 512×512 경계 검사 및 43개 슬롯 간 상호 AABB Overlap 0건 전수 검증
- [x] **4.2 오프라인 가상 아틀라스 CPU 무손실 검증 (Step 2)**
  - [x] 1080p 프레임을 $512 \times 512$ 가상 아틀라스로 조립하는 CPU 테스트 하네스 작성
  - [x] 가상 아틀라스 경유 디텍션 결과가 기존 파이프라인과 100% 일치함을 증명 (117개 테스트셋 및 `verify_pipeline`)
- [x] **4.3 DXGI 1080p 순수 1:1 하드웨어 아틀라스 캡처 연동 (Step 3)**
  - [x] 1080p 16:9 패스트패스: Draw Call 제로, 백버퍼 ➔ $512 \times 512$ VRAM Staging `CopySubresourceRegion` 43회 직행 (< 50 µs)
  - [x] 단 1회 1MB `Map(D3D11_MAP_READ)` 전송 및 카테고리 띠(64x60) / 마진(22x96) 슬롯 확장 지원
  - [x] `settings.json` 안전 가드 플래그(`enable_gpu_atlas`) 및 예외 시 즉시 레거시 전체화면 캡처로 폴백 연동
  - [x] `RoiManager` 자동 아틀라스 어댑터 및 `DetectionPipeline` 즉시 연동 완료
- [x] **4.4 비-1080p 해상도 지원을 위한 조건부 GPU Normalizer 구현 (Step 4)**
  - [x] 1080p가 아닐 때만 선별 동작하는 조건부 Draw Quad 렌더타겟($1920 \times 1080$) 파이프라인 구축
  - [x] 16:10(Steam Deck), 21:9(울트라와이드), 1440p/4K UV Crop 및 하드웨어 Bilinear 리샘플링
  - [x] 4K 환경에서 33MB 전송 폭탄 방지 및 아틀라스 무손실 전달 검증
- [x] **4.5 자켓 64×64 GPU 프리리사이즈 & 360p/540p 초저해상도 한계 탐색 (Step 5)**
  - [x] 자켓 매칭 프로파일링: CPU 리사이즈는 1.63µs에 불과함을 실측 입증하고, 이미지 인덱스 DB 호환성을 위해 1:1 무손실 유지 결정
  - [x] 360p/540p 다운샘플링 실측 벤치마크: Rate 인식률이 100%(1080p 아틀라스) -> 89.5%(540p) -> 63.2%(360p)로 붕괴(소수점 증발)됨을 규명하여 512x512 무손실 아틀라스의 우월성 최종 입증
- [x] **4.6 핑퐁 더블 버퍼링(Double-Buffered Staging) 및 실전 인게임 실측 검증 (Step 6)**
  - [x] 512×512 Staging 텍스처 2개 교대 핑퐁 및 GPU 비동기 `Flush()` 파이프라인 완성
  - [x] 4~5ms의 `context.Map` GPU 동기화 대기 시간(스톨)을 0ms로 소거
  - [x] 실전 벤치마크 실측 검증: DXGI 캡처 지연시간 4.50ms ➔ 0.62ms(P50: 0.63ms, P95: 0.80ms)로 **-86.2% 수직 단축(7.2배 고속화)** 및 결과창 인식률 100% 달성
  - [x] **3단계 A/B/C 실측 기여도 분리 검증**: [A] main(4.50ms) ➔ [B] fullframe-db(3.17ms, -1.33ms, 34.3% 기여) ➔ [C] atlas-db(0.62ms, -2.55ms 추가, 65.7% 기여)로 분리 입증
  - [x] **리뷰어 지적사항 정밀 팩트체크**: 결과창 인식 개선(12/19 ➔ 19/19)은 캡처 지연이 아닌 커밋 `8288bf1`(숫자 1 분할 수정)의 기여이며, 초기화 스파이크 분류 노이즈 규명 완료
- [x] **4.7 Windows HDR(scRGB) 무설정 자동 감지 & 64KB 고속 LUT 역변환 (Step 7)**
  - [x] Win32 CCD API(`DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL`) 및 DXGI 1.6 `IDXGIOutput6::GetDesc1().MaxLuminance` 기반 디스플레이 피크 휘도 무설정(Zero-Config) 1:1 자동 감지
  - [x] OS SDR 슬라이더 밝기(예: 240 nits = 3.0) 적용 시 게임 백버퍼 피크 휘도(scRGB ~5.11)와의 불일치로 인한 과노출(White Clipping) 근본 원인 규명 및 95.5% 실효 타깃 화이트(`MaxLuminance / 80.0 * 0.955`, ~4.88) 스케일 자동 결정으로 100% 해결
  - [x] 1440p 다운샘플링 블러 시 '8'의 가운데 허리선이 잘려 '0'으로 오인식되던 문제 및 100만 점/소수점 분리 실패를 `binarize_by_global_contrast` 72% 대비 이진화로 완전 해결
  - [x] $O(1)$ 64KB LUT 기반 scRGB FP16 ➔ sRGB BGRA8 0.38ms 고속 역변환 및 핫루프 락 프리 캐싱
  - [x] 비-1080p GPU Normalizer 및 멀티 모니터 전환 시 아틀라스 스테이징 리셋 안정화 완료
  - [x] DXGI 1.6 `IDXGIOutput6` 기반 실시간 모니터 색역(ColorSpace) 사전 감지 및 SDR/HDR 최적 포맷 동적 분기 완성
- [x] **4.8 HDR DCI-P3 광색역 역변환 및 오픈매치 거짓 양성 완전 방어 (Step 8)**
  - [x] DJMAX 엔진 내부 렌더링 색공간(DCI-P3 D65) 역변환 행렬($\mathbf{M}_{709 \to \text{P3}}$) 및 inv_scale 사전 곱셈(나눗셈 0회) 초고속 FMA 파이프라인 구현 (<0.1ms)
  - [x] scRGB 음수 채널 클리핑에 의한 초록/청록/주황/보라 컬러바 및 광색역 자켓 색상 파괴 100% 원천 해결
  - [x] 프리스타일 결과창이 오픈매치(`ResultOpen2`)로 오판정되던 버그 완전 박멸 (결과창 6건 전건 `ResultFreestyle` 100% 인식 달성)
- [x] **4.9 Auto HDR 2-Stage 점근 숄더 역톤매핑 및 자켓 매칭 100% 정상화 (Step 9)**
  - [x] Auto HDR 이중 스케일 딜레마(고휘도 폰트 4.88 vs 중간톤 자켓 4.00) 규명 및 $C^1$ 연속 Rational Spline 점근 숄더 수식 설계
  - [x] 39개 스냅샷 전수 실측 벤치마크: 자켓 평균 유사도 0.7564 ➔ **0.8700 (+15% 상승)** 및 0.65 미만 위험군 0건 완전 박멸
  - [x] Score 1,000,000점 및 Rate 100.00% 고휘도 폰트 블룸 억제 및 전수 인식 100% 보존
  - [x] 1,025-엔트리 Fast sRGB OETF 테이블(1KB L1 캐시 상주) 결합으로 픽셀 루프 내 `powf` 연산 소거 및 초고속 변환 달성
- [x] **4.10 자켓 매칭 동적 신뢰도 결합(Dynamic Confidence Fusion) 및 단색 배경 로고 인식 정상화 (Step 10)**
  - [x] 단색 배경 로고형 자켓(예: 메이플스토리2 'The Little Adventurer', SongID 439)의 하드 양자화(Hard Binning) 경계 결함(224 경계선 단층으로 L1 diff 폭발) 원인 규명
  - [x] pHash/dHash/aHash 3종 지문 초고신뢰도(Hamming 합계 11, 93.12% 일치) 상황에서 형태 지문 가중치(0.50 ➔ 0.75)를 동적 강화하는 Dynamic Confidence Fusion 설계 및 적용
  - [x] 히스토그램 L1 정규화 분모(3072.0 ➔ 4096.0) 현실화로 조명/배경 편차에 대한 안전 완충 지대 확보
  - [x] 신규 1440p 유저 캡처 자켓 유사도 0.4656 ➔ **0.7222** (+55% 상승) 및 씬 판정(Freestyle, 4B, NM) 100% 정상화
  - [x] 기존 39개 실측 HDR 스냅샷 벤치마크: 평균 유사도 0.8689 ➔ **0.9257**, 최저 유사도 0.7890 ➔ **0.8785**로 전체 정확도 대폭 향상
- [x] **4.11 Auto HDR 2-Anchor 일반화 파이프라인 (Step 11)**
  - [x] 패널 피크 휘도(MaxLuminance) 단일 비례식의 로컬 과적합(DisplayHDR 1000 등 고휘도 패널에서 자켓 언더노출 찌그러짐) 원인 규명
  - [x] Windows SDR 콘텐츠 백색(Anchor 1: Win32 CCD `detect_monitor_sdr_white_level`, 기본값 3.0)과 패널 피크 휘도(Anchor 2: DXGI 1.6 `MaxLuminance`, 기본값 4.88)를 분리하는 2-Anchor 모델 정립
  - [x] 중간톤 선형 복원 및 무릎점을 SDR 백색 기준으로 유도($\text{scale}_{\text{mid}} = A_{\text{sdr}} \times \frac{4.0}{3.0}$, $V_{\text{knee}} = A_{\text{sdr}} \times \frac{2.2}{3.0}$)하여 임의의 피크 패널에서도 자켓 밝기 및 형태 완벽 보존
  - [x] DXGI 캡처 엔진의 `resolve_sdr_white_level`을 Win32 CCD 우선 감지 구조로 전면 개편하고, 39개 스냅샷 100% 인식 유지 및 전체 유닛 테스트 무회귀 검증 완료
- [x] **4.12 1440p/Atlas 다운스케일링 폰트 인식 안정화 (ZNCC 소프트 매칭 & MaxRGB 이진화 & Cross-Validation) (Step 12)**
  - [x] L1 SAD 매칭의 획 면적 편향으로 인한 '8' ➔ '3' 오독을 원본 휘도 보존형 ZNCC 소프트 매칭(`match_character_soft`)으로 해결
  - [x] 글자 간 틈새 복원(`x1 -= 1`)의 인접 글자 경계 오염 제거로 1,000,000점('0' ➔ '8' 오독) 및 99.95%('5' ➔ '6' 오독) 정상화
  - [x] `LumaMethod::MaxRGB` 이진화 도입으로 비MaxCombo 등 빨간색(RED) 텍스트 1/3 감쇠(대비 68) 및 세그먼트 뭉침 결함 완벽 해결(대비 223 확보)
  - [x] Rate 영역 정밀 이진 템플릿 매칭(`.` 및 `%` 보존) 및 Score-Rate Cross-Validation을 통한 상호 검증 안전망 구축
  - [x] 선곡창/오픈매치 MAX COMBO 및 PERFECT 뱃지 임계치 20.0(`BADGE_MATCH_THRESHOLD`) 일원화
  - [x] 오픈매치 사용자 실전 캡처 5종(OM_1..5) 전수 검증 통과 및 전체 165개 단위/통합 테스트 무회귀 통과


---

## 5. 다국어 및 OS 언어 자동 감지 (i18n & Locale Automation)

- [x] **5.1 OS 기본 언어셋 자동 감지 파이프라인**
  - [x] Windows: Win32 API `GetUserDefaultUILanguage()` FFI 기반 0-allocation UI 언어 감지 (`windows.rs`)
  - [x] Linux: POSIX 표준 체인(`LANGUAGE` ➔ `LC_ALL` ➔ `LC_MESSAGES` ➔ `LANG`) 환경변수 파싱 (`linux.rs`)
  - [x] 설정 기본값(`"language": "auto"`) 및 `resolve_locale` 해석기 구축
- [x] **5.2 설정 UI 3버튼 슬림화 및 서열표 비고 로케일 격리**
  - [x] 불필요한 [자동] 버튼을 제거하고 감지된 언어가 바로 활성화된 직관적 3버튼(`[한국어 | English | 日本語]`) UX 구현
  - [x] 커뮤니티 서열표의 비정형 한국어 메모(`note`)는 `Locale::Ko`에서만 표출하도록 격리하여 글로벌 플레이어 시각 노이즈 소거

---

# 🎯 [Planned] Milestone v0.5.0 — 인게임 유틸리티 및 연동 고도화 (In-game Utilities & Automation)

## 6. 플레이어 편의성 및 인게임 유틸리티 (In-game Utilities & Controls)

- [ ] **6.1 글로벌/인게임 단축키(Hotkeys) 지원**
  - [ ] 오버레이 표시/숨김(Toggle Visibility), 라이트 모드 전환, 간편 V-Archive 업로드 단축키 연동
  - [ ] 게임 중 키 씹힘 방지 및 사용자 커스텀 단축키 설정 UI 제공
- [ ] **6.2 노트 레인 임시 가림막(Lane Blind / Curtain Overlay) 지원**
  - [ ] 연습용 상단/하단 가림막(SUDDEN / HIDDEN / BLIND 효과) 전용 경량 투명 서브 뷰포트 오버레이
  - [ ] 가림막 높이, 위치, 투명도 및 단축키 토글 제어 기능 지원

---

## 7. 기록 수집 및 V-Archive 자동 연동 (Record Automation)

- [ ] **7.1 결과창 V-Archive 자동 업로드 지원**
  - [ ] 결과창 씬 확정(`VerifiedPlayEvent`) 시 설정에 따라 백그라운드에서 V-Archive API 자동 업로드 트리거
  - [ ] 최고 기록(Personal Best) 갱신 시에만 선택적 자동 업로드 옵션 제공 및 네트워크 재시도 가드

---

## 8. 감지 씬 다양화 및 인게임 확장 (Scene Diversity & Ladder Match)

- [ ] **8.1 래더매치(Ladder Match) 씬 감지 대응**
  - [ ] 래더매치 밴픽/선곡 화면 및 대기실 감지 대응
  - [ ] 래더매치 결과창 인식 지원
