# Decision: OpenCV Removal & Rust HOG Verification

## Background
To reduce distribution size and remove heavy dependencies, OpenCV (`cv2`) was replaced with a custom Rust extension (`overmax_cv`) for image processing tasks (grayscale, resize, HOG, OCR preprocess).

## Phases

### Phase 0: Rust HOG Verification
- [x] `rust/overmax_cv` PyO3 extension skeleton.
- [x] Build with Python 3.14 compatibility.
- [x] Verification with `test/hog_compat_check.py`.
- [x] Accuracy alignment with OpenCV HOG (Gaussian weighting, local voting).

### Phase 1: OpenCV Removal (Image DB)
- [x] Rust feature API: grayscale, area resize, hash, HOG.
- [x] Remove `cv2` from `detection/image_db.py`.
- [x] Remove `cv2` from `capture/helpers.py`.
- [x] Top-1 accuracy verification (795 jackets).

### Phase 2: OpenCV Removal (OCR)
- [x] Rust feature API: 3x upscale, Otsu thresholding, BMP encoding.
- [x] Remove `cv2` from `detection/ocr_wrapper.py`.
- [x] OCR smoke tests.

### Phase 3: Runtime & Build Cleanup
- [x] Remove `runtime_patch.py` and `patch_cv2()`.
- [x] Remove `cv2` from `overmax.spec` and `requirements.txt`.
- [x] Final build size reduction verification.
