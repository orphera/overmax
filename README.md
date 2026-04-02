# Overmax

> DJMAX Respect V 선곡화면에 V-Archive 비공식 난이도를 실시간으로 오버레이하는 도구

게임 메모리를 건드리지 않고 화면 캡처 + OCR 방식으로만 동작하므로 사인코드(안티치트)에 안전합니다.

---

## 스크린샷

*(추후 추가 예정)*

---

## 요구사항

- Windows 10 / 11 (64bit)
- DJMAX Respect V (Steam)
- Python 3.10 이상 *(소스 실행 시)*

---

## 설치 및 실행

### 방법 A — 릴리즈 EXE 사용 (권장)

1. [Releases](../../releases) 에서 최신 `overmax-vX.X.X.zip` 다운로드
2. 압축 해제 후 `overmax.exe` 실행
3. 첫 실행 시 EasyOCR 모델을 자동 다운로드합니다 (약 300MB, 인터넷 필요)

### 방법 B — 소스에서 실행

```bash
# 1. 저장소 클론
git clone https://github.com/yourname/overmax.git
cd overmax

# 2. 의존성 설치
pip install -r requirements.txt

# 3. 실행
python main.py
```

> songs.json은 첫 실행 시 V-Archive API에서 자동으로 다운로드되어 `cache/` 폴더에 저장됩니다.
> 이후에는 캐시를 사용하며 24시간마다 갱신합니다.

### 방법 C — 직접 빌드

```bat
build.bat
```

빌드 결과물은 `dist\overmax\` 폴더에 생성됩니다. 자세한 내용은 [CONTEXT.md](CONTEXT.md)를 참고하세요.

---

## 사용법

게임을 실행하면 Overmax가 자동으로 게임 창을 감지합니다. 선곡화면에 진입하면 오버레이가 나타나고, 벗어나면 자동으로 숨겨집니다.

| 키 / 동작 | 기능 |
|-----------|------|
| `F9` | 오버레이 표시 / 숨김 토글 |
| 드래그 | 오버레이 위치 이동 |

### 표시 정보

선택된 곡의 버튼 모드(4B / 5B / 6B / 8B)별로 각 난이도 카드에 표시됩니다.

- **큰 숫자** — 공식 레벨
- **작은 숫자 (예: `9.1`)** — V-Archive 비공식 난이도 (floor). 미등록 패턴은 `-`

---

## 프로젝트 구조

```
overmax/
├── main.py             진입점, 컴포넌트 조립
├── varchive.py         V-Archive DB 로드 · 캐싱 · 검색
├── window_tracker.py   게임 창 위치 / 크기 추적 (pywin32)
├── screen_capture.py   화면 캡처 · 선곡화면 감지 · OCR
├── overlay.py          PyQt6 투명 오버레이 UI
├── runtime_patch.py    PyInstaller 패키징 환경 경로 패치
├── hook-easyocr.py     PyInstaller EasyOCR 커스텀 훅
├── overmax.spec        PyInstaller 스펙
├── build.bat           빌드 스크립트
├── version_info.txt    Windows EXE 버전 정보
├── requirements.txt
└── cache/              V-Archive 데이터 캐시 (git 제외)
```

설계 결정과 향후 과제에 대한 자세한 내용은 [CONTEXT.md](CONTEXT.md)를 참고하세요.

---

## 인식률 튜닝

`screen_capture.py` 상단의 상수로 동작을 조정할 수 있습니다.

```python
ANCHOR_BRIGHTNESS_THRESHOLD = 150  # 선곡화면 감지 민감도 (낮추면 더 민감)
OCR_INTERVAL = 0.4                 # 인식 주기(초). 낮출수록 빠르지만 CPU 사용 증가
```

---

## 데이터 출처

곡 정보 및 비공식 난이도 데이터는 [V-Archive](https://v-archive.net)에서 제공합니다.

---

## 라이선스

MIT
