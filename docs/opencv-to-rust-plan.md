# OpenCV to Rust Migration Plan

이 문서는 Overmax에서 OpenCV 의존성을 제거하기 위한 단계별 작업 계획이다.
목표는 빌드 결과물 크기를 줄이되, 기존 화면 캡처 기반 verified pipeline을 유지하는 것이다.

## 목표

- 런타임 `cv2` import 제거
- `opencv-python-headless` 배포 의존성 제거
- 곡 인식 top-1 결과 유지
- OCR/상태 전이 로직의 안정성 유지

## 현재 OpenCV 사용 지점

### 런타임 핵심 경로

- `detection/image_db.py`
  - BGR/BGRA to grayscale
  - area resize
  - pHash/dHash/aHash
  - DCT
  - HOG
- `capture/helpers.py`
  - BGRA thumbnail grayscale 변환
  - 32x32 area resize
- `detection/ocr_wrapper.py`
  - BGRA 3x upscale
  - grayscale 변환
  - Otsu threshold
  - border padding
  - BMP encoding
- `runtime_patch.py`
  - frozen 환경에서 cv2 Qt plugin path 보정

### 개발/검증 도구

- `detection/image_db_cli.py`
  - 이미지 파일 로드
- `test/hog_compat_check.py`
  - OpenCV 기준 HOG와 비교
- `test/rust_hog_check.py`
  - 이미지 파일 로드 및 기존 후보 비교
- `test/overlay_tester.py`
  - 비디오 재생, UI 표시, 저장, 디버그 분석

개발 도구는 런타임 빌드 크기에 직접 영향을 주지 않으므로 마지막 단계에서 분리한다.

## 단계 계획

### Phase 1: 이미지 매칭 핵심 경로 Rust 이전

- [x] Rust `overmax_cv`에 이미지 feature API 추가
  - 입력: raw image bytes, width, height, channels
  - 출력: `phash`, `dhash`, `ahash`, `hog`
- [x] Rust에 grayscale 변환, area resize, DCT, HOG 구현
- [x] `detection/image_db.py`에서 `cv2` 제거
- [x] `capture/helpers.py`의 thumbnail 생성을 Rust로 이전
- [x] 검증
  - `test/jackets` 795개 top-1 유지
  - 기존 stored HOG cosine worst case 기록
  - 앱 import smoke test

Phase 1 결과:

```text
candidate_expected_top1=795/795
candidate_matches_cv2_top1=795/795
candidate_vs_stored_hog_cosine min=0.949237 mean=0.996954 max=0.998480
```

이 단계 이후 이미지 매칭 런타임 경로의 `cv2` 참조는 제거되었다.
`detection/image_db_cli.py`는 개발용 CLI로 Phase 4에서 분리한다.

### Phase 2: OCR 전처리 Rust 이전

- [x] Rust `overmax_cv`에 OCR preprocess API 추가
  - 입력: BGRA raw bytes, width, height
  - 출력: BMP bytes
- [x] Rust에 grayscale, 3x resize, Otsu threshold, padding, BMP encoding 구현
- [x] `detection/ocr_wrapper.py`에서 `cv2` 제거
- [x] 검증
  - OCR engine import smoke test
  - Rust preprocess가 `BM` BMP bytes를 반환하는지 확인
  - 정적 ROI 샘플이 있으면 기존 OCR 결과와 비교

Phase 2 결과:

```text
ocr_preprocess_bgra(...).startswith(b"BM") == True
detection.ocr_wrapper import smoke test 통과
```

### Phase 3: 빌드/의존성 정리

- [x] `runtime_patch.py`의 `patch_cv2()` 제거
- [x] `overmax.spec` hiddenimports에서 `cv2` 제거
- [x] `requirements.txt`에서 `opencv-python-headless` 제거
- [x] OpenCV 기반 검증/개발 도구는 `requirements-dev.txt`로 분리
- [x] `detection/image_db.py`에서 개발 CLI 진입점 분리
- [x] PyInstaller 빌드 후 결과물 확인
- [x] 앱 import smoke test
- 실제 앱 실행 smoke test

Phase 3 결과:

```text
dist/overmax 내 cv2/opencv 파일 없음
runtime import smoke test 통과
dist/overmax total bytes=137515105
```

기존 `dist/overmax`는 작업 시작 시 `92779606` bytes였으나, 이전 산출물이 어떤
spec/cache 상태에서 만들어졌는지 확정할 수 없어 동일 조건 크기 비교로 보지는 않는다.

### Phase 4: 개발 도구 분리

- `test/overlay_tester.py`는 OpenCV 기반 개발 도구로 유지하거나 별도 extra 의존성으로 분리
- `test/hog_compat_check.py`는 OpenCV 기준 비교용이므로 배포 의존성과 분리
- `detection/image_db_cli.py` 이미지 로드는 Rust 또는 별도 lightweight 이미지 로더로 이전 검토

## 통과 기준

Phase 1 완료 기준:

```text
candidate_expected_top1=795/795
candidate_matches_cv2_top1=795/795
```

Phase 3 완료 기준:

- 앱 런타임 경로에서 `import cv2` 없음
- PyInstaller hiddenimports에 `cv2` 없음
- `requirements.txt` 런타임 의존성에 `opencv-python-headless` 없음

## 주의사항

- verified pipeline은 변경하지 않는다.
- `rate == 0.0`과 `rate is None` 구분 같은 기존 invariant는 건드리지 않는다.
- 인게임 중 실행되는 경로는 정확도보다 성능을 우선한다.
- OpenCV 기준 검증 코드는 배포 경로와 분리하되, 비교 기준으로는 유지할 수 있다.
