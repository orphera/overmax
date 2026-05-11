# Qt UI Alternatives

이 문서는 PyQt6 대체 UI를 하나씩 검토하기 위한 Phase 4 작업 기록이다.
목표는 verified pipeline을 유지하면서 UI 런타임 교체 가능성을 작게 확인하는
것이다.

## 1. 문제 정의

Phase 0~3에서 PyQt6 패키징 축소와 UI 경계 정리는 완료했다. 남은 문제는
PyQt6 자체가 배포 산출물에서 큰 비중을 차지한다는 점이다.

단, PyQt6는 단순 표시 레이어가 아니다.

- 투명 오버레이 창
- always-on-top / tool window
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`
- thread-safe signal bridge
- 트레이 메뉴
- 설정, 동기화, 디버그 보조 창

따라서 대체 UI 검토는 크기 절감 가능성보다 기존 오버레이 조건을 유지할 수
있는지를 먼저 확인한다.

## 2. 원인 분석

현재 PyQt6 import는 `overlay/`에만 집중되어 있다. Phase 3에서
verified state -> UI payload 변환은 Qt 독립 계층으로 분리했으므로,
대체 UI 검토는 detection/capture/core를 건드리지 않고 진행할 수 있다.

PyQt6 대체에서 반드시 지켜야 할 조건은 다음과 같다.

- 기존 verified pipeline 변경 없음
- 인게임 중 추가 처리량 증가 없음
- UI 실패가 detection 상태 commit에 영향을 주지 않음
- 실패 시 PyQt6 경로로 쉽게 되돌릴 수 있음

## 3. 해결 방법

### 옵션 A: PySide6

Qt 계열을 유지하는 가장 낮은 위험 후보이다.

검토 항목:

- `PySide6.QtCore`, `QtGui`, `QtWidgets` import 가능 여부
- `pyqtSignal` -> `Signal` 치환 비용
- enum / event API 호환성
- PyInstaller 산출물 크기
- 투명 오버레이와 `WDA_EXCLUDEFROMCAPTURE` 동작

최소 spike:

```text
test/pyside6_overlay_smoke.py --import-only
test/pyside6_overlay_smoke.py --show
```

중단 기준:

- PyQt6보다 산출물 크기가 의미 있게 줄지 않음
- signal/API 치환이 `overlay/` 전반의 대규모 수정으로 번짐
- 투명 오버레이 또는 캡처 제외 동작이 불안정함

### 옵션 B: Win32 직접 오버레이

배포 크기 절감 가능성이 가장 큰 후보이다. 대신 투명 렌더링, 클릭 처리,
트레이, DPI, 폰트 렌더링을 직접 책임져야 한다.

검토 순서:

1. 메인 오버레이 표시만 최소 구현
2. 캡처 제외와 always-on-top 확인
3. 트레이와 보조 창은 후순위로 분리

중단 기준:

- 렌더링 품질이 PyQt6 대비 떨어짐
- 입력/이동/투명도 처리 코드가 과도하게 커짐
- 인게임 성능 측정 없이 hot path에 영향을 줄 가능성이 생김

### 옵션 C: WebView / HTML 보조 창

설정/동기화/디버그 창에는 후보가 될 수 있다. 메인 투명 오버레이 후보로는
우선순위를 낮게 둔다.

검토 항목:

- 보조 창 표현력
- 배포 크기
- Python bridge 복잡도
- 메인 오버레이와 분리 가능한지

### 옵션 D: Rust native overlay

장기 후보이다. 현재는 Python verified pipeline을 유지해야 하므로 바로
전환하지 않는다.

검토 조건:

- Rust extension 빌드/패키징이 안정화된 뒤
- Python UI 런타임 제거 효과가 bridge 비용보다 클 때
- 메인 오버레이만 독립 프로세스 또는 얇은 native layer로 분리할 수 있을 때

## 4. 트레이드오프

PySide6는 실패 비용이 낮지만 크기 절감 폭이 작을 수 있다. Win32 직접
오버레이는 크기 절감 가능성이 크지만 구현과 검증 부담이 크다. WebView는
보조 창에는 적합할 수 있으나 메인 오버레이 요건과 충돌할 수 있다. Rust
native overlay는 장기적으로 가장 깔끔할 수 있지만 현재 단계에서는 범위가
크다.

## 5. 추천안

검토 순서는 다음으로 한다.

1. PySide6 최소 spike
2. PySide6 패키징 크기 측정
3. PySide6 전환 비용 산정
4. 절감 효과가 부족하면 Win32 직접 오버레이 spike 검토
5. WebView는 보조 창 전용 후보로 보류
6. Rust native overlay는 장기 후보로 보류

Phase 4의 첫 판단은 PySide6가 PyQt6의 저위험 대체재인지 확인하는 것이다.
이 검토가 실패하더라도 기존 PyQt6 앱 경로는 유지한다.

## 2026-05-11 PySide6 1차 결과

현재 `.venv_build` 기준 PySide6 import와 최소 오버레이 smoke test는 통과했다.

검증:

```text
.\.venv_build\Scripts\python.exe test\pyside6_overlay_smoke.py --import-only
PySide6 import ok

.\.venv_build\Scripts\python.exe test\pyside6_overlay_smoke.py --show
capture_excluded=True
```

설치된 버전:

```text
PySide6 6.11.0
shiboken6 6.11.0
```

site-packages 파일 크기 기준:

```text
PyQt6 directory total = 217,126,188 bytes (207.07 MiB)
PySide6 directory total = 658,527,924 bytes (628.02 MiB)
```

PySide6 메타 패키지는 Addons까지 포함하므로 그대로 쓰면 PyQt6보다 훨씬 크다.
패키지 RECORD 기준으로 보면 크기 구성이 다음과 같다.

```text
PySide6 files=74 bytes=4,994,728 (4.76 MiB)
PySide6_Essentials files=2456 bytes=208,701,102 (199.03 MiB)
PySide6_Addons files=1321 bytes=457,746,364 (436.54 MiB)
shiboken6 files=73 bytes=3,092,171 (2.95 MiB)
```

1차 판단:

- PySide6는 최소 오버레이 기능 조건을 만족할 가능성이 있다.
- 크기 절감 목적이라면 `PySide6` 메타 패키지는 부적합하다.
- `PySide6_Essentials + shiboken6`만 사용해도 PyQt6와 비슷한 크기라,
  전환 비용을 정당화할 만큼의 배포 크기 절감은 아직 확인되지 않았다.
- 다음 판단은 실제 PyInstaller 후보 산출물을 만들 때 Addons 제외가 가능한지
  확인해야 한다.

PyInstaller 최소 패키징 결과:

```text
.\.venv_build\Scripts\pyinstaller.exe --noconfirm --clean --name pyside6_overlay_smoke \
  --distpath scratch\pyside6_dist \
  --workpath scratch\pyside6_build \
  --specpath scratch\pyside6_spec \
  test\pyside6_overlay_smoke.py

scratch\pyside6_dist\pyside6_overlay_smoke = 116,234,031 bytes (110.85 MiB)
scratch\pyside6_overlay_smoke.zip = 46,519,077 bytes (44.36 MiB)
dist\overmax = 74,988,168 bytes (71.51 MiB)
dist\overmax.zip = 45,471,146 bytes (43.36 MiB)
```

PySide6 최소 smoke만 묶은 ZIP이 현재 전체 앱 ZIP보다 약간 크다. PyInstaller
hook은 최소 QtWidgets 스크립트에서도 `Qt6Network`, `Qt6Pdf`, `Qt6Quick`,
`Qt6Qml`, `opengl32sw.dll` 등을 함께 포함했다.

2차 판단:

- PySide6는 기능적으로 저위험 후보지만, 배포 크기 절감 후보로는 부적합하다.
- PyQt6에서 PySide6로 바꾸는 포팅 비용을 정당화할 만한 이득이 확인되지 않았다.
- 현재 단계에서는 PyQt6 유지가 합리적이다.
- 크기 축소를 계속 노린다면 다음 spike는 Win32 직접 오버레이를 메인 오버레이
  한정으로 검토한다.

## 검증 기준

PySide6 최소 spike는 다음을 확인한다.

```text
python test/pyside6_overlay_smoke.py --import-only
python test/pyside6_overlay_smoke.py --show
```

`--import-only`는 GUI를 띄우지 않는다. `--show`는 실제 투명 창, always-on-top,
캡처 제외 API 호출, Qt event loop 진입을 확인한다.

PySide6를 설치하지 않은 환경에서는 `--import-only`가 명확한 실패 메시지를
출력해야 한다.

## 주의사항

- `overlay/` 프로덕션 코드는 PySide6 spike 결과가 확인되기 전까지 변경하지 않는다.
- `requirements.txt`는 PySide6 전환 판단 전까지 변경하지 않는다.
- `overmax.spec`는 PySide6 패키징 측정 단계에서만 별도 후보 파일로 실험한다.
- detection/capture/core에는 Qt 대체 작업을 섞지 않는다.

## 2026-05-11 Win32 1차 결과

Win32 직접 오버레이는 프로덕션 코드와 분리한 smoke test로 먼저 확인했다.

검증:

```text
.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --import-only
Win32 import ok

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --show
capture_excluded=True
```

확인된 항목:

- `pywin32` 런타임 import 가능
- `WS_EX_TOPMOST`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE` 조합으로
  최소 창 생성 가능
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` 호출 성공
- Qt 없이 2초 표시 후 정상 종료되는 최소 메시지 루프 확인

1차 판단:

- 메인 오버레이 한정으로는 Win32 직접 구현 가능성을 더 검토할 만하다.
- PyQt6 전체 제거 후보로 바로 보기에는 아직 부족하다.
- 설정, 동기화, 디버그, 트레이는 Win32 spike 범위에 섞지 않는다.
- verified pipeline과 `overlay/ui_payload.py`는 변경하지 않는다.

다음 확인 항목:

- 현재 PyQt6 오버레이와 비교한 렌더링 품질
- 둥근 모서리/부분 투명도 구현 방식
- DPI scaling과 멀티 모니터 좌표 처리
- 사용자 이동, 위치 저장, 게임 포커스 복원
- PyInstaller 산출물 크기 비교

## 2026-05-11 Win32 2차 결과

1차 smoke를 확장해 Win32 창 속성, DPI, 모니터 정보, 간단한 샘플 렌더링,
PyInstaller 산출물 크기를 확인했다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile test\win32_overlay_smoke.py

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --import-only
Win32 import ok

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --diagnostics
capture_excluded=True
style_ok=True
dpi=96
rect=(120, 120, 480, 290)
monitor=(0, 0, 1920, 1080)
ex_style=0x08080088

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --show --duration-ms 3000
capture_excluded=True
dpi=96
```

PyInstaller 최소 패키징:

```text
.\.venv_build\Scripts\pyinstaller.exe --noconfirm --clean --name win32_overlay_smoke \
  --distpath scratch\win32_dist \
  --workpath scratch\win32_build \
  --specpath scratch\win32_spec \
  test\win32_overlay_smoke.py

scratch\win32_dist\win32_overlay_smoke = 19,361,810 bytes (18.46 MiB)
scratch\win32_overlay_smoke.zip = 8,692,687 bytes (8.29 MiB)

scratch\win32_dist\win32_overlay_smoke\win32_overlay_smoke.exe --diagnostics
capture_excluded=True
style_ok=True
dpi=96
rect=(120, 120, 480, 290)
monitor=(0, 0, 1920, 1080)
ex_style=0x08080088
```

2차 판단:

- Win32 직접 오버레이는 메인 오버레이 한정으로 크기 절감 가능성이 크다.
- `WS_EX_LAYERED`, `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE` 조합은
  현재 요구와 맞는다.
- `WDA_EXCLUDEFROMCAPTURE`는 소스 실행과 PyInstaller 산출물 모두에서 성공했다.
- DPI와 모니터 rect는 읽을 수 있으나, 실제 멀티 모니터/고DPI 배치 검증은 남아 있다.
- GDI 렌더링은 최소 표시에는 충분하지만, 현재 PyQt6 UI 품질과 동일하게 맞추려면
  별도 렌더링 계층 설계가 필요하다.

다음 확인 항목:

- 실제 `overlay/ui_payload.py` 샘플 데이터를 Win32 렌더링 입력으로 흘려보기
- PyQt6 메인 오버레이와 동일한 위치 계산/사용자 이동 저장 확인
- 멀티 모니터와 125% 이상 DPI에서 좌표 및 크기 확인
- 텍스트 품질, rounded region, per-pixel alpha 적용 비용 검토
- 트레이/설정/동기화/디버그 창은 계속 PyQt6에 남기는 혼합 전략 검토
