# Current Goal

- DJMAX RESPECT V 선곡 화면에서  
  V-Archive 기반 난이도 및 추천 정보를 실시간 오버레이로 제공

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
→ ScreenCapture
→ Detection Pipeline (ImageDB + mode_diff)
→ GameSessionState (verified)
→ OverlayController
→ OverlayWindow (PyQt6)
       └── VerticalTabPanel (난이도 탭)
       └── PatternRow × N  (추천 목록)
```

패키지 구조: `capture/`, `core/`, `data/`, `detection/`, `overlay/`

---

# Detection Pipeline

## Primary Signals

- **선곡화면 감지**: FREESTYLE 로고 OCR + 히스토리/히스테리시스
- **곡 인식**: 재킷 이미지 매칭 (ImageDB — perceptual hash + HOG + ORB, 가중치 0.45/0.35/0.20)
- **버튼 모드**: BTN_MODE_ROI 평균색 vs 대표색 거리 비교 (4B/5B/6B/8B), 임계값 60
- **난이도**: DIFF_PANEL_ROI 평균 밝기 비교, 상위 패널 vs 2위 margin ≥ 15.0

## Secondary Signals

- **OCR (Windows OCR)**: FREESTYLE 로고 검증 / Rate 수집
  - 3× upscale + Otsu binarization, force_invert 재시도 1회
  - OCR은 primary signal 불가 (small text, low contrast) — Rate 수집 및 로고 전용

## State Handling

- hysteresis 기반 선곡화면 판정 (on/off 비율 별도 임계값)
- 후반 히스토리 비율 하락 감지 시 `[이탈중]` skip
- `MODE_DIFF_HISTORY` 연속 동일 프레임 기반 안정화 (기본 3프레임)
- `GameSessionState.is_stable` = True일 때만 상태 commit

## ROI 좌표

`ROIManager` (`capture/roi_manager.py`) 가 1920×1080 기준 픽셀 좌표를 현재 창 크기로 변환한다.  
Letterbox/Pillarbox 자동 보정 포함.

**예외**: 16:9 비율 해상도에서만 검증 되었고, 16:10 비율 해상도를 추가로 지원 해야 함.

---

# Current State

- 전체 파이프라인 정상 동작
- verified 기반 상태 전이 안정적
- 추천 시스템 (floor 기반, ±0 범위) 구현 완료
- Rate OCR → RecordDB 자동 수집 구현 완료
- `image_db_updater.py`: GitHub Releases 기반 `image_index.db` 자동 업데이트 구현 완료
- 단일 인스턴스 보장 (Windows named mutex)
- 오버레이 위치 저장/복원 (`cache/overlay_position.json`)
- Steam ID 기반 사용자 식별 (로그인 세션 자동 감지)
- ROIManager 해상도 변환 완료 (Letterbox/Pillarbox 보정)

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

- `ImageDB.search()`: 전체 row 순회 (인메모리 캐시 미도입)
- OCR 호출 비용: 로고 + Rate 각 독립 호출

## 3. 사용자 식별

- Steam ID: `loginusers.vdf` 파싱, 멀티 계정 전환 시 갱신 타이밍 불안정

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

---

# Debug Strategy

- `DebugController`: 모듈별 색상 구분 로그, 필터/일시정지/지우기
- `RoiOverlayWindow`: 게임 화면 위 ROI 경계 실시간 표시 (디버그 창 ROI 토글 연동)
- 런타임 OCR 입력 품질 검증 필요 (스틸샷 vs 런타임 캡처 품질 차이)

---

# Next Focus

1. **ImageDB 인메모리 캐시 도입** — `search()` 전체 row 순회 제거
2. **빌드 결과물 크기 축소**
3. **버튼 모드 샘플 보강 및 임계값 튜닝**
4. **DLC 필터링** — 추천 목록에서 미보유 DLC 제외
