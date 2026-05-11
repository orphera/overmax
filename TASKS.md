# TASKS

Overmax의 현재 작업은 Python 기반 verified pipeline을 유지하면서,
OpenCV 런타임 의존성 제거와 Qt/PyQt6 영역의 기준 정리를 마친 뒤,
Win32 직접 오버레이를 프로덕션 메인 오버레이 후보로 단계적으로 연결하는 것이다.

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
- [ ] 기본 경로는 기존 PyQt6로 유지하고 Win32는 opt-in으로 시작
- [ ] overlay 표시/숨김, 위치 이동/저장, opacity, scale 반영
- [ ] game focus를 방해하지 않는 noactivate/topmost 동작 확인
- [ ] 캡처 제외 실패 시 verified pipeline에는 영향 없이 UI만 보수적으로 동작
- [ ] 트레이, 설정, 동기화, 디버그 창은 PyQt6 유지
- [ ] Win32 경로에서도 import smoke 및 실제 앱 smoke test 통과
- [ ] PyInstaller 빌드 후 Win32 opt-in 경로 import/실행 확인
- [ ] 배포 전 기본값 전환 여부를 별도 판단

Phase 5에서는 detection/capture/core와 recommendation 로직을 변경하지 않는다.
Win32 경로가 실패해도 verified state commit과 기록 저장 흐름은 영향을 받지
않아야 한다.

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
