from __future__ import annotations

import argparse
import sqlite3
import sys
from dataclasses import dataclass
from pathlib import Path

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from detection.image_db import _compute_hashes


@dataclass(frozen=True)
class HogStats:
    name: str
    cosine: float
    mean_abs: float
    max_abs: float
    cv_norm: float
    candidate_norm: float


@dataclass(frozen=True)
class DbEntry:
    image_id: str
    phash: int
    dhash: int
    ahash: int
    hog: np.ndarray


@dataclass(frozen=True)
class SearchResult:
    expected_id: str
    cv2_id: str
    candidate_id: str
    cv2_score: float
    candidate_score: float
    db_cosine: float


def cv2_hog(gray: np.ndarray) -> np.ndarray:
    descriptor = cv2.HOGDescriptor(
        _winSize=(64, 64),
        _blockSize=(16, 16),
        _blockStride=(8, 8),
        _cellSize=(8, 8),
        _nbins=9,
    )
    features = descriptor.compute(gray)
    if features is None:
        return np.zeros((1764,), dtype=np.float32)
    return features.reshape(-1).astype(np.float32)


def candidate_hog(gray: np.ndarray) -> np.ndarray:
    src = gray.astype(np.float32)
    gx = _central_diff_x(src)
    gy = _central_diff_y(src)
    mag = np.sqrt(gx * gx + gy * gy)
    angle = np.rad2deg(np.arctan2(gy, gx)) % 180.0

    cells = _cell_histograms(mag, angle)
    return _blocks_from_cells(cells)


def _central_diff_x(src: np.ndarray) -> np.ndarray:
    gx = np.zeros_like(src, dtype=np.float32)
    gx[:, 1:-1] = src[:, 2:] - src[:, :-2]
    gx[:, 0] = src[:, 1] - src[:, 0]
    gx[:, -1] = src[:, -1] - src[:, -2]
    return gx


def _central_diff_y(src: np.ndarray) -> np.ndarray:
    gy = np.zeros_like(src, dtype=np.float32)
    gy[1:-1, :] = src[2:, :] - src[:-2, :]
    gy[0, :] = src[1, :] - src[0, :]
    gy[-1, :] = src[-1, :] - src[-2, :]
    return gy


def _cell_histograms(mag: np.ndarray, angle: np.ndarray) -> np.ndarray:
    cells = np.zeros((8, 8, 9), dtype=np.float32)
    for y in range(mag.shape[0]):
        for x in range(mag.shape[1]):
            _vote_pixel(cells, x, y, float(mag[y, x]), float(angle[y, x]))
    return cells


def _vote_pixel(cells: np.ndarray, x: int, y: int, mag: float, angle: float) -> None:
    if mag == 0.0:
        return
    cell_x = (x + 0.5) / 8.0 - 0.5
    cell_y = (y + 0.5) / 8.0 - 0.5
    left = int(np.floor(cell_x))
    top = int(np.floor(cell_y))
    x_frac = cell_x - left
    y_frac = cell_y - top
    _vote_cell(cells, left, top, mag * (1.0 - x_frac) * (1.0 - y_frac), angle)
    _vote_cell(cells, left + 1, top, mag * x_frac * (1.0 - y_frac), angle)
    _vote_cell(cells, left, top + 1, mag * (1.0 - x_frac) * y_frac, angle)
    _vote_cell(cells, left + 1, top + 1, mag * x_frac * y_frac, angle)


def _vote_cell(cells: np.ndarray, cx: int, cy: int, mag: float, angle: float) -> None:
    if not (0 <= cx < 8 and 0 <= cy < 8):
        return
    bins = (angle - 10.0) / 20.0
    low = int(np.floor(bins)) % 9
    frac = bins - np.floor(bins)
    high = (low + 1) % 9
    cells[cy, cx, low] += mag * (1.0 - frac)
    cells[cy, cx, high] += mag * frac


def _blocks_from_cells(cells: np.ndarray) -> np.ndarray:
    blocks = []
    for x in range(7):
        for y in range(7):
            block = cells[y:y + 2, x:x + 2, :].reshape(-1)
            blocks.append(_normalize_block(block))
    return np.concatenate(blocks).astype(np.float32)


def _normalize_block(block: np.ndarray) -> np.ndarray:
    norm = np.sqrt(float(np.dot(block, block)) + 1e-6)
    out = block / norm
    out = np.minimum(out, 0.2)
    norm2 = np.sqrt(float(np.dot(out, out)) + 1e-6)
    return out / norm2


def compare(name: str, gray: np.ndarray) -> HogStats:
    resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
    expected = cv2_hog(resized)
    actual = candidate_hog(resized)
    diff = np.abs(expected - actual)
    return HogStats(
        name=name,
        cosine=_cosine(expected, actual),
        mean_abs=float(np.mean(diff)),
        max_abs=float(np.max(diff)),
        cv_norm=float(np.linalg.norm(expected)),
        candidate_norm=float(np.linalg.norm(actual)),
    )


def _cosine(a: np.ndarray, b: np.ndarray) -> float:
    denom = float(np.linalg.norm(a) * np.linalg.norm(b))
    if denom == 0.0:
        return 1.0
    return float(np.dot(a, b) / denom)


def make_synthetic_samples() -> list[tuple[str, np.ndarray]]:
    rng = np.random.default_rng(20260510)
    samples = [
        ("horizontal_gradient", _gradient_x()),
        ("vertical_gradient", _gradient_y()),
        ("checker", _checker()),
        ("circle", _circle()),
    ]
    for idx in range(8):
        samples.append((f"noise_{idx}", rng.integers(0, 256, (64, 64), dtype=np.uint8)))
    return samples


def _gradient_x() -> np.ndarray:
    row = np.linspace(0, 255, 64, dtype=np.uint8)
    return np.tile(row, (64, 1))


def _gradient_y() -> np.ndarray:
    col = np.linspace(0, 255, 64, dtype=np.uint8).reshape(64, 1)
    return np.tile(col, (1, 64))


def _checker() -> np.ndarray:
    yy, xx = np.indices((64, 64))
    return (((xx // 8 + yy // 8) % 2) * 255).astype(np.uint8)


def _circle() -> np.ndarray:
    yy, xx = np.indices((64, 64))
    mask = (xx - 32) ** 2 + (yy - 32) ** 2 <= 18 ** 2
    return (mask * 255).astype(np.uint8)


def load_image_samples(paths: list[Path]) -> list[tuple[str, np.ndarray]]:
    samples = []
    for path in paths:
        img = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
        gray = _to_gray(img)
        if gray is None:
            print(f"[skip] cannot load: {path}")
            continue
        samples.append((path.name, gray))
    return samples


def _to_gray(img: np.ndarray | None) -> np.ndarray | None:
    if img is None or img.size == 0:
        return None
    if img.ndim == 2:
        return img
    if img.ndim == 3 and img.shape[2] == 4:
        return cv2.cvtColor(img, cv2.COLOR_BGRA2GRAY)
    if img.ndim == 3 and img.shape[2] == 3:
        return cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
    return None


def print_stats(stats: list[HogStats]) -> None:
    for item in stats:
        print(
            f"{item.name:24} cosine={item.cosine:.6f} "
            f"mean_abs={item.mean_abs:.6f} max_abs={item.max_abs:.6f} "
            f"norms=({item.cv_norm:.3f}, {item.candidate_norm:.3f})"
        )
    if stats:
        cosines = np.array([s.cosine for s in stats], dtype=np.float32)
        print(
            f"summary                  min={float(cosines.min()):.6f} "
            f"mean={float(cosines.mean()):.6f} max={float(cosines.max()):.6f}"
        )


def load_db_entries(db_path: Path) -> list[DbEntry]:
    with sqlite3.connect(db_path) as conn:
        rows = conn.execute(
            "SELECT image_id, phash, dhash, ahash, hog FROM images"
        ).fetchall()
    return [
        DbEntry(
            image_id=str(row[0]),
            phash=int(str(row[1]), 16),
            dhash=int(str(row[2]), 16),
            ahash=int(str(row[3]), 16),
            hog=np.frombuffer(row[4], dtype=np.float32).copy(),
        )
        for row in rows
    ]


def evaluate_db(
    images_dir: Path,
    db_path: Path,
    top_k: int,
    hash_weight: float,
) -> None:
    entries = load_db_entries(db_path)
    paths = sorted(_iter_images(images_dir), key=lambda item: item.name)
    results = [_evaluate_image(path, entries, top_k, hash_weight) for path in paths]
    results = [item for item in results if item is not None]
    _print_search_summary(results, hash_weight)


def _iter_images(images_dir: Path) -> list[Path]:
    extensions = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}
    return [p for p in images_dir.rglob("*") if p.suffix.lower() in extensions]


def _evaluate_image(
    path: Path,
    entries: list[DbEntry],
    top_k: int,
    hash_weight: float,
) -> SearchResult | None:
    img = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    gray = _to_gray(img)
    if gray is None:
        print(f"[skip] cannot load: {path}")
        return None

    resized = cv2.resize(gray, (64, 64), interpolation=cv2.INTER_AREA)
    expected_id = path.stem
    cv2_match = _search(gray, cv2_hog(resized), entries, top_k, 0.45)
    candidate = candidate_hog(resized)
    candidate_match = _search(gray, candidate, entries, top_k, hash_weight)
    db_cosine = _stored_hog_cosine(expected_id, candidate, entries)
    return SearchResult(
        expected_id=expected_id,
        cv2_id=cv2_match[0],
        candidate_id=candidate_match[0],
        cv2_score=cv2_match[1],
        candidate_score=candidate_match[1],
        db_cosine=db_cosine,
    )


def _search(
    gray: np.ndarray,
    q_hog: np.ndarray,
    entries: list[DbEntry],
    top_k: int,
    hash_weight: float,
) -> tuple[str, float]:
    q_ph, q_dh, q_ah = _hash_ints(gray)
    h_scores = np.array([
        _hash_score(entry, q_ph, q_dh, q_ah) for entry in entries
    ])
    idx = np.argsort(h_scores)[:min(len(entries), top_k)]
    return _best_hog_match(q_hog, entries, h_scores, idx, hash_weight)


def _hash_ints(gray: np.ndarray) -> tuple[int, int, int]:
    phash, dhash, ahash = _compute_hashes(gray)
    return int(phash, 16), int(dhash, 16), int(ahash, 16)


def _hash_score(entry: DbEntry, q_ph: int, q_dh: int, q_ah: int) -> float:
    return (
        0.5 * int.bit_count(entry.phash ^ q_ph)
        + 0.3 * int.bit_count(entry.dhash ^ q_dh)
        + 0.2 * int.bit_count(entry.ahash ^ q_ah)
    )


def _best_hog_match(
    q_hog: np.ndarray,
    entries: list[DbEntry],
    h_scores: np.ndarray,
    idx: np.ndarray,
    hash_weight: float,
) -> tuple[str, float]:
    q_norm = max(float(np.linalg.norm(q_hog)), 1.0)
    scores = []
    for entry_idx in idx:
        entry = entries[int(entry_idx)]
        hog_sim = _cosine_with_norm(q_hog, q_norm, entry.hog)
        hash_sim = max(0.0, min(1.0, 1.0 - float(h_scores[entry_idx]) / 64.0))
        scores.append(hash_weight * hash_sim + (1.0 - hash_weight) * hog_sim)
    best = int(np.argmax(scores))
    entry = entries[int(idx[best])]
    return entry.image_id, float(scores[best])


def _cosine_with_norm(a: np.ndarray, a_norm: float, b: np.ndarray) -> float:
    b_norm = max(float(np.linalg.norm(b)), 1.0)
    return float(np.dot(a, b) / (a_norm * b_norm))


def _stored_hog_cosine(
    image_id: str,
    candidate: np.ndarray,
    entries: list[DbEntry],
) -> float:
    match = next((entry for entry in entries if entry.image_id == image_id), None)
    if match is None:
        return 0.0
    return _cosine(candidate, match.hog)


def _print_search_summary(results: list[SearchResult], hash_weight: float) -> None:
    total = len(results)
    if total == 0:
        print("images=0")
        print(f"candidate_hash_weight={hash_weight:.2f}")
        print("no images found")
        return

    cv2_hits = sum(1 for item in results if item.cv2_id == item.expected_id)
    candidate_hits = sum(1 for item in results if item.candidate_id == item.expected_id)
    same_top1 = sum(1 for item in results if item.cv2_id == item.candidate_id)
    cosines = np.array([item.db_cosine for item in results], dtype=np.float32)
    print(f"images={total}")
    print(f"candidate_hash_weight={hash_weight:.2f}")
    print(f"cv2_expected_top1={cv2_hits}/{total} ({_percent(cv2_hits, total):.2f}%)")
    print(
        f"candidate_expected_top1={candidate_hits}/{total} "
        f"({_percent(candidate_hits, total):.2f}%)"
    )
    print(f"candidate_matches_cv2_top1={same_top1}/{total} ({_percent(same_top1, total):.2f}%)")
    print(
        f"candidate_vs_stored_hog_cosine min={float(cosines.min()):.6f} "
        f"mean={float(cosines.mean()):.6f} max={float(cosines.max()):.6f}"
    )
    _print_mismatches(results)
    _print_worst_cases(results)


def _percent(count: int, total: int) -> float:
    if total == 0:
        return 0.0
    return count * 100.0 / total


def _print_worst_cases(results: list[SearchResult]) -> None:
    print("worst_candidate_hog_cosine:")
    for item in sorted(results, key=lambda result: result.db_cosine)[:10]:
        print(
            f"- {item.expected_id}: candidate={item.candidate_id} "
            f"cv2={item.cv2_id} cosine={item.db_cosine:.6f} "
            f"scores=({item.candidate_score:.6f}, {item.cv2_score:.6f})"
        )


def _print_mismatches(results: list[SearchResult]) -> None:
    mismatches = [item for item in results if item.candidate_id != item.cv2_id]
    print("candidate_top1_mismatches:")
    if not mismatches:
        print("- none")
        return

    for item in mismatches[:20]:
        print(
            f"- expected={item.expected_id} candidate={item.candidate_id} "
            f"cv2={item.cv2_id} cosine={item.db_cosine:.6f} "
            f"scores=({item.candidate_score:.6f}, {item.cv2_score:.6f})"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", type=Path, default=Path("cache/image_index.db"))
    parser.add_argument("--images-dir", type=Path)
    parser.add_argument("--hash-weight", type=float, default=0.45)
    parser.add_argument("--top-k", type=int, default=10)
    parser.add_argument("images", nargs="*", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.images_dir is not None:
        evaluate_db(args.images_dir, args.db, args.top_k, args.hash_weight)
        return

    samples = load_image_samples(args.images) if args.images else make_synthetic_samples()
    stats = [compare(name, gray) for name, gray in samples]
    print_stats(stats)


if __name__ == "__main__":
    main()
