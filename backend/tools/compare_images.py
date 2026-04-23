import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def load_rgba(path: Path) -> np.ndarray:
    with Image.open(path) as image:
        return np.asarray(image.convert("RGBA"), dtype=np.int16)


def compare_images(left_path: Path, right_path: Path) -> dict:
    left = load_rgba(left_path)
    right = load_rgba(right_path)

    dimensions_equal = left.shape == right.shape
    if not dimensions_equal:
        return {
            "left_dimensions": [int(left.shape[1]), int(left.shape[0])],
            "right_dimensions": [int(right.shape[1]), int(right.shape[0])],
            "dimensions_equal": False,
            "mean_abs_channel_error": None,
            "max_abs_channel_error": None,
            "changed_pixel_ratio": None,
            "changed_pixel_count": None,
            "total_pixel_count": None,
            "changed_bbox": None,
            "pass": False,
        }

    diff = np.abs(left - right)
    changed_mask = np.any(diff > 0, axis=2)
    changed_pixel_count = int(changed_mask.sum())
    total_pixel_count = int(changed_mask.size)
    changed_pixel_ratio = changed_pixel_count / total_pixel_count if total_pixel_count else 0.0
    mean_abs_channel_error = float(diff.mean()) if diff.size else 0.0
    max_abs_channel_error = int(diff.max()) if diff.size else 0

    changed_bbox = None
    if changed_pixel_count:
        ys, xs = np.where(changed_mask)
        changed_bbox = {
            "left": int(xs.min()),
            "top": int(ys.min()),
            "right": int(xs.max()),
            "bottom": int(ys.max()),
            "width": int(xs.max() - xs.min() + 1),
            "height": int(ys.max() - ys.min() + 1),
        }

    passed = (
        dimensions_equal
        and changed_pixel_ratio <= 0.02
        and mean_abs_channel_error <= 3.0
    )

    return {
        "left_dimensions": [int(left.shape[1]), int(left.shape[0])],
        "right_dimensions": [int(right.shape[1]), int(right.shape[0])],
        "dimensions_equal": True,
        "mean_abs_channel_error": mean_abs_channel_error,
        "max_abs_channel_error": max_abs_channel_error,
        "changed_pixel_ratio": changed_pixel_ratio,
        "changed_pixel_count": changed_pixel_count,
        "total_pixel_count": total_pixel_count,
        "changed_bbox": changed_bbox,
        "pass": passed,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare two RGBA images and emit a JSON diff report.")
    parser.add_argument("--left", required=True, help="Path to the baseline image.")
    parser.add_argument("--right", required=True, help="Path to the comparison image.")
    parser.add_argument("--report", required=True, help="Path to write the JSON report.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    left_path = Path(args.left)
    right_path = Path(args.right)
    report_path = Path(args.report)

    result = compare_images(left_path, right_path)

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
