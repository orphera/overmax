# DJMAX Respect V 난이도 오버레이

V-Archive의 비공식 난이도 데이터를 게임 선곡화면 위에 실시간으로 표시합니다.

## 설치

```bash
# 1. Python 3.10 이상 필요
python --version

# 2. 의존성 설치
pip install -r requirements.txt

# 3. songs.json 준비 (둘 중 하나)
#   A) cache/ 폴더에 songs.json 직접 복사
mkdir cache
copy songs.json cache\songs.json

#   B) 첫 실행 시 자동 다운로드 (인터넷 필요)

# 4. 실행
python main.py
```

## 사용법

| 키 | 동작 |
|----|------|
| F9 | 오버레이 표시/숨김 |
| 드래그 | 오버레이 위치 이동 |

- 게임 선곡화면 진입 시 자동으로 오버레이가 나타납니다
- 선곡화면을 벗어나면 자동으로 숨겨집니다

## 표시 정보

각 버튼 모드(4B/5B/6B/8B)별로:
- **공식 레벨** (큰 숫자)
- **비공식 난이도** (floorName, 예: 15.2) - V-Archive에 등록된 경우만 표시
- 난이도 미등록 패턴은 `-` 표시

## 구조

```
djmax-overlay/
├── main.py           # 진입점
├── varchive.py       # V-Archive DB 관리 + 검색
├── window_tracker.py # 게임 창 위치 추적
├── screen_capture.py # 화면 캡처 + 선곡화면 감지 + OCR
├── overlay.py        # PyQt6 투명 오버레이 UI
├── requirements.txt
└── cache/
    └── songs.json    # V-Archive 곡 데이터 캐시
```

## 주의사항

- **사인코드(안티치트) 안전**: 게임 메모리를 읽거나 수정하지 않습니다
- 화면 캡처 + OCR 방식으로만 동작합니다
- OCR 인식률은 게임 해상도/폰트에 따라 달라질 수 있습니다
- 창모드/전체화면 모두 지원 (비율 기반 좌표 계산)

## 튜닝 포인트

`screen_capture.py` 상단의 상수를 조정해 인식률을 높일 수 있습니다:

```python
ANCHOR_BRIGHTNESS_THRESHOLD = 150  # 선곡화면 감지 민감도
OCR_INTERVAL = 0.4                 # 인식 주기 (초), 낮출수록 빠르지만 CPU 사용 증가
```
