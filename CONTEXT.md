# Context: Overmax Development

이 문서는 Overmax 프로젝트의 현재 상태, 설계 결정 사항, 그리고 향후 계획을 기록한다.

---

# System Overview

Overmax는 DJMAX RESPECT V의 화면을 실시간으로 분석하여, 현재 선택된 곡의 난이도별 정보를 오버레이로 보여주는 도구이다.

- **인식 방식**: 화면 캡처 및 이미지 매칭 (OpenCV) + OCR (Windows OCR)
- **UI**: PyQt6 (투명 윈도우, 하드웨어 가속 활용)
- **데이터**: V-Archive DB (JSON) 및 로컬 기록 DB (SQLite)

---

# Core Constraints

- 메모리 접근 / 프로세스 인젝션 금지
- 화면 캡처 기반 처리 유지
- Python 환경 유지
- 인게임 성능 영향 최소화 (최우선)

---

# Current Architecture

```
WindowTracker
→ ScreenCapture (HysteresisBuffer + OcrDetector)
→ Detection Pipeline (ImageDB + PlayStateDetector)
→ GameSessionState (verified)
→ OverlayController
→ OverlayWindow (PyQt6)
       └── HeaderWidget / FooterWidget
       └── PatternView (난이도 탭)
       └── RecommendView × N (추천 목록)
       └── SettingsWindow (설정)
       └── SyncWindow (V-Archive 동기화)
       └── DebugWindow (디버그 로그)
```

패키지 구조: `capture/`, `core/`, `data/`, `detection/`, `overlay/`

---

# Detection Pipeline

## Primary Signals

- **선곡화면 감지**: FREESTYLE 로고 OCR + 히스토리/히스테리시스
- **곡 인식**: 재킷 이미지 매칭 (ImageDB — perceptual hash + HOG + ORB, 가중치 0.45/0.35/0.20)
- **PlayStateDetector** (`detection/play_state.py`): 모드/난이도/Max Combo/Rate를 원자 단위로 통합 감지
  - **버튼 모드**: BTN_MODE_ROI 평균색 vs 대표색 거리 비교 (4B/5B/6B/8B), 임계값 60
  - **난이도**: DIFF_PANEL_ROI 평균 밝기 비교, 상위 패널 vs 2위 margin ≥ 15.0
  - **Max Combo**: 뱃지 영역 평균 밝기 ≥ 160

## Secondary Signals

- **OCR (Windows OCR)**: `detection/ocr.py` — FREESTYLE 로고 검증 / Rate 수집
  - 3× upscale + Otsu binarization, force_invert 재시도 1회
  - OCR은 primary signal 불가 (small text, low contrast) — Rate 수집 및 로고 전용
  - PlayStateDetector 내부에서 상태 안정화 시 1회 Rate OCR 수행

## State Handling

- `HysteresisBuffer` (`capture/hysteresis.py`): 선곡화면 진입/이탈 판정 (on/off 비율 별도 임계값)
- **Confidence Score**: 히스테리시스 버퍼의 hit 비율을 기반으로 산출 (0.0~1.0)
- 후반 히스토리 비율 하락 감지 시 `[이탈중]` skip (Confidence 0.5x 보정)
- `PlayStateDetector` 연속 동일 프레임 기반 안정화 (기본 3프레임)
- `GameSessionState.is_stable` = True일 때만 상태 commit

## ROI 좌표

`ROIManager` (`capture/roi_manager.py`) 가 1920×1080 기준 픽셀 좌표를 현재 창 크기로 변환한다.  
Letterbox/Pillarbox 자동 보정 포함.

**예외**: 16:9 비율 해상도에서만 검증 되었고, 16:10 비율 해상도를 추가로 지원 해야 함.

---

# Progress Tracking

- 기본적인 곡 인식 및 오버레이 표시 구현 완료
- V-Archive 데이터 동기화 및 추천 시스템 구현 완료
- `ROIManager`를 통한 해상도 독립적 좌표 관리 구현 완료
- `image_db_updater.py`: GitHub Releases 기반 `image_index.db` 자동 업데이트 구현 완료
- `app_updater.py`: GitHub Releases 기반 앱 자동패치 구현 완료
- 단일 인스턴스 보장 (Windows named mutex)
- 오버레이 위치 저장/복원 (`settings.user.json` 내 `overlay.position`)
- Steam ID 기반 사용자 식별 (로그인 세션 자동 감지)
- ROIManager 해상도 변환 완료 (Letterbox/Pillarbox 보정)
- 설정창 추가 완료 (투명도 조절 및 설정 파일 분리 적용)
- **V-Archive 기록 연동**: API 기반 데이터 수집 및 로컬 DB 병합 완료 + 갱신 후보 추출 및 등록 기능 (`SyncWindow`)
- **Max Combo 감지**: 뱃지 영역 밝기 기반 감지 구현 완료
- **오버레이 스케일링**: 0.75x ~ 1.5x 프리셋 지원 및 스냅 기능 구현 완료
- **신뢰도 기반 투명도**: 인식 상태에 따라 오버레이가 부드럽게 페이드인/아웃됨
- **설정 시스템 최적화**: `settings.user.json`에 변경된 항목만 저장(delta save) 및 값 검증(clamp/snap)
- **자동 업데이트 시스템**: 앱(GitHub Releases) 및 이미지 DB(`image_index.db`) 자동 갱신 파이프라인 구축 완료
- **Detection 리팩토링**: `mode_diff.py` → `PlayStateDetector` (`play_state.py`)로 모드/난이도/Max Combo/Rate 통합
- **OCR 모듈 분리**: `capture/ocr_wrapper.py` → `detection/ocr.py` + `detection/ocr_wrapper.py`
- **오버레이 분해**: 헤더/푸터/네비게이션 위젯 분리, `DebugWindow`·`SyncWindow` 독립 모듈화
- **Core 유틸 분리**: `global_hotkey.py`, `utils.py`, `version.py` 추출

---

# Problems

## 1. 인식 정확도

- **버튼 모드**:
  - 대표색 고정 → 환경/감마 변화에 취약
  - 거리 임계값(60) 튜닝 필요, 샘플 색상 보강 필요

- **난이도**:
  - 밝기 기반이라 UI 전환 중 오인식 가능
  - margin 임계값(15.0) 환경에 따라 불안정

## 2. 성능 리스크

- OCR 호출 비용: 로고 + Rate 각 독립 호출 (Windows OCR 연산 부하 모니터링 필요)
- Rate OCR 시도 횟수 제한 (현재 최대 3회)으로 무한 재시도 방지

## 3. 사용자 식별

- Steam ID: `loginusers.vdf` 파싱, 멀티 계정 전환 시 갱신 타이밍 불안정
- V-Archive ID: 사용자가 직접 입력해야 하며, Steam ID와의 매핑 로직 필요

---

# Failed Approaches

## OCR Hybrid (버튼 모드/난이도 검증)

- 목표: 픽셀 기반 감지 보조
- 결과: 런타임에서 인식 불안정 (빈번한 실패)
- 원인: 작은 텍스트, low contrast, anti-aliasing, 캡처 품질 차이
- 결론: primary signal 불가 → verifier/fallback 용도도 제한
- 재검토 조건: ROI 정규화, 멀티프레임 처리 도입 후

---

# Important Invariants

- `is_stable = True`일 때만 상태 commit
- detection → verification → commit 흐름 유지
- 단일 프레임 결과에 의존하지 않음
- 동일 `(song_id, mode, diff)` 조합 Rate 수집 중복 제한 (`_recorded_states`)
- `rate == 0.0`은 저장하지 않음 (미플레이로 간주)
- `if rate is None` 과 `if rate == 0.0` 은 의미가 다름 — 명시적 None 체크 필수
- `song_id` 가 0인 경우도 존재함. `song_id is None` 으로 검사 해야 함
- image_db 에 추가/삭제는 overlay 프로그램을 통해 하지 않음
- 모든 설정 변경은 `settings.user.json`에 저장하며, `settings.json` 보다 우선순위가 높음
- **설정값 검증**: 저장 시 `_normalize_dict`를 통해 유효 범위 내로 강제 조정

---

# Debug Strategy

- `DebugController`: 모듈별 색상 구분 로그, 필터/일시정지/지우기
- `RoiOverlayWindow`: 게임 화면 위 ROI 경계 실시간 표시 (디버그 창 ROI 토글 연동)
- 런타임 OCR 입력 품질 검증 필요 (스틸샷 vs 런타임 캡처 품질 차이)

---

# Next Focus

1. **Rate OCR 좌표 비율 추가 지원** — 현재 16:9 비율만 지원
1. **DLC 필터링** — 추천 목록에서 미보유 DLC 제외
1. **버튼 모드/난이도 인식 보강** — 단순 픽셀 거리/밝기 외에 보조 알고리즘 검토
1. **빌드 결과물 크기 축소** — 불필요한 패키지 제외 및 리소스 최적화
