# TASKS

Overmax의 현재 작업은 Python 기반 verified pipeline을 기준 구현으로 유지하면서,
Rust 네이티브 앱으로 단계적으로 전환하는 것이다.

OpenCV 제거 이력은 `docs/opencv-to-rust-plan.md`, 전체 Rust 전환 계획은
`docs/rust-native-port-plan.md`를 따른다.

## 현재 단계: Rust 전체 포팅 준비

- [x] `CONTEXT.md`의 Python-only 제약을 Rust 전환 기준으로 갱신
- [x] Rust 앱 기준 스택을 `egui/winit`으로 명시
- [x] 기존 파일 호환 정책 명시
- [x] verified pipeline 불변 조건을 Rust 포팅 문서에 복사
- [x] 루트 Rust workspace 골격 추가
- [x] `overmax-core`, `overmax-data`, `overmax-app` crate 경계 정의
- [x] 기존 `rust/overmax_cv`를 workspace member로 유지
- [x] 비-Windows 실행 시 명확한 unsupported 에러를 반환하는 app skeleton 추가
- [x] Python 기준 fixture와 Rust 결과 비교 harness 작성

## 다음 단계: Core State 모델 포팅

- [x] `GameSessionState` Rust 타입 시작점 추가
- [x] `rate == 0.0`과 `rate is None` 의미 차이를 `Option<f32>`로 보존
- [x] `song_id == 0`을 유효 ID로 유지하고 없음은 `Option`으로 표현
- [x] Python 기준 fixture를 만들어 Rust 결과와 비교

## 다음 단계: 설정 시스템 포팅

- [ ] `settings.json` + `settings.user.json` 우선순위 재현
- [ ] delta save 정책 재현
- [ ] normalize/clamp/snap 규칙 재현
- [ ] 오버레이 위치, 스케일, 투명도, V-Archive user_map 파일 형식 호환 유지

## 다음 단계: 데이터 계층 포팅

- [ ] V-Archive songs JSON 로드, 인덱싱, exact/fuzzy 검색 Rust 구현
- [ ] `record.db` SQLite schema 호환 유지
- [ ] `(steam_id, song_id, button_mode, difficulty)` primary key 유지
- [ ] floor 기반 추천 정렬 유지
- [ ] Python 추천 결과와 Rust 추천 결과 golden test 작성

## 다음 단계: 런타임/배포 포팅

- [ ] Image DB 검색 경로 Rust 앱 내부 API로 전환
- [ ] Window tracking / capture Rust 구현
- [ ] Detection pipeline Rust 구현
- [ ] Windows OCR 연동 Rust 구현
- [ ] `egui/winit` 오버레이 UI 구현
- [ ] 설정 / 동기화 / 디버그 창 구현
- [ ] Steam session / hotkey / single instance 구현
- [ ] Updater / packaging Rust 구현
- [ ] 병렬 런타임 검증 harness 작성
- [ ] Rust 앱 전환 및 Python 제거 절차 진행

## 완료: Rust HOG 검증

- [x] `rust/overmax_cv` PyO3 확장 골격 유지
- [x] Python 3.14 환경에서 빌드되도록 PyO3 버전 조정
- [x] `maturin develop --release`로 `.venv_build`에 설치 확인
- [x] `test/hog_compat_check.py --backend rust` 검증 경로 추가
- [x] 실제 재킷 이미지셋으로 DB top-1 기준 확인
- [x] OpenCV HOG에 더 가깝게 block-local 투표, Gaussian block weight, border gradient 보정 적용
- [x] 기준 통과 전까지 `detection/image_db.py` 프로덕션 경로 변경 금지

## 현재 단계: OpenCV 제거 Phase 1

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
- [ ] 가능하면 정적 OCR ROI 샘플 비교

## 현재 단계: OpenCV 제거 Phase 3

- [x] `runtime_patch.py`의 `patch_cv2()` 제거
- [x] `overmax.spec` hiddenimports에서 `cv2` 제거
- [x] `requirements.txt`에서 `opencv-python-headless` 제거
- [x] OpenCV 기반 검증/개발 도구용 `requirements-dev.txt` 분리
- [x] PyInstaller 빌드 후 결과물 확인
- [x] 앱 import smoke test
- [ ] 실제 앱 실행 smoke test

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
