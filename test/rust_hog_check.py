from __future__ import annotations

import argparse
import sys
from pathlib import Path

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from test.hog_compat_check import _to_gray, candidate_hog


def rust_hog(gray_64: np.ndarray) -> np.ndarray:
    try:
        from overmax_cv import hog_gray_64
    except ImportError as exc:
        raise SystemExit(
            "overmax_cv is not installed. Build it with maturin first."
        ) from exc

    return np.array(hog_gray_64(gray_64.tobytes()), dtype=np.float32)


def compare_image(path: Path) -> tuple[str, float, float]:
    img = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    gray = _to_gray(img)
    if gray is None:
        raise ValueError(f"cannot load image: {path}")

    resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
    expected = candidate_hog(resized)
    actual = rust_hog(resized)
    diff = np.abs(expected - actual)
    return path.name, _cosine(expected, actual), float(np.max(diff))


def _cosine(a: np.ndarray, b: np.ndarray) -> float:
    denom = float(np.linalg.norm(a) * np.linalg.norm(b))
    if denom == 0.0:
        return 1.0
    return float(np.dot(a, b) / denom)


def iter_images(root: Path) -> list[Path]:
    extensions = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}
    return [path for path in root.rglob("*") if path.suffix.lower() in extensions]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--images-dir", type=Path, default=Path("test/images"))
    parser.add_argument("--limit", type=int, default=20)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    paths = sorted(iter_images(args.images_dir), key=lambda path: path.name)
    if args.limit > 0:
        paths = paths[:args.limit]

    results = [compare_image(path) for path in paths]
    if not results:
        print("no images found")
        return

    for name, cosine, max_abs in results:
        print(f"{name:16} cosine={cosine:.8f} max_abs={max_abs:.8f}")

    cosines = np.array([item[1] for item in results], dtype=np.float32)
    print(f"summary          min={float(cosines.min()):.8f} mean={float(cosines.mean()):.8f}")


if __name__ == "__main__":
    main()
