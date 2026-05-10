# overmax_cv

Overmax에서 OpenCV 제거 가능성을 검증하기 위한 최소 Rust/PyO3 확장이다.
현재 목적은 기존 `cache/image_index.db`의 HOG BLOB과 비교 가능한
1764차원 `float32` HOG feature를 생성하는 것이다.

## 현재 구현 범위

- 입력: 64x64 grayscale bytes
- 출력: 1764개 `float`
- HOG 조건:
  - 8x8 cells
  - 16x16 blocks
  - 8px block stride
  - 9 unsigned orientation bins
  - orientation center offset 10 degrees
  - block-local cell spatial vote interpolation
  - Gaussian block weight, sigma 4.0
  - outer border gradients treated as zero
  - x-major block flatten order
  - L2-Hys normalization, clip 0.2

## 준비

Rust와 maturin이 아직 없다면 설치가 필요하다.

```powershell
.\.venv_build\Scripts\python.exe -m pip install maturin
```

Rust 설치 후 현재 폴더에서:

```powershell
cd rust\overmax_cv
$env:VIRTUAL_ENV=(Resolve-Path ..\..\.venv_build).Path
..\..\.venv_build\Scripts\python.exe -m maturin develop --release
```

`cargo test`를 직접 실행할 때는 PyO3가 사용할 Python을 지정한다.

```powershell
$env:PYO3_PYTHON=(Resolve-Path ..\..\.venv_build\Scripts\python.exe).Path
cargo test
```

## 검증 목표

Rust backend를 `test/hog_compat_check.py`에 연결한 뒤 최소 기준은 다음과 같다.

```text
candidate_expected_top1=795/795
candidate_matches_cv2_top1=795/795
```

이 기준을 통과하기 전에는 프로덕션 `detection/image_db.py` 경로를 바꾸지 않는다.

현재 `test/jackets` 795개 기준으로 top-1은 통과했고, stored HOG와의 cosine은
`min=0.949237 mean=0.996954 max=0.998480` 수준이다. byte-level 동일 구현은 아니므로
프로덕션 연결 전에는 worst case를 다시 확인한다.

Rust backend를 사용해 검증하려면:

```powershell
.\.venv_build\Scripts\python.exe test\hog_compat_check.py --backend rust --images-dir <재킷 이미지 폴더>
```
