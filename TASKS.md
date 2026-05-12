# TASKS

Overmax의 현재 작업은 Python 기반 verified pipeline을 유지하면서,
OpenCV 런타임 의존성 제거와 Qt/PyQt6 영역의 기준 정리를 마친 뒤,
Win32 직접 오버레이를 프로덕션 메인 오버레이 후보로 단계적으로 연결하고,
PyQt6/Win32 오버레이 간 룩앤필 차이를 줄이는 것이다.

OpenCV 제거 상세 기록은 `docs/opencv-to-rust-plan.md`를 따른다.
Qt 정리 상세 계획은 `docs/qt-runtime-plan.md`를 따른다.
PyQt6 대체 UI 검토는 `docs/qt-ui-alternatives.md`를 따른다.

## 완료: Rust HOG 검증

- [x] `rust/overmax_cv` PyO3 확장 골격 유지
- [x] Python 3.14 환경에서 빌드되도록 PyO3 버전 조정
- [x] `maturin develop --release`로 `.venv_build`에 설치 확인
- [x] `test/hog_compat_check.py --backend rust` 검증 경로 추가
- [x] 실제 재킷 이미지셋으로 DB top-1 기준 확인
- [x] OpenCV HOG에 더 가깝게 block-local 투표, Gaussian block weight, border gradient 보정 적용
- [x] 기준 통과 전까지 `detection/image_db.py` 프로덕션 경로 변경 금지

## 완료: OpenCV 제거 Phase 1

- [x] OpenCV 사용 지점 조사
- [x] 단계별 이전 문서 작성
- [x] Rust feature API 추가: grayscale, area resize, hash, HOG
- [x] `detection/image_db.py`에서 런타임 `cv2` 제거
- [x] `capture/helpers.py` thumbnail 경로에서 `cv2` 제거
- [x] `test/jackets` 795개 top-1 검증
- [x] Phase 1 결과 문서 갱신

## 완료: OpenCV 제거 Phase 2

- [x] Rust OCR preprocess API 추가: 3x upscale, grayscale, Otsu, padding, BMP encoding
- [x] `detection/ocr_wrapper.py`에서 `cv2` 제거
- [x] OCR import smoke test
- [x] 가능하면 정적 OCR ROI 샘플 비교

## 완료: OpenCV 제거 Phase 3

- [x] `runtime_patch.py`의 `patch_cv2()` 제거
- [x] `overmax.spec` hiddenimports에서 `cv2` 제거
- [x] `requirements.txt`에서 `opencv-python-headless` 제거
- [x] OpenCV 기반 검증/개발 도구용 `requirements-dev.txt` 분리
- [x] PyInstaller 빌드 후 결과물 확인
- [x] 앱 import smoke test
- [x] 실제 앱 실행 smoke test

## 완료: Qt 런타임 정리 Phase 0

- [x] PyQt6 사용 지점 조사
- [x] Qt 런타임 정리 계획 문서 작성
- [x] `dist/overmax.zip` 산출물 크기 기록
- [x] 개별 DLL 크기 기록
- [x] 현재 `overmax.spec`의 Qt include/exclude 조정 한계 확인
- [x] 앱 import smoke test
- [x] 실제 앱 실행 smoke test

## 완료: Qt 런타임 정리 Phase 1

- [x] 사용하지 않는 Qt plugin/module 제외 후보 검토
- [x] `overmax.spec` 조정 한계 도달 판단
- [x] 추가 spec 조정은 보류
- [x] 배포 산출물 크기 기준은 `dist/overmax.zip`으로 기록

## 완료: Qt UI 구조 정리 Phase 2

- [x] `overlay/settings_window.py` 기능 단위 분리
- [x] `overlay/sync_window.py` 기능 단위 분리
- [x] `overlay/controller.py` orchestration 경계 정리
- [x] 각 파일 500라인 이하 또는 초과 사유 명시

## 완료: Qt 경계 정리 Phase 3

- [x] verified state → UI payload 변환 로직을 Qt 독립 계층으로 분리
- [x] Qt signal bridge를 표시 계층 경계에만 유지
- [x] detection/capture/core에서 Qt import 없음 확인

## 완료: Qt 대체 UI Phase 4

- [x] Phase 0~3 결과 기준으로 PyQt6 대체 spike 필요성 판단
- [x] 대체 UI 후보 평가 기준 문서화
- [x] PySide6 import/package 크기 기준 확인
- [x] PySide6 최소 오버레이 smoke test
- [x] PyQt6 유지 / PySide6 전환 / Win32 후속 spike 중 다음 방향 결정
- [x] Win32 직접 오버레이 최소 smoke test 추가
- [x] Win32 topmost/layered/noactivate/capture exclusion 확인
- [x] Win32 diagnostics로 style/DPI/monitor 정보 확인
- [x] Win32 smoke PyInstaller 산출물 크기 측정
- [x] `overlay/ui_payload.py` 샘플 데이터를 Win32 렌더링 입력으로 연결
- [x] Win32 위치 계산, 저장 위치 적용, 사용자 이동 콜백 smoke 확인
- [x] Win32 DPI/멀티모니터 좌표 계산 smoke 확인
- [x] Win32 alpha/rounded region/ClearType font diagnostics 확인
- [x] Win32 긴 텍스트 elide/레이아웃 밀도 smoke 확인
- [x] Win32 메모리 DC 픽셀 렌더링 smoke 확인
- [x] Win32 ClearType 텍스트 배경 모드 보정 smoke 확인
- [x] 대체 UI 후보를 검토하더라도 verified pipeline 변경 금지

결론:

- PySide6는 최소 기능 smoke test를 통과했지만 배포 크기 절감 근거가 부족해 보류한다.
- Win32 직접 오버레이는 메인 오버레이 한정 프로덕션 후보로 승격한다.
- `scratch\win32_overlay_smoke.zip`은 8,692,687 bytes (8.29 MiB)로 측정되어,
  Qt 기반 산출물 대비 크기 절감 가능성이 충분하다.
- Win32 smoke는 창 속성, 캡처 제외, payload 연결, 위치 저장, DPI/멀티모니터
  계산, 렌더링 API, 긴 텍스트 레이아웃, 메모리 DC 픽셀 검증, ClearType 배경
  모드까지 통과했다.
- 상세 기록은 `docs/qt-ui-alternatives.md`의 Win32 1~9차 결과를 따른다.

## 진행 중: Win32 프로덕션 전환 Phase 5

목표는 PyQt6 전체 제거가 아니라, 검증된 Win32 렌더링 경로를 메인 오버레이에
먼저 연결하고 보조 창은 기존 PyQt6 경로에 남기는 것이다.

- [x] 프로덕션 코드용 Win32 overlay module 위치 결정
- [x] smoke test helper와 프로덕션 renderer 경계 분리
- [x] `overlay/ui_payload.py` payload를 Win32 overlay update 입력으로 연결
- [x] PyQt6 메인 오버레이와 Win32 메인 오버레이를 설정/플래그로 선택 가능하게 구성
- [x] 기본 경로는 기존 PyQt6로 유지하고 Win32는 opt-in으로 시작
- [x] overlay 표시/숨김, 위치 이동/저장, opacity, scale 반영
- [x] game focus를 방해하지 않는 noactivate/topmost 동작 확인
- [x] 캡처 제외 실패 시 verified pipeline에는 영향 없이 UI만 보수적으로 동작
- [x] 트레이, 설정, 동기화, 디버그 창은 PyQt6 유지
- [x] Win32 경로에서도 import smoke 및 실제 앱 smoke test 통과
- [x] PyInstaller 빌드 후 Win32 opt-in 경로 import/실행 확인
- [x] 배포 전 기본값 전환 여부를 별도 판단

Phase 5에서는 detection/capture/core와 recommendation 로직을 변경하지 않는다.
Win32 경로가 실패해도 verified state commit과 기록 저장 흐름은 영향을 받지
않아야 한다.

2026-05-11 확인:
- Win32 diagnostics에서 `capture_excluded=True`, `style_ok=True`,
  `noactivate=True`, `topmost=True`, `focus_preserved=True`를 확인했다.
- `SetWindowDisplayAffinity` 실패는 예외로 전파하지 않고, Win32 overlay 표시만
  억제한다. detection/capture/core/recommendation 경로는 변경하지 않았다.
- `overlay/tray_icon.py`, `overlay/settings_window.py`, `overlay/sync_window.py`,
  `overlay/debug_window.py`는 PyQt6 경로에 남아 있음을 확인했다.
- `.venv_build` 기준 Win32 overlay import smoke와 `win32_overlay_smoke.py
  --diagnostics`가 통과했다.
- PyInstaller 산출물 `dist/overmax/overmax.exe`가 존재하고, Win32는 배포
  기본값으로 전환하지 않고 opt-in 경로로 유지한다.

## 완료: PyQt6 / Win32 룩앤필 정렬 Phase 6

목표는 PyQt6와 Win32가 서로 다른 UI로 느껴지지 않도록, 같은
`overlay/ui_payload.py` 입력에서 색상, 타이포그래피, 간격, 상태 표현을
최대한 같은 기준으로 맞추는 것이다. 이 단계에서는 detection/capture/core,
recommendation, verified pipeline은 변경하지 않는다.

- [x] PyQt6 메인 오버레이와 Win32 메인 오버레이의 현재 차이 목록화
  - 배경/테두리/투명도, 폰트 크기/굵기, 줄간격, 패딩, 정렬, 긴 텍스트 처리,
    색상, verified/unverified 상태 표현을 비교한다.
- [x] PyQt6 쪽에서 실제 사용자 기준 룩앤필 원본으로 삼을 요소와 버릴 요소를 결정
  - 단순히 PyQt6를 복제하지 않고, 가독성/성능/캡처 제외 안정성에 영향을 주는
    요소는 Win32 기준으로 재판단한다.
- [x] Win32 renderer의 스타일 상수를 payload 처리와 분리
  - 색상, spacing, typography, opacity, state color를 한 곳에서 조정할 수 있게
    정리하되, 과한 abstraction은 만들지 않는다.
- [x] 동일 payload fixture로 Win32 스크린샷 또는 픽셀 기준 비교 경로 추가
  - 가능하면 기존 `test/win32_overlay_smoke.py` 흐름을 확장하고, dev server는
    사용하지 않는다.
- [x] 긴 제목, 한글/영문 혼합, 낮은 opacity, scale 변경, verified=False 상태를
  대표 케이스로 고정
- [x] DPI/멀티모니터/scale 변경 시 Win32 레이아웃이 PyQt6 대비 과하게 밀리지
  않는지 확인
- [x] Win32 opt-in 상태를 유지한 채 실제 앱 smoke로 표시/숨김, 위치 저장,
  noactivate/topmost/capture exclusion이 유지되는지 재확인

2026-05-11 진행:
- PyQt6 기준의 panel/header/recommendation/footer 색상 계층을 Win32 renderer에
  반영했다.
- Win32 스타일 상수는 `overlay/win32/style.py`로 분리했고,
  `overlay/win32/view_state.py` payload 처리 경계는 유지했다.
- `test/win32_overlay_smoke.py --mixed-unstable-sample`로 한글/영문 혼합,
  `verified=False`, 긴 footer 대표 케이스를 확인할 수 있게 했다.
- PyQt6 자동 스크린샷 비교는 아직 추가하지 않았고, 현재 검증은 Win32 layout,
  render, pixel, diagnostics, dpi check 기준이다.
- PyQt6 샘플 화면 기준으로 Win32 논리 높이를 확장하고, 좌측 난이도 탭,
  6개 추천 행, 난이도 배지, P/M 상태 배지, rate 색상, 한국어 footer 표현을
  renderer에 맞췄다.
- Win32 GDI round rect의 불필요한 외곽선을 제거하고, 탭/배지 라벨 중앙 정렬과
  설정 버튼 클릭 콜백을 PyQt6 `settings_requested` 흐름에 맞췄다.
- 1.25배 스크린샷 기준으로 header meta 중앙 정렬, footer 오른쪽 값 정렬,
  설정 아이콘 `⚙` 위치와 body 시작 여백을 추가 보정했다.
- Win32 캡처 비교 기준으로 추천 row 배경의 왼쪽 시작선을 배지 시작선에 맞추고,
  `⚙`는 `Segoe UI Emoji` 전용 폰트로 렌더링하도록 분리했다.
- `overlay/win32/window.py`가 `overlay/win32/geometry.py`와 같은 DPI scale 기준으로
  창 크기, renderer scale, 설정 버튼 hit-test, rounded region을 계산하도록 맞췄다.
  `WM_DPICHANGED`도 같은 경로로 처리한다.
- Win32 renderer의 scale helper 오타를 바로잡아 diagnostics/render/layout/pixel
  smoke가 실제 GDI 렌더링 경로까지 통과하도록 확인했다.
- PyQt6 기준 padding/margin을 다시 대조해 Win32 logical height를 337로 조정하고,
  header 내부 12/8 margin, body gap 6, 추천 목록 top/bottom 8, 추천 행 30px,
  행 간격 3px 기준에 맞춰 Win32 row/tab/footer 좌표를 보정했다.
- GDI `DrawText`의 emoji font fallback 차이로 설정 아이콘 렌더링이 불안정해,
  Win32 설정 아이콘은 폰트 글리프 대신 GDI line/circle drawing으로 처리한다.

2026-05-12 진행:
- PyQt6 메인 오버레이의 font-size/font-weight 기준을 Win32 스타일 상수로 옮겼다.
  제목 14/700, mode badge 12/900, meta 10/600, 추천 곡명 11/600,
  난이도 badge 10/700, rate/footer 값 11/700, P/M badge 9/800 기준이다.
- Win32 renderer가 텍스트별 PyQt6 기준 웨이트 상수를 사용하도록 바꿨고,
  기존 ClearType font 생성 및 높이 fit 조건은 유지됨을 확인했다.
- PyQt6 `QLabel`의 실제 font metrics를 확인해 Win32 GDI font cell height를
  보정했다. 10px 텍스트는 13px line height, 11px 텍스트는 15px line height가
  되도록 조정해 PyQt6 쪽 line spacing에 더 가깝게 맞췄다.
- Header/body/footer 사이가 과하게 벌어져 보이지 않도록 Win32 body 시작과
  footer 위치를 각각 2px 위로 조정했다. Qt 쪽 panel spacing/padding 기준은
  유지하되 시각적 밀도만 소폭 맞춘다.
- Qt layout margin이 서로 맞닿는 구간은 합산값이 아니라 절반 기준으로 적용해야
  하므로, header-body/body-footer 간격과 body 내부 좌우 간격을 다시 줄였다.
  Win32 row/tab/footer 좌표를 이 기준으로 재조정했다.
- Win32 텍스트 렌더링을 Direct2D DCRenderTarget + DirectWrite 기반 경로로
  전환했다. 초기화 또는 draw 실패 시 기존 GDI 텍스트로 fallback하며, 설정
  아이콘은 DirectWrite `Segoe UI Symbol` 글리프를 우선 사용하고 실패 시 기존
  GDI primitive 아이콘으로 돌아간다.
- `test/win32_overlay_smoke.py --render-check --payload-sample`에서
  `directwrite_available=True`, `directwrite_used=True`를 확인했다. DirectWrite
  안티앨리어싱으로 픽셀 스모크의 중간색 샘플 기준은 소폭 완화했다.
- 실제 오버레이 캡처에서 center/right 텍스트 정렬이 서로 뒤바뀐 것을 확인해,
  DirectWrite `DWRITE_TEXT_ALIGNMENT_TRAILING/CENTER` 상수값을 바로잡았다.
- 실제 오버레이 캡처에서 텍스트가 아래쪽으로 붙어 보이는 것을 확인해,
  DirectWrite `DWRITE_PARAGRAPH_ALIGNMENT_CENTER` 상수값을 바로잡았다.
- 실제 오버레이 캡처에서 DirectWrite 정렬이 정상화된 것을 확인한 뒤, 텍스트
  렌더링의 GDI `DrawText` fallback 경로를 제거했다. 설정 아이콘도 DirectWrite
  글리프만 사용하며 GDI primitive fallback은 제거했다.
- Win32 `WM_PAINT` 경로가 창 DC에 직접 그리던 구조라 draw 단계가 노출될 수
  있어, 메모리 DC/compatible bitmap에 한 프레임을 먼저 완성한 뒤 `BitBlt`로
  복사하는 double buffering 경로를 추가했다.
- PyQt6 자동 스크린샷 비교는 만들지 않았지만, Win32 layout/render/pixel/
  diagnostics/dpi/position smoke와 실제 캡처 기반 보정으로 Phase 6의 목적은
  충족한 것으로 보고 닫는다.

완료 기준:
- [x] 같은 payload에서 PyQt6와 Win32 오버레이가 한 화면 안에서 명확히 다른
  제품처럼 보이지 않는다.
- [x] Win32 경로의 성능, noactivate/topmost, capture exclusion 방어 동작이
  Phase 5 기준에서 후퇴하지 않는다.
- [x] 룩앤필 조정은 renderer/style 계층에 한정되고, verified state commit과
  기록 저장 흐름에는 영향이 없다.

## 예정: Win32 UI Infra 정리 Phase 7

목표는 Phase 5~6에서 검증한 Win32 overlay 구현을 다른 창으로 확장할 수 있는
얇은 내부 Infra로 정리하는 것이다. 이 단계는 PyQt6 보조 창을 즉시 제거하는
단계가 아니며, 트레이, 설정, 동기화, 디버그, 앱 업데이트 진행 알림을 이후
Win32로 옮길 때 반복될 창 생성, 메시지 처리, DPI, 렌더링, 입력 처리 기준을
먼저 고정한다.

Phase 7에서도 detection/capture/core, recommendation, verified pipeline은 변경하지
않는다. Win32 Infra 정리는 표시 계층 내부의 책임 분리에 한정한다.

- [x] 현재 `overlay/win32/window.py`, `render.py`, `style.py`, `geometry.py`,
  `view_state.py`의 책임을 다시 목록화한다.
- [x] Overlay 전용 로직과 모든 Win32 창에서 재사용 가능한 Infra 로직을 분리한다.
  - 재사용 후보는 window class 등록, message dispatch, DPI/monitor 좌표,
    saved position, double buffering surface, DirectWrite text renderer,
    theme/style token, hit-test primitive이다.
- [x] 트레이, 설정, 동기화, 디버그, 앱 업데이트 진행 알림의 Win32 전환
  요구사항을 최소 수준으로 정리한다.
  - 트레이: 아이콘, 메뉴, 종료/설정 진입, 알림 표시 가능성
  - 설정: 슬라이더, 체크박스, 선택 항목, 저장/취소 흐름
  - 동기화: 진행 상태, 결과 목록, 에러 메시지, 긴 작업 중 UI 멈춤 방지
  - 디버그: append log, filter, pause/clear, ROI 토글 같은 반복 입력
  - 앱 업데이트: `app_updater`의 "업데이트 중입니다..." 진행 안내와 업데이트
    중 사용자 입력 제한
- [x] Infra 범위를 "작은 Win32 toolkit" 수준으로 제한하고, 범용 layout engine이나
  Qt/React식 widget tree는 만들지 않는다.
- [x] 기존 Win32 overlay가 새 Infra 위에서도 Phase 6 외형과 동작을 유지하는지
  확인한다.
- [x] PyQt6 보조 창은 그대로 유지하고, 실제 이전 대상과 순서는 다음 Phase에서
  별도로 결정한다.

완료 기준:
- [x] Overlay가 새 Infra를 사용해도 Phase 6 smoke 기준에서 후퇴하지 않는다.
- [x] 새 Win32 창을 만들 때 필요한 최소 window/render/text/style 입력 경계가
  문서와 코드에서 드러난다.
- [x] 트레이, 설정, 동기화, 디버그, 앱 업데이트 진행 알림을 고려한 요구사항은
  남기되 실제 이전은 Phase 7 범위 밖으로 둔다.

2026-05-12 시작:
- 현재 `overlay/win32` 책임 목록:
  - `window.py`: 메인 overlay 창 생명주기, 표시/숨김, opacity/confidence,
    위치 저장/이동 콜백, settings hit-test 연결, WM_PAINT/WM_DPICHANGED 등
    overlay 창 메시지 처리, diagnostics orchestration.
  - `render.py`: `Win32OverlayViewState`를 실제 overlay panel/header/tab/row/footer
    도형과 텍스트로 그리는 overlay 전용 renderer. 텍스트 fit/render diagnostics도
    현재는 overlay smoke 기준에 묶여 있다.
  - `style.py`: 색상, 폰트, panel/header/row/footer 좌표, spacing 등 visual token.
    색/폰트 token은 재사용 후보지만 좌표 대부분은 현재 overlay 전용이다.
  - `geometry.py`: DPI scale, overlay 창 크기, 게임 창 기준 위치 계산,
    DPI/monitor diagnostics fixture.
  - `view_state.py`: `OverlayUpdatePayload`를 Win32 renderer 입력 모델로 변환하는
    overlay 전용 adapter.
  - `infra/gui/back_buffer.py`: memory DC/compatible bitmap/BitBlt 기반 double buffering.
    새 Win32 창에서도 재사용 가능한 Infra 성격이다.
  - `infra/gui/text_renderer.py`: Direct2D DCRenderTarget + DirectWrite 텍스트 렌더러.
    새 Win32 창에서도 재사용 가능한 Infra 성격이다.
- 1차 Infra 분리:
  - `infra/gui/dpi.py`: process/window/system DPI helper.
  - `infra/gui/windowing.py`: window class 등록, message loop,
    capture exclusion, monitor rect, ex-style 검사, noactivate show 검증 helper.
  - `infra/gui/input.py`: lparam 좌표 변환, signed word, hit-test 값,
    scaled client rect hit-test helper.
  - `infra/gui`는 추후 별도 프로젝트 승격 가능성을 고려한 임시 최상위 경계이며,
    GUI Infra의 최종 이름은 다음 Phase에서 결정한다.
  - `back_buffer.py`, `text_renderer.py`는 `infra/gui`로 이동했다.
    `back_buffer`는 background color를 호출부에서 받고, `text_renderer`는
    target size와 font height/weight 변환 함수를 호출부에서 주입받아 overlay
    style 의존을 제거했다.
  - `infra/gui/theme.py`는 RGB 생성, 기본 GUI font face, ClearType quality,
    single-line text flag, font height/weight 보정 helper만 가진다. DJMAX 색상
    팔레트와 overlay 좌표는 `overlay/win32/style.py`에 남겨 범용 theme engine으로
    번지는 것을 막는다.
  - `infra/gui/windowing.py`의 `WindowCreateSpec`은 새 Win32 창을 만들 때 필요한
    최소 입력인 class name, title, ex-style, style, position, size만 가진다.
    message dispatch와 렌더링 구조는 각 창 구현이 직접 선택한다.
  - `infra/gui/placement.py`는 `SetWindowPos`/`GetWindowRect` 기반 move/resize와
    수동 위치가 적용된 창은 자동 anchor follow를 멈추는 `ManualPlacement` 정책만
    가진다. 실제 저장소 반영 callback은 각 창 구현이 맡는다.
- 보조 창 전환 요구사항:
  - 트레이: 시스템 트레이 사용 가능 여부 확인, 기본 아이콘/tooltip, 표시/숨김,
    설정, 디버그 토글, 종료 메뉴, 더블클릭 toggle, 추후 알림 표시 가능성이
    필요하다. PyQt6 `QSystemTrayIcon` 대체는 Phase 7 범위 밖이다.
  - 설정: opacity/scale 슬라이더, backend/시스템 선택, 체크박스류 옵션,
    V-Archive 계정 경로 입력, 저장/취소 또는 즉시 저장 기준, 설정 변경 callback이
    필요하다. 기존 `settings_window.py`는 유지한다.
  - 동기화: 계정별 후보 스캔, 진행 중 버튼 비활성화, 결과 목록/행별 상태,
    등록/삭제 후 재스캔, 에러 메시지, 긴 작업 중 UI pump 또는 worker dispatch가
    필요하다. 기존 `sync_window.py`와 worker 동작은 유지한다.
  - 디버그: append log, 최대 줄 수 제한, pause/clear, tag filter, ROI toggle,
    capture exclusion, 다른 thread에서 오는 로그를 UI thread로 넘기는 bridge가
    필요하다. 기존 `debug_window.py`는 유지한다.
  - 앱 업데이트: 일반 보조 진행 창, 닫기 제한, 긴 문구 word wrap, update 중 입력
    제한, pump 가능한 진행 메시지가 필요하다. 기존 `app_updater`의 PyQt6 안내
    창은 유지한다.
  - 실제 이전 순서는 다음 Phase에서 별도로 정하고, Phase 7에서는 overlay 외
    보조 창을 Win32로 교체하지 않는다.

## 진행 중: Win32 보조 창 전환 Phase 8

목표는 Phase 7에서 분리한 Win32 GUI Infra를 실제 보조 창에 처음 적용하되,
PyQt6 보조 창 전체 제거로 범위를 넓히지 않는 것이다. 이 단계에서도
detection/capture/core, recommendation, verified pipeline은 변경하지 않는다.

1. 문제 정의

- 메인 오버레이는 Win32 opt-in 후보까지 올라왔지만, 보조 창은 여전히 PyQt6에
  묶여 있다.
- 보조 창은 트레이, 설정, 동기화, 디버그, 앱 업데이트 진행 알림처럼 성격이
  서로 달라 한 번에 이전하면 검증 범위가 과해진다.

2. 원인 분석

- 트레이/설정/동기화/디버그는 입력, 메뉴, worker bridge, 로그 append 같은
  상태가 많아 첫 Win32 보조 창 대상으로는 리스크가 크다.
- 앱 업데이트 진행 알림은 별도 worker 프로세스에서 문구 표시, 닫기 제한,
  message pump만 필요하므로 verified pipeline과 가장 멀다.

3. 해결 방법

- 옵션 A: 앱 업데이트 진행 알림을 Win32로 먼저 이전한다.
- 옵션 B: 디버그 창을 먼저 이전해 append log와 filter 입력을 검증한다.
- 옵션 C: 설정 창을 먼저 이전해 checkbox/slider/select 입력을 검증한다.
- 옵션 D: 트레이를 먼저 이전해 PyQt6 event loop 의존을 줄인다.

4. 트레이드오프

- 옵션 A는 기능 폭이 작아 첫 검증에 적합하지만, 트레이/설정처럼 복잡한 입력
  UI 검증은 다음 단계로 남는다.
- 옵션 B/C/D는 PyQt6 제거 효과에 더 가까우나 실패 시 사용자가 체감하는 기본
  조작 경로가 흔들릴 수 있다.

5. 추천안

- Phase 8의 첫 대상은 앱 업데이트 진행 알림으로 한다.
- `data.app_updater`의 업데이트 판단, 다운로드, 복사, 재실행 로직은 유지한다.
- PyQt6 기반 `_UpdateStatusReporter`만 Win32 상태창으로 교체한다.
- 검증은 import/py_compile와 Win32 상태창 smoke로 제한하고, 실제 업데이트
  배포 테스트는 별도 로컬 업데이트 체크리스트에서 수행한다.

- [x] 앱 업데이트 진행 알림의 PyQt6 의존 지점을 Win32 상태창 경계로 교체
- [x] Win32 상태창이 close 제한, message pump, 긴 문구 표시를 지원
- [x] 업데이트 worker 실패/성공 문구 갱신 경로가 기존과 같은 호출 구조 유지
- [x] import/py_compile 및 상태창 smoke test 통과
- [x] 트레이, 설정, 동기화, 디버그 창은 그대로 PyQt6에 남김

2026-05-12 진행:
- `infra/gui/status_window.py`에 worker 진행 안내용 Win32 상태창을 추가했다.
  작은 caption window와 native static label만 사용하며, 닫기 요청은
  무시하고 caller가 `close()`로 닫는다.
- `data/app_updater.py`의 `_UpdateStatusReporter`는 PyQt6 `QApplication/QWidget`
  경로 대신 `Win32StatusWindow`를 사용한다. 업데이트 판단, 다운로드, 압축 해제,
  파일 복사, 재실행 로직은 변경하지 않았다.
- `test/win32_status_window_smoke.py`를 추가해 import와 diagnostics를 확인한다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile data\app_updater.py infra\gui\status_window.py test\win32_status_window_smoke.py
.\.venv_build\Scripts\python.exe test\win32_status_window_smoke.py --import-only
.\.venv_build\Scripts\python.exe test\win32_status_window_smoke.py --diagnostics
```

결과:

```text
Win32 status window import ok
hwnd_created=True
label_created=True
topmost=False
close_disabled=True
```

주의:
- 실제 표시 루프용 `--show`는 GUI 창을 띄우는 검증이므로 이번 자동 검증에서는
  실행하지 않았다.
- `--show` 육안 확인에서 native static label 영역이 흰색으로 남는 문제가 있어,
  Win32 상태창의 `WM_ERASEBKGND`와 `WM_CTLCOLORSTATIC`에서 창/label 배경을 같은
  system face 색으로 칠하도록 보정했다.
- 2026-05-12 정책 보정: `WS_EX_TOPMOST`는 메인 overlay 창에만 사용한다. 업데이트
  상태창, 디버그 창, 설정 창, 동기화 창은 일반 tool window로 표시한다.

## 진행 중: Win32 디버그 창 전환 Phase 9

목표는 PyQt6 디버그 창을 즉시 제거하지 않고, Phase 7 Infra 위에 Win32 디버그
창 후보를 opt-in으로 연결해 append log, pause/clear, tag filter, ROI toggle의
최소 동작을 확인하는 것이다. 이 단계에서도 detection/capture/core,
recommendation, verified pipeline은 변경하지 않는다.

1. 문제 정의

- 기존 `overlay/debug_window.py`는 PyQt6 `QTextEdit`, `QPushButton`,
  `QCheckBox`, Qt signal bridge에 의존한다.
- 디버그 창은 runtime log와 ROI overlay toggle을 다루므로, 단순 상태창보다
  입력과 상태 관리가 많다.

2. 원인 분석

- 다른 thread에서 들어오는 로그는 UI thread로 넘겨야 하며, 현재는 PyQt signal
  bridge가 이 역할을 한다.
- Win32 창을 바로 기본값으로 바꾸면 로그 표시, 필터, ROI toggle 사용성이 한 번에
  흔들릴 수 있다.

3. 해결 방법

- 옵션 A: Qt signal bridge는 유지하고, 표시 창만 Win32로 opt-in 연결한다.
- 옵션 B: thread-safe bridge까지 Win32 message post 기반으로 전환한다.
- 옵션 C: 기존 PyQt6 디버그 창을 유지하고 다음 보조 창으로 넘어간다.

4. 트레이드오프

- 옵션 A는 PyQt6 의존을 완전히 제거하지는 못하지만, 디버그 창의 native window
  동작을 가장 작게 검증할 수 있다.
- 옵션 B는 더 완전한 전환이지만 message ownership과 thread safety 검증 범위가
  커진다.
- 옵션 C는 안정적이지만 Phase 7 Infra를 실제 입력 창에 적용하지 못한다.

5. 추천안

- Phase 9의 첫 조각은 옵션 A로 진행한다.
- `overlay.main_backend = "win32"`일 때만 Win32 디버그 창을 사용한다.
- 별도 `debug_window.backend` 설정은 두지 않고, 메인 오버레이 backend 값을
  모든 Win32 보조 창 전환 기준으로 재사용한다.

- [x] `overlay/win32/debug_window.py`에 Win32 디버그 창 후보 추가
- [x] append log, 최대 줄 수 trim, pause, clear, tag filter, ROI toggle callback
  경계 추가
- [x] `overlay/debug_window.py`의 `DebugController`에서 Win32 opt-in backend 연결
- [x] `overlay.main_backend` 기준으로 Win32 디버그 창 opt-in 여부 결정
- [x] import/py_compile 및 Win32 debug smoke 통과
- [ ] 실제 앱에서 `overlay.main_backend=win32` 기준 트레이 디버그 토글 육안 확인

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\debug_window.py overlay\win32\debug_window.py test\win32_debug_window_smoke.py settings.py
.\.venv_build\Scripts\python.exe test\win32_debug_window_smoke.py --import-only
.\.venv_build\Scripts\python.exe test\win32_debug_window_smoke.py --diagnostics
.\.venv_build\Scripts\python.exe test\win32_debug_window_smoke.py --append-check
.\.venv_build\Scripts\python.exe -c "from settings import SETTINGS; SETTINGS['overlay']['main_backend']='win32'; from overlay.debug_window import DebugController; c=DebugController(); w=c.create_window(); c.log('[Overlay] controller smoke'); print(type(w).__name__)"
```

결과:

```text
Win32 debug window import ok
hwnd_created=True
edit_created=True
capture_excluded=True
filter_count=5
max_lines=500
append_ok=True
Win32DebugWindow
```

주의:
- 이번 단계의 Win32 디버그 창은 native control 기반이라 PyQt6의 tag별 inline
  색상 표현은 아직 복제하지 않는다.
- thread-safe log bridge는 기존 PyQt signal을 재사용한다. PyQt6 전체 제거를
  위한 bridge 교체는 별도 단계에서 판단한다.
- Win32 native control은 기본 배경/텍스트 영역이 흰색 또는 system 기본색으로
  따로 칠해질 수 있다. 새 보조 창을 만들 때는 `WM_ERASEBKGND`,
  `WM_CTLCOLORSTATIC`, `WM_CTLCOLOREDIT`, 필요 시 `WM_CTLCOLORBTN`까지 확인해
  창 배경과 텍스트 control 배경이 따로 튀지 않도록 먼저 맞춘다.
- 보조 창은 게임 화면 위에 직접 얹히는 오버레이가 아니므로 어두운 overlay
  palette를 그대로 쓰지 않는다. 설정/동기화/디버그/업데이트 같은 보조 창은
  밝은 system dialog 톤을 기본으로 삼고, 로그/입력 영역만 필요한 만큼 구분한다.
- Win32 native control은 font face, weight, DPI, 한글 렌더링에 따라 실제 text
  extent가 달라진다. 버튼, 체크박스, label 폭은 고정 숫자만 믿지 말고
  `GetTextExtentPoint32` 같은 font metrics 기반으로 최소 폭을 계산해 짤림을
  먼저 막는다.

2026-05-12 follow-up:
- `--show` 육안 확인에서 디버그 창 배경과 로그 텍스트 영역이 서로 튀는 문제가
  있어, Win32 디버그 창의 window/static/button/edit control 배경과 글자색을
  명시적으로 칠하도록 보정했다.
- 보조 창은 밝은 톤을 기본으로 한다는 기준에 맞춰 Win32 디버그 창 배경을 밝은
  system dialog 계열로 바꾸고, 로그 영역만 흰색으로 분리했다.
- `ROI 표시 ON` 같은 한글 상태 텍스트가 잘리지 않도록 버튼/체크박스 폭을 현재
  Win32 font의 text extent 기준으로 계산하도록 바꿨다.

## 예정: Win32 설정 창 전환 Phase 10

목표는 사용자가 자주 보는 설정 창을 Win32로 옮기기 전에 기존 PyQt6 설정 창의
동작 계약과 표시 기준을 먼저 고정하고, 작은 절편으로 검증하는 것이다. 설정 창은
메인 오버레이보다 직접 조작이 많으므로 디버그 창보다 더 보수적으로 진행한다.

Phase 10에서도 detection/capture/core, recommendation, verified pipeline은
변경하지 않는다. `overlay.main_backend=win32`를 보조 창 전환 기준으로 쓰되,
Win32 설정 창은 충분히 검증되기 전까지 실제 앱 기본 경로에 연결하지 않는다.

1. 문제 정의

- 기존 `SettingsWindow`는 UI, V-Archive, System 탭을 한 창에서 제공한다.
- 사용자가 설정 창에서 주로 보는 항목은 오버레이 투명도/크기, 자동 업데이트,
  V-Archive 계정/ID/동기화 진입이다.
- 설정 창은 자주 보는 보조 창이므로 어두운 overlay palette나 임시 native control
  배치가 그대로 노출되면 완성도가 낮게 보인다.

2. 원인 분석

- 기존 PyQt 설정 창은 다음 signal/callback 계약을 가진다.
  - `opacity_changed(float)`
  - `scale_changed(float)`
  - `fetch_varchive_requested(steam_id, v_id, button)`
  - `sync_requested(steam_id, persona_name, account_path)`
  - `account_file_changed(steam_id, account_path)`
  - `refresh_current_steam_indicator()`
- V-Archive 탭은 Steam 세션 목록, 현재 계정 강조, 다른 계정 펼침, account.txt
  경로 선택, 동기화 후보 창 진입까지 포함해 UI/System 탭보다 리스크가 크다.
- Win32 native control은 font metrics, DPI, 한글 폭, 버튼/check 상태 텍스트에 따라
  layout이 쉽게 깨질 수 있다.

3. 해결 방법

- 옵션 A: UI/System 탭만 먼저 Win32 후보로 구현하고 실제 연결은 보류한다.
- 옵션 B: UI/System/V-Archive 전체를 한 번에 Win32로 구현한 뒤 opt-in 연결한다.
- 옵션 C: 기존 PyQt 설정 창을 유지하고 설정 창 전환은 뒤로 미룬다.

4. 트레이드오프

- 옵션 A는 사용자가 가장 자주 보는 기본 설정 흐름을 먼저 다듬을 수 있지만,
  V-Archive 전환은 다음 절편으로 남는다.
- 옵션 B는 한 번에 완성도가 높아 보일 수 있으나, account file picker와 sync
  진입까지 같이 검증해야 해 실패 범위가 크다.
- 옵션 C는 안정적이지만 Win32 보조 창 전환 진도가 멈춘다.

5. 추천안

- Phase 10은 옵션 A로 시작한다.
- 첫 절편은 Win32 설정 창 후보의 UI/System 탭, 밝은 보조창 palette, font metrics
  기반 adaptive layout, 저장/callback 계약 smoke까지로 제한한다.
- V-Archive 탭은 현재 PyQt 동작을 더 자세히 대조한 뒤 다음 절편에서 구현한다.
- 실제 `OverlayController` 연결은 Win32 설정 창이 UI/System/V-Archive 계약을 모두
  만족한 뒤 진행한다.

- [x] 기존 PyQt `settings_window.py` / `settings_varchive.py`의 표시 항목과
  signal 계약 목록화
- [x] Win32 설정 창 후보는 밝은 system dialog 톤으로 설계
- [x] slider, preset button, checkbox, tab/button width는 font metrics와 DPI 기준으로
  adaptive하게 배치
- [x] UI 탭: 오버레이 투명도 저장, `opacity_changed` callback smoke
- [x] UI 탭: 오버레이 크기 preset 저장, `scale_changed` callback smoke
- [x] System 탭: 자동 업데이트 checkbox 저장, 현재 버전 표시
- [x] V-Archive 탭 구현 전에는 실제 앱 설정 경로에 연결하지 않음
- [x] Win32 settings smoke와 육안 `--show` 체크 통과 후 연결 여부 판단

2026-05-12 진행:
- `overlay/win32/settings_window.py`에 Win32 설정 창 후보를 추가했다.
  첫 절편은 UI/System 탭에 한정하고, V-Archive 탭은 placeholder만 둔다.
- 보조 창 기준에 맞춰 밝은 system dialog 톤을 사용하고, tab/button/checkbox 폭은
  현재 Win32 font의 `GetTextExtentPoint32` 결과를 바탕으로 계산한다.
- `persist=False` smoke 모드를 두어 자동 검증이 `settings.user.json`을 쓰지 않게
  했다. 실제 연결 시에는 기본 `persist=True`로 `save_settings()` 경로를 사용한다.
- `set_opacity_callback()` / `set_scale_callback()`으로 PyQt signal 계약의 첫 부분을
  Win32 후보에서도 검증할 수 있게 했다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\win32\settings_window.py test\win32_settings_window_smoke.py
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --import-only
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --diagnostics
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --callback-check
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --show --duration-ms 4000
```

결과:

```text
Win32 settings window import ok
hwnd_created=True
trackbar_created=True
scale_button_count=4
system_checkbox_created=True
current_tab=ui
opacity=0.7
scale=1.25
opacity_callback=0.7
scale_callback=1.25
show_ok=True
```

주의:
- `OverlayController` 연결은 `overlay.main_backend=win32` opt-in 경로에만 적용한다.
- V-Archive 탭은 현재 Steam 계정 1개 기준으로 구현했다. 여러 계정 펼침과
  account.txt 파일 선택 dialog는 다음 절편으로 남긴다.
- `--show` 확인 중 제목/닫기 버튼이 탭별 control list에 섞일 수 있는 구조를
  발견해, header control은 공통 list로 분리했다.
- native title bar가 이미 있으므로 body 내부의 별도 제목/닫기 header는 제거했다.
  설정 창 body는 탭과 설정 항목만 담는다.

2026-05-12 V-Archive 절편:
- Win32 설정 창 후보에 현재 Steam 계정 1개 기준 V-Archive 탭을 추가했다.
- 지원 범위는 시작 시 자동 갱신 checkbox, V-Archive ID 입력, 4B/5B/6B/8B/All
  fetch callback, account path 입력, 동기화 후보 callback이다.
- smoke에서는 가짜 `SteamSession`을 주입하고 `persist=False`를 사용해 실제
  Steam 파일이나 `settings.user.json`에 의존하지 않는다.
- 여러 Steam 계정 펼침과 account.txt 파일 선택 dialog는 아직 다음 절편으로
  남긴다.

추가 검증:

```text
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --varchive-check
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --show --tab varchive --duration-ms 4000
```

결과:

```text
fetch=('76561198000000000', 'test-v-id', 4)
sync=('76561198000000000', 'Smoke User', 'C:\tmp\account.txt')
account=('76561198000000000', 'C:\tmp\account.txt')
show_ok=True
```

2026-05-12 연결 절편:
- `overlay.main_backend=win32`일 때 `OverlayController`가 `Win32SettingsWindow`를
  생성하도록 연결했다.
- PyQt6 경로는 기존 `SettingsWindow`와 Qt signal 연결을 유지한다.
- Win32 경로는 `set_opacity_callback`, `set_scale_callback`,
  `set_fetch_varchive_callback`, `set_sync_callback`,
  `set_account_file_callback`으로 기존 설정창 계약을 연결한다.
- Win32 설정창의 `refresh_current_steam_indicator()`는 현재 no-op으로 둔다.
  전체 세션 목록 refresh는 여러 계정 절편에서 다시 구현한다.

추가 검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\controller.py overlay\win32\settings_window.py test\win32_settings_window_smoke.py
.\.venv_build\Scripts\python.exe -c "from settings import SETTINGS; SETTINGS['overlay']['main_backend']='win32'; from overlay.controller import OverlayController; c=OverlayController(None, None); print(type(c._create_settings_window()).__name__)"
```

결과:

```text
[Overlay] 설정 창 backend: win32
Win32SettingsWindow
```

현재 판단:
- 사용자가 `overlay.main_backend=win32` 설정창 경로를 실제로 확인했고, 기본 동작은
  잘 된다고 판단했다.
- Phase 10은 완전 종료가 아니라, 설정창 후속 절편을 남긴 상태로 다음 작업을
  이어간다.

다음 작업:
- [x] 여러 Steam 계정 표시와 다른 계정 펼침/접기 구현
- [x] 현재 Steam 계정 변경 시 Win32 설정창의 V-Archive 탭 refresh 구현
- [x] `account.txt` 파일 선택 dialog 구현
- [x] V-Archive account path 변경 후 `account_file_changed` 계약을 실제 앱 경로에서
  재확인
- [ ] Win32 설정창을 실제 앱에서 트레이 설정 메뉴, overlay 설정 버튼, scale 변경,
  opacity 변경, V-Archive fetch/sync 진입까지 end-to-end로 확인
- [x] 설정창 파일이 500라인에 가까워지면 V-Archive 탭 구현을 별도 module로 분리

2026-05-12 마무리 절편:
- Win32 설정창 공용 native control helper를 `overlay/win32/settings_common.py`로
  분리해 설정창 본문 파일을 500라인 이하로 유지했다.
- V-Archive 탭에서 현재 계정과 다른 계정을 함께 만들고, 다른 계정 영역은
  "다른 계정 보기/숨기기" 버튼으로 접고 펼치게 했다.
- account path 행에 `찾기` 버튼을 추가해 `GetOpenFileNameW` 기반 account.txt
  선택 dialog를 열 수 있게 했다.
- `refresh_current_steam_indicator()`는 현재 창을 재생성해 Steam session 순서와
  V-Archive 탭 내용을 다시 구성한다.
- smoke에서는 두 개의 가짜 Steam session을 주입해 여러 계정 표시와 펼침 상태를
  실제 Steam 설치 없이 검증한다.

추가 검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\win32\settings_common.py overlay\win32\settings_window.py test\win32_settings_window_smoke.py
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --diagnostics
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --callback-check
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --varchive-check
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --multi-account-check
.\.venv_build\Scripts\python.exe test\win32_settings_window_smoke.py --show --tab varchive --duration-ms 4000
```

결과:

```text
varchive_session_count=2
other_session_count=10
others_visible=False
opacity_callback=0.7
scale_callback=1.25
fetch=('76561198000000000', 'test-v-id', 4)
sync=('76561198000000000', 'Smoke User', 'C:\tmp\account.txt')
account=('76561198000000000', 'C:\tmp\account.txt')
before_visible=False
after_visible=True
show_ok=True
```

완료 기준:
- [x] `overlay.main_backend=win32`에서 설정 창이 Win32 후보로 열린다.
- [x] UI/System/V-Archive 기본 계약이 smoke에서 통과한다.
- [x] 보조 창 밝은 palette와 font metrics 기반 width 계산 원칙을 따른다.
- [x] 설정창 관련 파일은 500라인 제한 안에 남아 있다.

## 예정: Win32 동기화 창 전환 Phase 11

목표는 PyQt6 `SyncWindow`를 Win32 후보로 옮기되, 긴 작업 중 UI 멈춤 방지와
결과 목록의 조작성/가독성을 먼저 보장하는 것이다. 동기화 창은 V-Archive API
등록/삭제와 local record 삭제를 실행하므로, 설정창보다 검증 범위를 더 엄격하게
나눈다.

Phase 11에서도 detection/capture/core, recommendation, verified pipeline은
변경하지 않는다. `overlay.main_backend=win32`를 기준으로 하되, 실제 API 등록/삭제
경로는 smoke와 no-op callback 확인 후에만 연결한다.

1. 문제 정의

- 기존 PyQt `SyncWindow`는 후보 스캔 worker, 결과 row list, 등록/삭제 action,
  재스캔 queue, 계정 유무에 따른 버튼 활성화를 한 창에서 처리한다.
- Win32 native control로 옮기면 긴 작업 중 message pump, row 갱신, 버튼 상태,
  scroll/list 표현을 직접 책임져야 한다.
- 동기화 창은 실수로 실제 V-Archive 업로드나 local record 삭제를 실행할 수 있어
  테스트와 실제 연결을 분리해야 한다.

2. 원인 분석

- 현재 계약은 다음 요소로 구성된다.
  - `show_window(steam_id, persona_name, account_path)`
  - `set_account(steam_id, account)`
  - account가 있을 때만 scan/upload 버튼 활성화
  - `_start_scan()`이 worker thread에서 `build_candidates()` 실행
  - scan 결과를 row list로 렌더링
  - row별 `등록`, `삭제`, 상태 표시
  - action 완료 후 재스캔
- UI는 밝은 보조창 기준으로 재설계해야 하며, 기존 어두운 overlay palette를
  그대로 쓰지 않는다.
- 후보 row는 곡명, 난이도, 모드, Overmax/V-Archive 값, 차이 사유, action 버튼이
  한 줄에 들어가므로 font metrics 기반 column width와 elide가 필요하다.

3. 해결 방법

- 옵션 A: scan/action 로직은 기존 `SyncActionsMixin` 흐름을 유지하고, Win32 창은
  표시/버튼/상태 갱신만 담당한다.
- 옵션 B: scan/action까지 Win32 전용 controller로 재작성한다.
- 옵션 C: PyQt 동기화 창을 유지하고 다른 보조 창 전환을 먼저 진행한다.

4. 트레이드오프

- 옵션 A는 기존 검증된 business/action 흐름을 유지하지만, PyQt signal bridge와
  같은 thread-safe event 전달 방식을 Win32 message/post 방식으로 별도 구현해야
  한다.
- 옵션 B는 장기적으로 더 깔끔할 수 있으나 업로드/삭제/재스캔 경로를 한 번에
  건드려 위험하다.
- 옵션 C는 안정적이지만 PyQt6 보조 창 제거 진도가 멈춘다.

5. 추천안

- Phase 11은 옵션 A로 시작한다.
- 첫 절편은 실제 API/DB mutation 없이 Win32 동기화 창 shell, account 상태,
  refresh 버튼, empty/loading/status text, sample candidate rows까지 구현한다.
- 두 번째 절편에서 worker 결과를 Win32 UI thread로 전달하는 post-message bridge를
  추가한다.
- 세 번째 절편에서 실제 `build_candidates()`, upload/delete callback, action 후
  재스캔을 연결한다.
- 실제 앱 연결은 sample row smoke, no-account smoke, account-present smoke,
  worker completion smoke가 모두 통과한 뒤에만 진행한다.

다음 작업:
- [x] Win32 동기화 창 후보 파일을 500라인 이하로 설계하고 row rendering은 별도
  module로 분리
- [x] 밝은 보조창 palette와 font metrics 기반 column layout 적용
- [x] `show_window(steam_id, persona_name, account_path)` / `set_account()` 계약 구현
- [x] no-account 상태: refresh 버튼 비활성화, 안내 문구 표시
- [x] account-present 상태: refresh 버튼 활성화, sample candidate rows 렌더링
- [x] row별 등록/삭제 버튼은 첫 절편에서 no-op callback smoke만 수행
- [x] worker thread 결과를 Win32 UI thread로 넘기는 post-message bridge 설계
- [x] 실제 `build_candidates()` 연결 전 sample fixture smoke 추가
- [x] 실제 upload/delete 연결 전 API mutation이 일어나지 않는 dry-run smoke 추가
- [x] `OverlayController`는 Win32 opt-in 경로에서 동기화 창 골격으로 연결

2026-05-12 골격/연결 절편:
- `overlay/win32/sync_window.py`에 Win32 동기화 창 후보 shell을 추가했다.
  지원 범위는 title/status/count, no-account refresh disabled, account-present
  refresh enabled, sample 후보 표시, 등록/삭제 no-op 안내까지로 제한한다.
- 후보 row native control 생성은 `overlay/win32/sync_row.py`로 분리해 동기화 창
  본문 파일을 500라인 이하로 유지했다.
- `overlay.main_backend=win32`일 때 `OverlayController`가 `Win32SyncWindow`를
  생성하도록 연결했다. PyQt6 기본 경로는 기존 `SyncWindow`를 유지한다.
- 실제 `build_candidates()`, upload/delete, action 후 rescan은 아직 연결하지 않는다.

검증:

```text
.\.venv_build\Scripts\python.exe -m py_compile overlay\controller.py overlay\win32\sync_window.py overlay\win32\sync_row.py test\win32_sync_window_smoke.py
.\.venv_build\Scripts\python.exe test\win32_sync_window_smoke.py --import-only
.\.venv_build\Scripts\python.exe test\win32_sync_window_smoke.py --diagnostics
.\.venv_build\Scripts\python.exe test\win32_sync_window_smoke.py --account-check
.\.venv_build\Scripts\python.exe -c "from settings import SETTINGS; SETTINGS['overlay']['main_backend']='win32'; from overlay.controller import OverlayController; c=OverlayController(None, None); print(type(c._create_sync_window()).__name__)"
```

결과:
 
 ```text
 Win32 sync window import ok
 hwnd_created=True
 refresh_enabled=False
 row_count=2
 status_text=sample 후보를 표시했습니다. 등록/삭제는 아직 no-op입니다.
 before_refresh_enabled=False
 after_refresh_enabled=True
 [Overlay] 동기화 창 backend: win32
 Win32SyncWindow
 ```

2026-05-12 Bridge/Worker 절편:
- `overlay/win32/sync_bridge.py`에 thread-safe `PostMessage` bridge를 추가했다.
- `Win32SyncWindow`가 `WM_SYNC_SCAN_FINISHED`, `WM_SYNC_ROW_STATUS`,
  `WM_SYNC_ACTION_FINISHED`를 처리해 background worker 결과를 UI thread로
  안전하게 받도록 연결했다.
- `Win32SyncWindow`에 `_start_scan`, `_upload_worker`, `_delete_worker`를
  추가해 실제 `build_candidates`, `upload_score`, `record_manager.delete`
  경로를 Win32 UI thread-safe하게 연결했다.
- `overlay/win32/sync_row.py`에 individual status label을 추가해 행별
  진행 상태("업로드 중...", "완료", "실패" 등)를 표시하게 했다.

검증:

```text
.\.venv_build\Scripts\python.exe test\win32_sync_window_smoke.py --bridge-check
```

결과:

```text
bridge_row_count=2
bridge_ok=True
```

# Phase 12: Win32 UI Polish & Layout Infra

Win32 UI의 하드코딩된 레이아웃을 개선하고, 심미적 완성도를 높인다.

- [ ] Win32 UI Infra (`settings_common.py`) 고도화
    - [ ] `LayoutContext` 도입 (Spacing, Alignment 관리)
    - [ ] 시각적 그룹화를 위한 `Card`/`Divider` 컴포넌트 추가
- [ ] Win32 동기화 창 (`Win32SyncWindow`) 디자인 개선
    - [ ] Sticky Header/Footer 레이아웃 적용
    - [ ] 동기화 후보 Row 컬럼 정렬 및 시각적 대비 강화
- [ ] Win32 설정 창 (`Win32SettingsWindow`) 디자인 개선
    - [ ] 카드 섹션 기반 레이아웃으로 개편
    - [ ] 탭 시스템 시각적 피드백 강화
- [ ] 통합 검증
    - [ ] DPI 스케일링 대응 확인
    - [ ] 기존 오버레이 기능 영향 확인 (Zero Regression)


## 검증 기준

실제 이미지셋이 준비되면 다음 기준을 통과해야 한다.

```text
candidate_expected_top1=795/795
candidate_matches_cv2_top1=795/795
```

2026-05-11 기준 `test/jackets` 795개 이미지에서 Rust backend는 위 기준을 통과했다.
다만 HOG 값이 byte-level로 완전히 동일하지는 않으므로, 프로덕션 연결 전에는
stored HOG cosine worst case를 함께 확인한다.

```text
candidate_vs_stored_hog_cosine min=0.949237 mean=0.996954 max=0.998480
```

## 제약

- 기존 verified pipeline은 변경하지 않는다.
- 선곡 화면 전용 로직은 정확도를 우선하되, 인게임 성능 영향은 피한다.
- Rust backend는 검증 스크립트에서 충분히 확인된 뒤 프로덕션 검색 경로에 연결한다.
