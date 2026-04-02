# CONTEXT.md

이 문서는 Overmax의 설계 결정, 미완성 항목, 향후 과제를 기록합니다.
코드베이스를 새로 보는 사람(또는 미래의 자신)이 빠르게 맥락을 파악할 수 있도록 작성되었습니다.

---

## 프로젝트 개요

DJMAX Respect V 선곡화면에서 V-Archive의 비공식 난이도(floor)를 실시간으로 보여주는 Windows 오버레이 도구입니다.

핵심 제약 조건은 **사인코드(SINCODE, 게임 자체 안티치트) 안전성**입니다. 게임 프로세스 메모리를 읽거나 DLL을 인젝션하지 않고, 순수하게 화면 캡처 + OCR로만 동작합니다.

---

## 아키텍처

```
main.py
 ├── WindowTracker       (스레드) 게임 창 위치/크기 폴링
 ├── ScreenCapture       (스레드) 화면 캡처 → 선곡화면 감지 → OCR
 │    └── → OverlayController.notify_song()  곡명 전달
 └── OverlayController   (메인 스레드) Qt 이벤트 루프
      └── OverlayWindow  PyQt6 투명 창 UI
```

스레드 간 통신은 PyQt6 시그널/슬롯으로만 합니다. `OverlaySignals`가 브릿지 역할을 하며, 백그라운드 스레드에서 emit하면 Qt가 메인 스레드에서 슬롯을 실행합니다. IPC 없이 같은 프로세스 안에서 처리되므로 오버헤드가 없습니다.

---

## 주요 설계 결정

### 오버레이 프레임워크: PyQt6

Electron도 검토했지만 리듬게임 환경 특성상 CPU 점유를 최소화해야 했습니다. PyQt6는 메모리 ~30MB, 정적일 때 CPU ~1% 수준입니다. EasyOCR/OpenCV와 같은 Python 생태계를 쓰므로 IPC 없이 단일 프로세스로 구성할 수 있다는 장점도 있습니다.

### 선곡화면 감지: 앵커 픽셀 + 하이라이트 색상

BGA(배경 영상)가 곡마다 바뀌므로 배경 픽셀은 기준으로 쓸 수 없습니다. 대신 두 단계로 판별합니다.

1. **선곡화면 진입 여부**: 하단 힌트바(`Esc 메뉴` 근처) 영역의 평균 밝기로 판별합니다. 힌트바는 BGA와 무관하게 항상 동일한 위치에 흰색 텍스트로 렌더링됩니다.
2. **현재 선택 곡**: 오른쪽 리스트에서 주황-보라 그라데이션 하이라이트 행을 HSV 색상 마스크로 찾고, 해당 행만 OCR합니다.

이 방식의 한계는 게임 UI 업데이트로 힌트바 위치나 하이라이트 색상이 바뀌면 재튜닝이 필요하다는 점입니다.

### 좌표계: 비율 기반

1920×1080 고정 좌표 대신 창 클라이언트 영역 크기에 대한 비율로 저장합니다. `WindowRect.region()` 메서드가 비율 → 절대 픽셀 변환을 담당합니다. 전체화면/창모드/다양한 해상도를 별도 처리 없이 지원합니다.

`GetWindowRect` 대신 `GetClientRect + ClientToScreen`을 사용해 타이틀바와 윈도우 테두리를 제외한 실제 게임 렌더링 영역만 추적합니다.

### V-Archive 데이터: 로컬 캐시 우선

`cache/songs.json`이 존재하고 24시간 이내이면 API를 호출하지 않습니다. 검색은 정확 매칭 → rapidfuzz 퍼지 매칭 순으로 시도하며, OCR 오인식(예: `Kamui` → `Kamul`)에 대응합니다. rapidfuzz가 없으면 stdlib의 `difflib`로 폴백합니다.

### 패키징: PyInstaller onedir

`--onefile`이 아닌 `--onedir`을 선택했습니다. onefile은 실행 시 임시 폴더에 압축 해제하는 과정이 있어 첫 실행이 수초 걸리는 반면, onedir은 즉시 실행됩니다. 리듬게임 특성상 게임 실행 중에 오버레이를 빠르게 켜야 하므로 onedir이 적합합니다.

EasyOCR 모델(수백 MB)은 EXE에 포함하지 않고 `{exe위치}/models/`에 첫 실행 시 다운로드합니다. `runtime_patch.py`가 `EASYOCR_MODULE_PATH` 환경변수를 설정해 경로를 고정합니다.

---

## 미완성 / 검증 필요 항목

### 실제 게임에서 검증 필요한 좌표

`screen_capture.py`의 모든 비율 상수는 인터넷에서 구한 스크린샷 한 장을 기준으로 추정한 값입니다. 실제 게임을 실행해서 검증하고 보정해야 합니다.

```python
# 이 값들은 추정치 - 실측 필요
ANCHOR_REGION = (0.88, 0.972, 0.99, 0.995)
ANCHOR_BRIGHTNESS_THRESHOLD = 150
LIST_REGION = (0.188, 0.08, 0.595, 0.955)
HIGHLIGHT_HUE_RANGES = [(15, 35), (130, 150)]
TITLE_X_RATIO = (0.215, 0.475)
```

검증 방법: `screen_capture.py`를 단독으로 실행하면 감지 루프가 돌면서 콘솔에 결과를 출력합니다. OpenCV로 캡처 이미지를 저장해 시각적으로 확인하는 디버그 모드를 추가하면 좋습니다.

### OCR 정확도

EasyOCR의 한글+영어 혼합 인식 정확도는 폰트와 배경에 따라 크게 달라집니다. 특수문자가 많은 곡명(예: `I want You ~반짝 반짝 Sunshine~`)에서 오인식이 발생할 수 있습니다. rapidfuzz 퍼지 매칭으로 어느 정도 보완하지만, threshold 값(현재 80)은 실측 후 조정이 필요합니다.

Windows OCR API(`Windows.Media.Ocr`)가 EasyOCR보다 가벼우므로, 정확도가 충분하다면 교체를 고려할 수 있습니다.

### 버튼 모드 자동 감지

현재는 모든 버튼 모드(4B/5B/6B/8B)를 항상 표시합니다. 게임 화면에서 현재 선택된 버튼 모드를 인식해 해당 모드만 강조 표시하면 UX가 더 좋아집니다. 버튼 모드 선택 UI도 고정 위치에 있으므로 픽셀 색상으로 감지 가능합니다.

---

## 향후 과제 (TODO)

**기능**
- [ ] 버튼 모드 자동 감지 및 강조 표시
- [ ] 오버레이 위치/크기 설정 저장 (JSON 설정 파일)
- [ ] V-Archive 개인 클리어 데이터 연동 (로그인 필요)
- [ ] 트레이 아이콘 (백그라운드 상주, 우클릭 메뉴)
- [ ] 디버그 모드: 캡처 이미지 + 마스크를 별도 창에 표시

**안정성**
- [ ] 게임 해상도 변경 시 좌표 재계산 트리거
- [ ] songs.json 파싱 실패 시 UI 에러 표시
- [ ] OCR 연속 실패 시 재시도 로직

**빌드 / 배포**
- [ ] GitHub Actions CI (자동 빌드 + 릴리즈)
- [ ] UPX 설치 여부에 따른 조건부 압축
- [ ] 서명되지 않은 EXE에 대한 Windows Defender 경고 안내

---

## 빌드 관련 메모

### 빌드 결과물 크기

PyTorch를 포함하므로 `dist/overmax/` 폴더가 1.5~2GB 정도 나옵니다. zip 압축 시 600~800MB 수준입니다. 크기를 줄이려면 CPU 전용 torch(`torch+cpu` 인덱스)를 설치하거나, EasyOCR 대신 Windows OCR API로 교체하는 방법이 있습니다.

### 알려진 PyInstaller 이슈

- `torch`의 일부 C 확장이 UPX 압축과 충돌합니다. `overmax.spec`의 `upx_exclude`에 문제가 되는 DLL을 추가하세요.
- PyQt6와 OpenCV가 각자 Qt를 번들링하므로 DLL 충돌이 생길 수 있습니다. `cv2`의 Qt 플러그인을 `excludes`에 추가하는 것으로 해결 가능합니다.
- EasyOCR 모델 다운로드는 패키징 후에도 런타임에 발생합니다. 오프라인 배포가 필요하면 `models/` 폴더를 미리 채워서 동봉하세요.

### 디버그 빌드

콘솔 창이 보이는 빌드가 필요할 때:

```bat
build.bat --debug
```

또는 `overmax.spec`에서 `console=False`를 `console=True`로 바꾸고 직접 `pyinstaller overmax.spec`을 실행합니다.
