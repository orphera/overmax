# Overmax (Rust)

DJMAX RESPECT V 선곡 화면에서 V-Archive 기반 비공식 난이도 정보를 실시간으로 보여주는 **네이티브 Rust** 오버레이 도구입니다.

---

## 사용자 안내

### 무엇을 해 주나요?

선곡 화면에서 현재 선택된 곡의 **V-Archive 비공식 난이도**와 **유사 난이도 추천 목록**을 게임 화면 옆에 띄워줍니다.

- 현재 선택 곡의 버튼 모드별 비공식 난이도 표시 (NM/HD/MX/SC)
- **V-Archive 기록 연동**: V-Archive의 플레이 기록을 불러오고, 로컬 수집 기록을 V-Archive에 등록 가능
- **실시간 Rate / Max Combo 수집**: 게임 내에서 기록을 갱신하면 자동으로 인식하여 로컬에 저장
- **유사 난이도 추천**: 현재 패턴과 유사한 난이도의 다른 패턴 추천 (Rate 낮은 순 → 미플레이 순)

메모리 읽기나 게임 파일 수정은 일절 없으며, **창 추적 + 화면 캡처** 방식으로만 동작합니다.

### 설치 방법

1. [Releases](https://github.com/orphera/overmax/releases) 에서 최신 버전의 `overmax.zip`을 다운로드합니다.
2. 압축을 풀고 `overmax.exe`를 실행합니다.
3. 실행 중 DJMAX RESPECT V를 실행하면 자동으로 인식이 시작됩니다.

> **자동 업데이트**: 앱 시작 시 자동으로 최신 버전 여부 및 곡 DB(`image_index.db`) 상태를 확인하여 업데이트를 수행합니다.

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

---

## 개발자 안내

### 빌드 및 실행

```bash
# Rust 설치 필요 (rustup)
cargo build --release -p overmax-app
./target/release/overmax.exe
```

### 프로젝트 구조 (Rust)

- `rust/overmax_app`: 메인 어플리케이션 (egui/winit 기반 UI 및 이벤트 루프)
- `rust/overmax_core`: 핵심 상태 모델 및 공통 로직
- `rust/overmax_data`: 설정, DB(SQLite), V-Archive API 연동
- `rust/overmax_cv`: 이미지 처리 핵심 알고리즘 (HOG, OCR 전처리 등)

### 빌드 스크립트

- `build.bat`: 전체 빌드 및 패키징 자동화
- `scripts/package-rust.ps1`: 배포용 zip 파일 생성 스크립트

---

## 데이터 출처

- [V-Archive](https://v-archive.net)

---

## 라이선스

MIT
