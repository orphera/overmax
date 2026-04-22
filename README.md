# Overmax

DJMAX RESPECT V 선곡 화면에서 V-Archive 기반 비공식 난이도 정보를 실시간으로 보여주는 오버레이 도구입니다.

---

## 사용자 안내

### 무엇을 해 주나요?

선곡 화면에서 현재 선택된 곡의 **V-Archive 비공식 난이도**와 **유사 난이도 추천 목록**을 게임 화면 옆에 띄워줍니다.

- 현재 선택 곡의 버튼 모드별 비공식 난이도 표시 (NM/HD/MX/SC)
- 플레이 기록이 있는 패턴은 달성률(Rate)과 함께 표시
- 현재 패턴과 유사한 난이도의 다른 패턴 추천 (Rate 낮은 순 → 미플레이 순)

메모리 읽기나 게임 파일 수정은 일절 없으며, **창 추적 + 화면 캡처** 방식으로만 동작합니다.

### 설치 방법

1. [Releases](https://github.com/orphera/overmax/releases) 에서 최신 버전의 `overmax.zip`을 다운로드합니다.
2. 압축을 풀고 `overmax.exe`를 실행합니다.
3. 실행 중 DJMAX RESPECT V를 실행하면 자동으로 인식이 시작됩니다.

> **image_index.db 없이 실행하면 곡 인식이 동작하지 않습니다.**  
> 첫 실행 시 자동으로 최신 DB를 다운로드합니다. 실패할 경우 [Overmax Image DB Releases](https://github.com/orphera/overmax-image-db/releases) 페이지에서 `image_index.db`를 받아 `cache/` 폴더에 넣어주세요.

### 요구사항

- Windows 10 1809 이상 (64bit) — Windows OCR 필수
- DJMAX RESPECT V (Steam, 한국어 또는 영어로 실행)
- 실행 중 인터넷 연결 (V-Archive 데이터 다운로드, image_index.db 업데이트)
- 실행 중 인터넷 연결 (V-Archive 데이터 다운로드, image_index.db 업데이트, 앱 자동패치 확인)

### 단축키

| 키 | 동작 |
|---|---|
| `F3` | 오버레이 표시/숨김 |

트레이 아이콘 더블클릭으로도 오버레이를 토글할 수 있습니다.  
트레이 우클릭 메뉴에서 디버그 창을 열 수 있습니다.

### 주의사항

- 인식은 선곡 화면에서만 동작합니다. 인게임 중에는 오버레이가 표시되지 않습니다.
- 화면 해상도가 1920×1080이 아닌 경우 Rate OCR 수집이 정확하지 않을 수 있습니다.
- 동일 인스턴스가 이미 실행 중이면 새 실행은 즉시 종료됩니다.

---

## 현재 구현 상태

| 기능 | 상태 |
|---|---|
| 선곡화면 감지 | `FREESTYLE` 로고 OCR + 히스토리/히스테리시스 |
| 곡 감지 | 재킷 이미지 매칭 (perceptual hash + HOG + ORB) |
| 버튼 모드 감지 | 픽셀 색상 분류 (4B/5B/6B/8B) |
| 난이도 감지 | 패널 밝기 비교 (NM/HD/MX/SC) |
| Rate 자동 수집 | Windows OCR → RecordDB (SQLite) 자동 저장 |
| 유사 난이도 추천 | floor 기준 ±0 범위, rate 포함 정렬 |
| image_index.db 자동 업데이트 | GitHub Releases 기반, 버전 비교 후 필요 시 다운로드 |
| 앱 자동패치 | GitHub Releases 최신 버전 감지 시 `overmax.zip` 다운로드 후 재시작 교체 |
| 해상도 독립 좌표 | ROI Manager — Letterbox/Pillarbox 자동 보정 |
| 단일 인스턴스 | Windows named mutex |
| 오버레이 위치 저장 | `cache/overlay_position.json` |
| Steam ID 감지 | `loginusers.vdf` 파싱, 창 발견 시 자동 갱신 |
| 디버그 창 | 모듈별 색상 로그, ROI 표시 토글 |

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

디버그 빌드 (콘솔 창 표시):

```bat
build.bat --debug
```

빌드 결과물:
- 실행 폴더: `dist\overmax\`
- 릴리즈 패키지: `dist\overmax.zip`
- 자동패치 매니페스트: `dist\release_manifest.json`

### image_index.db 구축

```bash
python -m detection.image_db
```

대화형 CLI에서:
- **폴더 일괄 추가**: 재킷 이미지 폴더 지정 → 파일명 stem을 `song_id`로 사용 (숫자 파일명만 허용)
- **단일 이미지 추가**: `song_id` + 파일 경로 직접 지정
- 같은 `song_id` 재등록 시 upsert(갱신)

구축된 `cache/image_index.db`를 릴리즈에 첨부합니다.

### 주요 설정

설정 파일: `settings.json`

| 섹션 | 키 | 설명 |
|---|---|---|
| `screen_capture` | `logo_*` | 선곡화면 로고 ROI |
| | `freestyle_on/off_ratio` | 히스테리시스 on/off 임계값 |
| | `freestyle_on/off_min_samples` | 판정 최소 샘플 수 |
| `jacket_matcher` | `similarity_threshold` | 재킷 매칭 최소 유사도 |
| `app_update` | `enabled / owner / repo / asset_name` | 앱 자동패치 대상 릴리즈 설정 |
| `mode_diff_detector` | `history_size` | 모드/난이도 다수결 샘플 수 |
| `varchive` | `fuzzy_threshold` | 퍼지 곡명 매칭 최소 점수 |

---

## 남은 개발 과제

- **설정창 만들기**
- **오버레이 불투명도 조절**
- **V-Archive 기록 연동**
- **Rate OCR 좌표 비율 추가 지원** — 현재 16:9 비율만 지원
- **DLC 필터링** — 추천 목록에서 미보유 DLC 제외
- **빌드 결과물 크기 축소**

---

## 프로젝트 구조

```
overmax/
├── main.py                      # 진입점, 컴포넌트 조립
├── settings.py                  # 설정 로더 (기본값 + settings.json 병합)
├── settings.json                # 사용자 설정
├── constants.py                 # 중앙 상수 관리
├── runtime_patch.py             # PyInstaller 환경 경로 패치
│
├── capture/
│   ├── window_tracker.py        # 게임 창 위치/크기 추적
│   ├── screen_capture.py        # 캡처, OCR, 인식 파이프라인
│   ├── roi_manager.py           # ROI 좌표 관리 및 해상도 변환
│   └── helpers.py               # 캡처 파이프라인 공용 함수
│
├── core/
│   ├── game_state.py            # GameSessionState (공유 상태 모델)
│   └── global_hotkey.py         # Windows 전역 단축키 (RegisterHotKey)
│
├── data/
│   ├── varchive.py              # V-Archive 데이터 로드/검색
│   ├── record_db.py             # 플레이 기록 로컬 캐시 (SQLite)
│   ├── recommend.py             # 유사 난이도 추천
│   ├── image_db_updater.py      # GitHub Releases 기반 DB 자동 업데이트
│   ├── app_updater.py           # GitHub Releases 기반 앱 자동패치
│   └── steam_session.py         # Steam ID 조회
│
├── detection/
│   ├── image_db.py              # 재킷 이미지 특징 DB (pHash/HOG/ORB)
│   ├── image_db_cli.py          # ImageDB 관리 CLI
│   └── mode_diff.py             # 버튼 모드 / 난이도 감지
│
├── overlay/
│   ├── controller.py            # 오버레이 컨트롤러 (이벤트 → Qt 시그널)
│   ├── window.py                # PyQt6 오버레이 메인 창
│   ├── debug_window.py          # 디버그 로그 창
│   └── ui/
│       ├── pattern_view.py      # 난이도 탭 위젯 (DiffTab, VerticalTabPanel)
│       ├── recommend_view.py    # 추천 패턴 행 위젯 (PatternRow)
│       └── navigation.py        # ROI 디버그 오버레이 (RoiOverlayWindow)
│
├── test/
│   └── overlay_tester.py        # 게임 영상 재생 테스트 도구
│
├── overmax.spec                 # PyInstaller 스펙
├── build.bat                    # 빌드 스크립트
├── version_info.txt             # EXE 버전 정보
├── CONTEXT.md                   # 개발자 컨텍스트 메모
└── AGENTS.md                    # 에이전트 행동 원칙
```

---

## 데이터 출처

- [V-Archive](https://v-archive.net)

---

## 라이선스

MIT
