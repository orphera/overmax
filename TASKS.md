# TASKS

Overmax의 현재 작업은 Python 기반 verified pipeline을 유지하면서,
OpenCV 런타임 의존성 제거 이후 Qt/PyQt6 영역의 배포 크기와 구조를
단계적으로 정리하는 것이다.

OpenCV 제거 상세 기록은 `docs/opencv-to-rust-plan.md`를 따른다.
Qt 정리 상세 계획은 `docs/qt-runtime-plan.md`를 따른다.

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

## 다음 단계: Qt 대체 UI spike 여부 결정

- [ ] Phase 0~3 결과 기준으로 PyQt6 대체 spike 필요성 판단
- [ ] 대체 UI 후보를 검토하더라도 verified pipeline 변경 금지

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
