# Qt Runtime Plan

이 문서는 OpenCV 런타임 의존성 제거 이후, Overmax의 Qt/PyQt6 영역을
정리하기 위한 계획이다.

목표는 기존 Python 기반 verified pipeline과 화면 캡처 방식을 유지하면서,
Qt 의존성의 현재 한계를 기록하고 UI 코드 복잡도를 단계적으로 줄이는 것이다.

## 1. 문제 정의

현재 Overmax의 UI는 PyQt6 기반으로 동작한다.

- `overlay/window.py`: 메인 투명 오버레이
- `overlay/controller.py`: 런타임 이벤트와 Qt UI 연결
- `overlay/settings_window.py`: 설정 창
- `overlay/sync_window.py`: V-Archive 동기화 창
- `overlay/debug_window.py`: 디버그 로그 창
- `overlay/ui/*`: 오버레이 하위 위젯

OpenCV 제거로 이미지 처리 런타임 의존성은 줄었지만, 배포 결과물에는 여전히
PyQt6와 Qt 플러그인 묶음이 큰 비중을 차지한다. 또한 일부 UI 파일은
AGENTS.md의 파일 크기 기준을 이미 초과하고 있어, Qt 정리 작업을 바로
의존성 교체로 시작하면 기능 회귀와 대규모 리팩토링 위험이 크다.

Qt 작업의 핵심 문제는 다음 세 가지다.

- 배포 크기: spec 조정은 이미 상당히 진행되었고, 현재 산출물을 기준으로 한계를 기록해야 한다.
- 안정성: 투명 윈도우, 항상 위, 캡처 제외, 트레이, 시그널 브릿지 동작을 유지해야 한다.
- 유지보수성: 큰 UI 파일을 기능 단위로 나누되, verified pipeline에는 영향을 주지 않아야 한다.

## 2. 원인 분석

### 런타임 의존성

`requirements.txt`는 `PyQt6>=6.6.0`을 런타임 의존성으로 가진다.
`overmax.spec`는 필요한 Qt 모듈을 hidden import로 지정하고, 사용하지 않는
일부 Qt 모듈은 제외 대상으로 둔다.

현재 확인된 PyQt6 사용 영역은 다음 범위에 집중되어 있다.

- `QtWidgets`: `QApplication`, `QWidget`, `QFrame`, layout, button, label, tray
- `QtCore`: `Qt`, `QObject`, `pyqtSignal`, `QPoint`, `QRect`
- `QtGui`: `QPainter`, `QBrush`, `QColor`, `QFont`, `QPen`, `QAction`

즉, Qt는 단순 표시뿐 아니라 이벤트 루프, thread-safe signal, 투명 오버레이,
트레이 메뉴까지 담당한다.

### 코드 구조

파일 크기 기준으로 `settings_window.py`와 `sync_window.py`가 가장 크다.
이 두 파일은 창 구성, row/card 구성, 사용자 입력 처리, 외부 작업 콜백,
상태 갱신 로직이 한 파일에 함께 있다.

큰 파일을 먼저 정리하지 않고 Qt 의존성 교체를 시도하면, 다음 문제가 생긴다.

- UI 동작 회귀 지점을 좁히기 어렵다.
- PyQt6 고유 API와 앱 비즈니스 로직의 결합이 계속 남는다.
- 대체 UI 후보를 검증해도 실제 전환 비용을 예측하기 어렵다.

### 성능 경계

Qt UI는 인게임 중에도 오버레이 표시와 상태 갱신을 수행한다.
따라서 UI 전환 작업은 detection pipeline보다 낮은 우선순위로 실행되어야 하며,
선곡 화면 인식과 verified state commit 흐름을 바꾸면 안 된다.

## 3. 해결 방법 (옵션별)

### 옵션 A: PyQt6 유지 + PyInstaller 패키징 축소

현재 PyQt6 구조는 유지하고, 배포 결과물에서 불필요한 Qt 모듈과 플러그인을
제외한다.

2026-05-11 기준 이 옵션은 이미 충분히 시도되었고, 현재 spec 조정은
실용적인 한계에 도달한 것으로 본다. 이후 작업은 추가 제외보다 측정값 기록과
UI 구조 정리에 둔다.

작업 범위:

- `dist/overmax.zip` 산출물 크기 산출
- `dist/overmax` 내 개별 DLL 크기 산출
- `overmax.spec` 추가 조정 보류 판단 기록
- 앱 import smoke test
- 실제 앱 실행 smoke test

통과 기준:

- 오버레이 표시, 이동, 투명도, 스케일, 트레이 메뉴 정상
- 설정 창, 동기화 창, 디버그 창 열림
- `dist/overmax.zip` 크기 기록
- 주요 DLL 크기 기록

### 옵션 B: PyQt6 유지 + UI 파일 구조 정리

Qt 의존성은 유지하되, 큰 UI 파일을 기능 단위로 분리한다.
이 단계의 목적은 대체 UI 전환이 아니라 회귀 범위 축소다.

우선 분리 대상:

- `overlay/settings_window.py`
  - UI section builder
  - V-Archive 계정 입력/계정 목록
  - opacity/scale controls
  - system actions
- `overlay/sync_window.py`
  - candidate row widget
  - scan/action signal bridge
  - list rendering
  - worker callback handling
- `overlay/controller.py`
  - Qt app/tray lifecycle
  - settings/sync window orchestration
  - verified state to UI state 변환

통과 기준:

- 분리 후 각 파일 500라인 이하
- 함수 50라인 이하 원칙 유지
- 기존 public method 이름과 signal 연결 유지
- UI 동작 smoke test 통과

### 옵션 C: Qt abstraction boundary 도입

PyQt6를 바로 제거하지 않고, controller와 window 사이에 얇은 UI port를 둔다.
이 단계는 장기적으로 다른 UI 런타임을 검토할 수 있게 하는 준비 작업이다.

작업 범위:

- verified state에서 UI update payload로 변환하는 순수 Python 계층 분리
- Qt signal에 직접 실리는 데이터 구조 최소화
- tray/debug/settings/sync 창 생명주기 경계 명확화
- detection/capture/core 패키지에서 Qt import가 없는지 확인

통과 기준:

- detection pipeline import graph에 PyQt6가 섞이지 않음
- UI payload 생성 로직은 Qt 없이 단위 검증 가능
- 기존 Qt window API는 유지

### 옵션 D: 대체 UI 런타임 검토

PyQt6 제거 또는 축소 가능성을 별도 spike로 검토한다.
단, 현재 제약상 Python 환경과 화면 캡처 기반은 유지한다.

검토 후보:

- PySide6: Qt 계열 유지, 패키징 크기/라이선스/호환성 비교
- Win32 직접 오버레이: 크기 축소 가능, 투명 렌더링/입력/트레이 구현 부담 큼
- WebView/HTML UI: 표현력은 좋지만 투명 always-on-top 오버레이와 캡처 제외 보장이 불확실
- Rust native overlay: 장기 후보이나 Python verified pipeline과 bridge 비용이 큼

이 옵션은 바로 구현하지 않는다. 옵션 A~C 이후 수치와 경계가 정리된 뒤
별도 실험 문서로 판단한다.

## 4. 트레이드오프

### PyQt6 유지

장점:

- 현재 기능 안정성이 가장 높다.
- 투명 창, 트레이, signal, widget 생태계를 그대로 쓸 수 있다.
- detection pipeline에 손대지 않아도 된다.

단점:

- 배포 크기 절감 폭이 제한될 수 있다.
- PyInstaller/Qt plugin 이슈가 계속 남는다.

### UI 구조 정리

장점:

- 회귀 범위를 줄이고 리뷰 가능성을 높인다.
- 이후 Qt 대체 여부와 무관하게 유지보수성이 좋아진다.
- AGENTS.md의 파일 크기/함수 크기 기준에 가까워진다.

단점:

- 당장 배포 크기를 크게 줄이지 못할 수 있다.
- UI smoke test 범위가 넓다.

### Qt 대체

장점:

- 성공하면 배포 크기와 런타임 의존성을 크게 줄일 가능성이 있다.

단점:

- 투명 오버레이, 캡처 제외, tray, thread-safe update를 다시 검증해야 한다.
- 인게임 성능과 안정성을 해칠 위험이 가장 크다.
- 전체 리팩토링으로 번질 가능성이 높다.

## 5. 추천안

Qt 작업은 다음 순서로 진행한다.

### Phase 0: 기준 측정

- `dist/overmax.zip` 산출물 크기 기록
- 개별 DLL 크기 기록
- 현재 `overmax.spec`의 Qt include/exclude 조정 한계 확인
- 앱 import smoke test
- 실제 앱 실행 smoke test

산출물:

- `docs/qt-runtime-plan.md`에 기준 수치 갱신
- 추가 spec 조정은 보류한다는 판단 기록

2026-05-11 측정값:

```text
dist/overmax.zip = 45,471,146 bytes (43.36 MiB)
DLL total = 53,580,088 bytes (51.10 MiB), 40 files
PyQt6 directory total = 52,172,052 bytes (49.76 MiB), 128 files
```

주요 DLL 크기:

```text
_internal\PyQt6\Qt6\bin\Qt6Core.dll                                             10,486,584 bytes (10.00 MiB)
_internal\PyQt6\Qt6\bin\Qt6Gui.dll                                               9,544,504 bytes (9.10 MiB)
_internal\PyQt6\Qt6\bin\Qt6Widgets.dll                                           6,588,216 bytes (6.28 MiB)
_internal\PyQt6\Qt6\bin\opengl32sw.dll                                           5,529,744 bytes (5.27 MiB)
_internal\PyQt6\Qt6\bin\Qt6Pdf.dll                                               4,610,360 bytes (4.40 MiB)
_internal\numpy.libs\libscipy_openblas64_-63c857e738469261263c764a36be9436.dll   3,857,408 bytes (3.68 MiB)
_internal\python314.dll                                                          2,013,528 bytes (1.92 MiB)
_internal\PyQt6\Qt6\bin\Qt6Network.dll                                           1,770,808 bytes (1.69 MiB)
_internal\libcrypto-3.dll                                                        1,633,136 bytes (1.56 MiB)
_internal\PyQt6\Qt6\plugins\platforms\qwindows.dll                               1,000,248 bytes (0.95 MiB)
_internal\sqlite3.dll                                                               679,256 bytes (0.65 MiB)
_internal\PyQt6\Qt6\bin\Qt6Svg.dll                                                  640,824 bytes (0.61 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qjpeg.dll                                  577,336 bytes (0.55 MiB)
_internal\numpy.libs\msvcp140-a4c2229bdc2a2a630acdc095b4d86008.dll                  575,056 bytes (0.55 MiB)
_internal\winrt\MSVCP140.dll                                                        567,352 bytes (0.54 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qwebp.dll                                  562,488 bytes (0.54 MiB)
_internal\PyQt6\Qt6\bin\MSVCP140.dll                                                557,728 bytes (0.53 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qtiff.dll                                  441,656 bytes (0.42 MiB)
_internal\PyQt6\Qt6\bin\MSVCP140_2.dll                                              280,200 bytes (0.27 MiB)
_internal\libssl-3.dll                                                              229,744 bytes (0.22 MiB)
_internal\PyQt6\Qt6\plugins\styles\qmodernwindowsstyle.dll                          229,176 bytes (0.22 MiB)
_internal\PyQt6\Qt6\bin\VCRUNTIME140.dll                                            124,544 bytes (0.12 MiB)
_internal\VCRUNTIME140.dll                                                          120,400 bytes (0.11 MiB)
_internal\PyQt6\Qt6\plugins\platforms\qoffscreen.dll                                113,976 bytes (0.11 MiB)
_internal\PyQt6\Qt6\plugins\generic\qtuiotouchplugin.dll                            102,712 bytes (0.10 MiB)
_internal\python3.dll                                                                74,072 bytes (0.07 MiB)
_internal\PyQt6\Qt6\plugins\iconengines\qsvgicon.dll                                 72,504 bytes (0.07 MiB)
_internal\pywin32_system32\pywintypes314.dll                                         63,488 bytes (0.06 MiB)
_internal\PyQt6\Qt6\plugins\platforms\qminimal.dll                                   61,752 bytes (0.06 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qicns.dll                                   55,608 bytes (0.05 MiB)
_internal\PyQt6\Qt6\bin\VCRUNTIME140_1.dll                                           49,792 bytes (0.05 MiB)
_internal\VCRUNTIME140_1.dll                                                         49,776 bytes (0.05 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qgif.dll                                    47,928 bytes (0.05 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qico.dll                                    45,880 bytes (0.04 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qpdf.dll                                    42,296 bytes (0.04 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qsvg.dll                                    39,224 bytes (0.04 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qtga.dll                                    38,200 bytes (0.04 MiB)
_internal\PyQt6\Qt6\plugins\imageformats\qwbmp.dll                                   36,664 bytes (0.03 MiB)
_internal\PyQt6\Qt6\bin\MSVCP140_1.dll                                               35,952 bytes (0.03 MiB)
_internal\libffi-8.dll                                                               29,968 bytes (0.03 MiB)
```

### Phase 1: PyInstaller Qt 패키징 축소

완료로 본다. 사용자가 이미 spec 조정을 충분히 반복했고, 현재가 실용적인
한계라고 판단했다.

성공 조건:

- 추가 spec 조정은 보류
- 이후 기준은 `dist/overmax.zip` 산출물 크기로 비교
- 근거 없는 성능/크기 개선 주장 없이 실제 수치만 기록

### Phase 2: 큰 UI 파일 분리

- `settings_window.py`부터 분리
- 이후 `sync_window.py` 분리
- 마지막으로 `controller.py`의 orchestration 경계 정리

성공 조건:

- 각 파일 500라인 이하 또는 초과 사유 명시
- 함수 50라인 이하 유지
- 기존 창 동작 유지

2026-05-11 결과:

```text
overlay/settings_window.py      365 lines
overlay/settings_varchive.py    361 lines
overlay/controller.py           351 lines
overlay/sync_window.py          312 lines
overlay/sync_candidate_row.py   191 lines
overlay/sync_actions.py         189 lines
overlay/tray_icon.py             47 lines
```

분리 경계:

- `settings_varchive.py`: V-Archive 계정 목록, account.txt 선택, fetch/sync 요청
- `sync_candidate_row.py`: 동기화 후보 행 위젯
- `sync_actions.py`: 후보 scan/upload/delete 백그라운드 작업
- `tray_icon.py`: 시스템 트레이 메뉴 구성과 더블클릭 처리

### Phase 3: UI boundary 정리

- verified state → UI payload 변환 로직을 Qt 독립 계층으로 분리
- Qt signal bridge는 표시 계층에만 남김
- detection/capture/core에서 Qt import가 없는지 확인

성공 조건:

- verified pipeline 변경 없음
- UI payload 로직은 Qt 없이 검증 가능
- 앱 smoke test 통과

### Phase 4: 대체 UI spike 여부 결정

Phase 0~3 결과를 기준으로 대체 UI 런타임 검토 여부를 결정한다.
이 단계 전에는 PyQt6 제거를 목표로 한 대규모 전환을 시작하지 않는다.

## 검증 기준

Qt 작업은 최소 다음 기준을 만족해야 한다.

```text
python -c "import main; print('import ok')"
```

실제 앱 실행 smoke test에서 확인할 항목:

- 게임 창 미검출 시 앱이 정상 대기
- 선곡 화면 진입 시 오버레이 표시
- 오버레이 위치 이동 및 저장
- 투명도/스케일 변경
- 설정 창 열기/닫기
- 동기화 창 열기/닫기
- 디버그 창 로그 표시
- 트레이 메뉴 표시 및 종료
- ROI overlay 토글

## 주의사항

- verified pipeline은 변경하지 않는다.
- detection/capture/core 로직에 Qt 정리 작업을 섞지 않는다.
- 인게임 중 실행되는 경로의 추가 작업량을 늘리지 않는다.
- Qt 대체는 측정과 경계 정리 이후에만 검토한다.
- 전체 리팩토링이 아니라 패키징, 구조 정리, 경계 설정 순서로 진행한다.
