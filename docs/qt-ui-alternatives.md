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

## 2026-05-11 Win32 3차 결과

Win32 smoke에 `--payload-sample` 옵션을 추가해 Qt 독립
`overlay/ui_payload.py` 경계를 실제 Win32 렌더링 입력으로 연결했다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile test\win32_overlay_smoke.py test\ui_payload_check.py overlay\ui_payload.py

.\.venv_build\Scripts\python.exe test\ui_payload_check.py
ui_payload_check_ok

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --import-only
Win32 import ok

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --diagnostics --payload-sample
capture_excluded=True
style_ok=True
dpi=96
rect=(120, 120, 480, 290)
monitor=(0, 0, 1920, 1080)
ex_style=0x08080088

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --show --payload-sample --duration-ms 1000
capture_excluded=True
dpi=96
```

확인된 항목:

- `OverlayPayloadBuilder.build_initial()`과 verified `GameSessionState` update를
  Win32 표시 상태로 변환 가능
- payload 샘플 경로에서도 `WDA_EXCLUDEFROMCAPTURE`와 필수 extended style 유지
- PyQt6 signal/window 경로를 거치지 않고 추천 제목, 모드/난이도, 추천 행,
  footer 요약을 GDI 렌더러에 전달
- verified pipeline과 프로덕션 `overlay/controller.py`, `overlay/window.py` 변경 없음

3차 판단:

- Phase 3에서 분리한 UI payload 경계는 Win32 직접 오버레이 후보에도 재사용 가능하다.
- 다음 검토는 payload 전달 가능성이 아니라 렌더링 품질과 좌표/입력 동작이다.
- 아직 실제 프로덕션 오버레이 교체 단계는 아니며, Win32 후보는 계속 smoke test
  범위에 둔다.

## 2026-05-11 Win32 4차 결과

Win32 smoke에 `--position-check` 옵션을 추가해 위치 계산과 사용자 이동 저장
경계를 확인했다. payload 샘플 헬퍼는 `test/win32_overlay_payload_sample.py`로
분리해 smoke 본문 파일 크기를 500라인 이하로 유지했다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile test\win32_overlay_smoke.py test\win32_overlay_payload_sample.py test\ui_payload_check.py overlay\ui_payload.py overlay\utils.py

.\.venv_build\Scripts\python.exe test\ui_payload_check.py
ui_payload_check_ok

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --diagnostics --payload-sample
capture_excluded=True
style_ok=True
dpi=96
rect=(120, 120, 480, 290)
monitor=(0, 0, 1920, 1080)
ex_style=0x08080088

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --position-check --payload-sample
calculated=(1496, 654)
saved=(1520, 672)
moved=(1532, 682)
callback_position=(1532, 682)
monitor=(0, 0, 1920, 1080)

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --show --payload-sample --duration-ms 1000
capture_excluded=True
dpi=96
```

확인된 항목:

- Win32 후보도 기존 `overlay.utils.calculate_overlay_position()`을 재사용 가능
- 저장된 위치 적용은 `SetWindowPos(..., SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER)`로 처리 가능
- 사용자 이동 완료 시점은 `WM_EXITSIZEMOVE`에서 콜백으로 전달 가능
- 자동 smoke에서는 같은 콜백 경로를 `simulate_user_move()`로 검증
- 기존 PyQt6 프로덕션 경로와 verified pipeline 변경 없음

4차 판단:

- 기본 위치 계산과 수동 위치 저장 경계는 Win32 직접 오버레이에서도 큰 구조 변경 없이 옮길 수 있다.
- 남은 큰 리스크는 실제 DPI 배율/멀티 모니터 환경에서의 좌표 일관성, 텍스트 품질,
  per-pixel alpha 또는 region 기반 rounded rendering 품질이다.
- 트레이/설정/동기화/디버그 창은 계속 Win32 main overlay spike와 분리해 판단한다.

## 2026-05-11 Win32 5차 결과

Win32 smoke에 `--dpi-check` 옵션을 추가해 DPI 배율과 가상 멀티모니터 배치를
계산 검증했다. 실제 고DPI 모니터가 없어도 기본 산식이 monitor rect 밖으로
나가지 않는지 확인하기 위한 순수 smoke이다.

검증:

```text
.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --dpi-check
dpi=96 scale=1.00 size=(360, 170) position=(1496, 654) within_monitor=True monitor=(0, 0, 1920, 1080)
dpi=120 scale=1.25 size=(450, 212) position=(1560, 668) within_monitor=True monitor=(0, 0, 2560, 1440)
dpi=144 scale=1.50 size=(540, 255) position=(3404, 621) within_monitor=True monitor=(1920, 0, 4480, 1440)
dpi=192 scale=2.00 size=(720, 340) position=(-1820, 508) within_monitor=True monitor=(-1920, 0, 0, 1080)
```

확인된 항목:

- 96/120/144/192 DPI에서 window size와 margin을 같은 scale로 계산 가능
- primary monitor, right-side virtual monitor, left-side virtual monitor rect에서
  결과 window가 대상 monitor 내부에 머무름
- `--dpi-check`는 모든 케이스가 `within_monitor=True`일 때만 성공 종료

5차 판단:

- DPI 대응의 계산 경계는 Win32 후보에서도 작게 유지할 수 있다.
- 실제 검증은 125% 이상 DPI 모니터에서 `GetDpiForWindow()` 값과 표시 크기가
  계산 결과와 일치하는지 별도로 봐야 한다.

## 2026-05-11 Win32 6차 결과

Win32 smoke에 `--render-check` 옵션을 추가해 렌더링 품질과 관련된 첫 번째
API 경계를 확인했다.

검증:

```text
.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --render-check --payload-sample
alpha=232
rounded_region=True
font_created=True
font_quality=5
text_extent=(96, 20)
```

확인된 항목:

- `SetLayeredWindowAttributes(..., alpha=232, LWA_ALPHA)` 경로 유지
- `CreateRoundRectRgn`/`SetWindowRgn`으로 rounded window region 적용 성공
- Segoe UI font handle 생성 성공
- `CLEARTYPE_QUALITY` font 설정과 텍스트 extent 산출 성공
- `--render-check`는 alpha, region, font, text extent 조건을 만족해야 성공 종료

6차 판단:

- API 수준에서는 rounded region과 ClearType 기반 텍스트 렌더링을 적용할 수 있다.
- 하지만 GDI `RoundRect`와 window region 조합은 per-pixel alpha가 아니므로,
  PyQt6의 antialias rounded corner와 동일 품질이라고 볼 수는 없다.
- 다음 렌더링 검토는 스크린샷 또는 실제 표시 육안 확인으로 corner edge,
  텍스트 elide, 추천 row 밀도, 고DPI 폰트 크기를 비교해야 한다.

## 2026-05-11 Win32 7차 결과

Win32 smoke에 `--layout-check`와 긴 payload 샘플을 추가해, 텍스트가 실제
오버레이 슬롯 안에서 높이 잘림 없이 처리되는지 확인했다. 렌더링 보조 로직은
`test/win32_overlay_render.py`로 분리해 smoke 본문 파일 크기를 500라인 이하로
유지했다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile test\win32_overlay_smoke.py test\win32_overlay_payload_sample.py test\win32_overlay_render.py

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --layout-check --long-payload-sample
title=width:432/240 height:20/26 fits_width:False fits_height:True
mode_diff=width:43/56 height:20/24 fits_width:True fits_height:True
subtitle=width:226/306 height:20/28 fits_width:True fits_height:True
footer=width:454/306 height:20/20 fits_width:False fits_height:True
recommendation_1=width:581/298 height:20/24 fits_width:False fits_height:True
recommendation_2=width:447/298 height:20/24 fits_width:False fits_height:True
overflowing_cases=4

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --render-check --payload-sample
alpha=232
rounded_region=True
font_created=True
font_quality=5
text_extent=(96, 20)
```

확인된 항목:

- 긴 제목, 추천 행, footer 샘플에서 폭 초과 케이스가 실제로 발생함
- 모든 텍스트 슬롯의 높이 조건은 통과함
- Win32 텍스트 렌더링은 `DT_END_ELLIPSIS`를 사용해 폭 초과를 줄임 처리함
- footer 텍스트 영역은 16px에서 20px로 조정해 Segoe UI 15px font 높이와 맞춤

7차 판단:

- 텍스트 elide와 기본 row 밀도는 자동 smoke에서 더 이상 즉시 탈락하지 않는다.
- 아직 실제 화면 캡처 기반의 corner edge, 안티앨리어싱, 고DPI 폰트 품질 비교는
  남아 있다.
- 프로덕션 PyQt6 오버레이 교체는 계속 시작하지 않는다.

## 2026-05-11 Win32 8차 결과

Win32 렌더링을 실제 화면에 띄우기 전 단계로, 같은 GDI drawing path를 메모리
DC에 그리고 픽셀 샘플을 확인하는 smoke를 추가했다. 목적은 렌더링 API 성공
여부를 넘어서, panel/text/accent/status/divider가 실제 픽셀로 찍히는지를
자동으로 확인하는 것이다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile test\win32_overlay_pixel_check.py test\win32_overlay_smoke.py test\win32_overlay_render.py test\win32_overlay_payload_sample.py

.\.venv_build\Scripts\python.exe test\win32_overlay_pixel_check.py
total_pixels=61200
non_blank_pixels=52624
panel_bg_pixels=43401
bright_text_pixels=2828
accent_pixels=1072
cyan_pixels=26
divider_pixels=310
unique_colors=123
```

확인된 항목:

- 메모리 DC 렌더링이 blank frame이 아님
- 패널 배경색 픽셀이 충분히 검출됨
- 밝은 텍스트, 제목 강조색, cyan 상태 램프, divider 픽셀이 각각 검출됨
- 긴 payload 샘플 기반 렌더링 경로에서도 픽셀 색상 다양성이 확인됨

8차 판단:

- Win32 후보의 GDI 렌더링 경로는 API 성공뿐 아니라 실제 픽셀 생성까지 자동
  smoke로 확인 가능하다.
- 이 검증은 화면 합성, layered alpha의 실제 desktop blend, rounded edge의
  육안 품질, 고DPI ClearType 품질을 완전히 대체하지 않는다.
- 다음 검토는 `--show` 또는 별도 screenshot 기반으로 PyQt6 오버레이와 실제
  표시 품질을 비교하는 단계가 적절하다.

## 2026-05-11 Win32 9차 결과

Win32 layered window에서 ClearType 가장자리 품질이 흔들리지 않도록,
텍스트 렌더링 시 실제 배경색을 지정하고 `OPAQUE` 배경 모드를 사용하게 조정했다.

검증:

```text
.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --render-check --payload-sample
alpha=232
rounded_region=True
font_created=True
font_quality=5
text_extent=(96, 20)

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --layout-check --long-payload-sample
title=width:432/240 height:20/26 fits_width:False fits_height:True
mode_diff=width:43/56 height:20/24 fits_width:True fits_height:True
subtitle=width:226/306 height:20/28 fits_width:True fits_height:True
footer=width:454/306 height:20/20 fits_width:False fits_height:True
recommendation_1=width:581/298 height:20/24 fits_width:False fits_height:True
recommendation_2=width:447/298 height:20/24 fits_width:False fits_height:True
overflowing_cases=4

.\.venv_build\Scripts\python.exe test\win32_overlay_pixel_check.py
total_pixels=61200
non_blank_pixels=52624
panel_bg_pixels=43401
bright_text_pixels=2828
accent_pixels=1072
cyan_pixels=26
divider_pixels=310
unique_colors=123

.\.venv_build\Scripts\python.exe test\win32_overlay_smoke.py --show --payload-sample --duration-ms 1000
capture_excluded=True
dpi=96
```

확인된 항목:

- 텍스트 출력은 `TRANSPARENT` 배경 모드가 아니라 실제 패널/배지 배경색을 가진
  `OPAQUE` 배경 모드를 사용한다.
- `--render-check`, `--layout-check --long-payload-sample`, 픽셀 smoke,
  실제 표시 루프가 모두 통과했다.
- 긴 텍스트는 폭 초과를 허용하되 높이 잘림 없이 `DT_END_ELLIPSIS` 경로로 처리한다.
- Win32 smoke는 여전히 프로덕션 PyQt6 경로와 verified pipeline을 변경하지 않는다.

9차 판단:

- Win32 직접 오버레이는 메인 오버레이 한정 프로덕션 후보로 승격해도 된다.
- 전환은 PyQt6 전체 제거가 아니라, 메인 overlay window만 opt-in으로 바꾸는
  Phase 5로 진행한다.
- 트레이, 설정, 동기화, 디버그 창은 PyQt6에 남겨 전환 범위를 작게 유지한다.
- 기본값 전환은 실제 앱 smoke test와 PyInstaller 산출물 검증 이후 별도로 판단한다.

## 2026-05-11 Win32 Phase 5 시작 결과

Phase 5의 첫 범위는 프로덕션 코드 위치 확정과 smoke/prod 경계 분리로 제한했다.
Win32 메인 오버레이 후보는 `overlay/win32/` 패키지로 이동하고, smoke CLI는
해당 패키지를 호출하는 검증 도구로 축소했다.

결정:

- 프로덕션 Win32 main overlay 후보 위치는 `overlay/win32/`로 둔다.
- `overlay/win32/window.py`는 창 생성, noactivate/topmost/layered 속성, 캡처 제외,
  위치 이동 콜백, diagnostics를 담당한다.
- `overlay/win32/render.py`는 GDI drawing path와 렌더링 diagnostics를 담당한다.
- `overlay/win32/view_state.py`는 `overlay/ui_payload.py`의 payload를 Win32 표시
  상태로 변환하는 경계로 둔다.
- 기존 `test/win32_overlay_smoke.py`는 프로덕션 구현을 import해서 확인하는 CLI로만
  유지한다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\win32\__init__.py overlay\win32\geometry.py overlay\win32\view_state.py overlay\win32\render.py overlay\win32\window.py test\win32_overlay_smoke.py test\win32_overlay_payload_sample.py test\win32_overlay_pixel_check.py test\win32_overlay_geometry.py test\win32_overlay_render.py
```

판단:

- Phase 5의 첫 두 항목은 verified pipeline, detection/capture/core, recommendation
  로직을 변경하지 않고 완료했다.
- 다음 작업은 `overlay/ui_payload.py` payload를 실제 Win32 overlay update 입력으로
  연결하는 단계다.
