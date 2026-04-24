# Overmax

DJMAX RESPECT V 선곡 화면에서 V-Archive 기반 비공식 난이도 정보를 실시간으로 보여주는 오버레이 도구입니다.

---

## 사용자 안내

### 무엇을 해 주나요?

선곡 화면에서 현재 선택된 곡의 **V-Archive 비공식 난이도**와 **유사 난이도 추천 목록**을 게임 화면 옆에 띄워줍니다.

- 현재 선택 곡의 버튼 모드별 비공식 난이도 표시 (NM/HD/MX/SC)
- **V-Archive 기록 불러오기**: V-Archive의 플레이 기록을 연동하여 달성률(Rate)을 함께 표시 (현재 읽기 전용)
- **실시간 Rate 수집**: 게임 내에서 기록을 갱신하면 자동으로 인식하여 로컬에 저장
- **유사 난이도 추천**: 현재 패턴과 유사한 난이도의 다른 패턴 추천 (Rate 낮은 순 → 미플레이 순)

메모리 읽기나 게임 파일 수정은 일절 없으며, **창 추적 + 화면 캡처** 방식으로만 동작합니다.

### 설치 방법

1. [Releases](https://github.com/orphera/overmax/releases) 에서 최신 버전의 `overmax.zip`을 다운로드합니다.
2. 압축을 풀고 `overmax.exe`를 실행합니다.
3. 실행 중 DJMAX RESPECT V를 실행하면 자동으로 인식이 시작됩니다.

> **자동 업데이트**: 앱 시작 시 자동으로 최신 버전 여부 및 곡 DB(`image_index.db`) 상태를 확인하여 업데이트를 수행합니다. 수동 업데이트가 필요한 경우 [Overmax Image DB Releases](https://github.com/orphera/overmax-image-db/releases) 페이지를 참조하세요.

### 요구사항

- Windows 10 1809 이상 (64bit) — Windows OCR 필수
- DJMAX RESPECT V (Steam, 한국어 또는 영어로 실행)
- 실행 중 인터넷 연결 (V-Archive 데이터 다운로드, 앱 및 DB 자동 업데이트 확인)

### 단축키 및 설정

| 키 | 동작 |
|---|---|
| `F3` | 오버레이 표시/숨김 |

- 오버레이 헤더의 **톱니바퀴 버튼(⚙)**을 누르면 설정 창이 열립니다.
- 트레이 아이콘 더블클릭으로도 오버레이를 토글할 수 있습니다.
- 설정 창에서 **오버레이 크기(75% ~ 150%)**와 **투명도**를 조절할 수 있습니다.
- 오버레이는 마우스 드래그로 원하는 위치에 옮길 수 있으며, 위치는 자동으로 저장됩니다.

### 주의사항

- 인식은 선곡 화면에서만 동작합니다. 인게임 중에는 오버레이가 표시되지 않습니다.
- **인식 신뢰도**에 따라 오버레이가 투명해지거나 다시 진해질 수 있습니다.
- 화면 해상도가 1920×1080이 아닌 경우 Rate OCR 수집이 정확하지 않을 수 있습니다.
- 동일 인스턴스가 이미 실행 중이면 새 실행은 즉시 종료됩니다.

---

## 현재 구현 상태

| 기능 | 상태 |
|---|---|
| 선곡화면 감지 | `FREESTYLE` 로고 OCR + 히스토리 기반 신뢰도(Confidence) 산출 |
| 곡 감지 | 재킷 이미지 매칭 (perceptual hash + HOG + ORB) |
| 버튼 모드 감지 | 픽셀 색상 분류 (4B/5B/6B/8B) |
| 난이도 감지 | 패널 밝기 비교 (NM/HD/MX/SC) |
| Rate 자동 수집 | Windows OCR → RecordDB (SQLite) 자동 저장 (중복 수집 방지) |
| V-Archive 연동 | API 기반 유저 기록 불러오기 (Read-only, 설정에서 ID 입력 필요) |
| 유사 난이도 추천 | floor 기준 ±0 범위, rate 포함 정렬 |
| 오버레이 스케일링 | 0.75x, 1.0x, 1.25x, 1.5x 프리셋 지원 |
| 신뢰도 기반 투명도 | 인식 상태에 따라 오버레이 투명도 실시간 조절 |
| 앱 및 DB 업데이트 | 앱 실행 시 최신 릴리즈 자동 감지 및 패치 |
| 해상도 독립 좌표 | ROI Manager — Letterbox/Pillarbox 자동 보정 |
| 설정 시스템 | `settings.user.json`에 변경분만 저장 (Minimal Delta Save) |
| Steam ID 감지 | `loginusers.vdf` 파싱 (재귀적 파싱 지원) |

---

## 개발자 안내

### 소스 실행

```bash
git clone https://github.com/orphera/overmax.git
cd overmax
pip install -r requirements.txt
python main.py
```

- `songs.json`은 없으면 V-Archive API에서 자동 다운로드됩니다 (`cache/` 저장).
- `image_index.db`가 없으면 GitHub Releases에서 자동 다운로드를 시도합니다.
- 앱 시작 시 `orphera/overmax` 최신 Release를 확인하고, 신규 버전이 있으면 자동 패치를 시도합니다.

### 설정 시스템

- `settings.json`: 배포 시 포함되는 기본 설정 파일입니다.
- `settings.user.json`: 사용자가 변경한 사항만 저장됩니다. `settings.json` 보다 우선순위가 높습니다.
- `settings.py` 내의 `_normalize_dict`에서 저장 전 값 검증 및 스냅(Snap) 처리를 수행합니다.

### 앱 자동패치 릴리즈 규칙

- Release asset에 최소 `overmax.zip`을 포함해야 합니다.
- 선택적으로 `release_manifest.json`을 포함하면 SHA256 검증을 수행합니다.
- `overmax.zip` 내부에는 실행 파일 `overmax.exe`가 포함되어야 합니다.
- 자동패치 설정은 `settings.json > app_update`에서 변경할 수 있습니다.

`release_manifest.json` 예시:

```json
{
  "assets": [
    {
      "name": "overmax.zip",
      "sha256": "REPLACE_WITH_SHA256"
    }
  ]
}
```

### 빌드

```bat
build.bat
```

- 전용 가상환경(`.venv_build`)을 생성하여 빌드 의존성을 분리합니다.
- Portable UPX를 지원하여 빌드 결과물 크기를 압축합니다.
- `overmax.spec`을 통해 `version_info.txt`의 버전 메타데이터를 EXE에 포함합니다.

---

## 남은 개발 과제

- **Max Combo, Perfect 수집** — Max Combo, Perfect 영역에 대한 이미지매칭 구현
- **V-Archive 갱신 후보 추출** — 후보 추출 하고 업데이트 하는 기능 구현
- **Rate OCR 좌표 비율 추가 지원** — 현재 16:9 비율만 지원
- **DLC 필터링** — 추천 목록에서 미보유 DLC 제외
- **버튼 모드/난이도 인식 보강** — 환경 변화에 강인한 알고리즘 도입
- **빌드 결과물 크기 축소**

---

## 프로젝트 구조

```
overmax/
├── main.py                      # 진입점, 컴포넌트 조립
├── settings.py                  # 설정 로더 및 검증 (delta save 지원)
├── settings.json                # 기본 배포용 설정
├── settings.user.json           # 사용자 개인 설정 (Git 제외)
├── constants.py                 # 중앙 상수 관리
├── runtime_patch.py             # PyInstaller 환경 경로 패치
│
├── capture/
│   ├── window_tracker.py        # 게임 창 위치/크기 추적
│   ├── screen_capture.py        # 캡처 및 인식 파이프라인 (Confidence 산출)
│   ├── roi_manager.py           # ROI 좌표 관리 및 해상도 변환
│   └── ocr_wrapper.py           # Windows OCR 인터페이스
│
├── core/
│   ├── game_state.py            # GameSessionState (공유 상태 모델)
│   └── app.py                   # OvermaxApp (메인 컨트롤러)
│
├── data/
│   ├── varchive.py              # V-Archive 데이터 및 기록 연동
│   ├── record_db.py             # 플레이 기록 로컬 캐시 (SQLite)
│   ├── recommend.py             # 유사 난이도 추천
│   ├── app_updater.py           # GitHub Releases 기반 앱 자동패치
│   └── steam_session.py         # Steam ID 및 세션 관리
│
├── detection/
│   ├── image_db.py              # 재킷 이미지 특징 DB (pHash/HOG/ORB)
│   └── mode_diff.py             # 버튼 모드 / 난이도 감지
│
├── overlay/
│   ├── controller.py            # 오버레이 컨트롤러
│   ├── window.py                # PyQt6 오버레이 메인 창 (스케일링 지원)
│   ├── settings_window.py       # 사용자 설정 UI
│   └── ui/
│       ├── pattern_view.py      # 난이도 탭 위젯
│       └── recommend_view.py    # 추천 패턴 행 위젯
│
├── overmax.spec                 # PyInstaller 스펙
├── build.bat                    # 빌드 스크립트 (격리 빌드 환경 지원)
└── version_info.txt             # EXE 버전 정보
```

---

## 데이터 출처

- [V-Archive](https://v-archive.net)

---

## 라이선스

MIT
