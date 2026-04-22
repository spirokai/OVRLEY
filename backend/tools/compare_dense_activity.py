import argparse
import json
from datetime import datetime
from pathlib import Path


NUMERIC_RELATIVE_KEYS = {
    "speed",
    "elevation",
    "gradient",
    "heartrate",
    "cadence",
    "power",
    "temperature",
}


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def safe_abs_error(left, right):
    if left is None or right is None:
        return None if left is None and right is None else float("inf")
    return abs(float(left) - float(right))


def safe_rel_error(left, right):
    if left is None or right is None:
        return None if left is None and right is None else float("inf")
    scale = max(abs(float(left)), abs(float(right)), 1e-12)
    return abs(float(left) - float(right)) / scale


def compare_series(left_values, right_values, abs_tolerance, rel_tolerance=None):
    max_abs_error = 0.0
    max_rel_error = 0.0
    mismatch_count = 0

    if len(left_values) != len(right_values):
        return {
            "max_abs_error": float("inf"),
            "max_rel_error": float("inf") if rel_tolerance is not None else None,
            "mismatch_count": abs(len(left_values) - len(right_values)),
            "length_equal": False,
            "pass": False,
        }

    for left, right in zip(left_values, right_values):
        abs_error = safe_abs_error(left, right)
        if abs_error is None:
            continue
        rel_error = safe_rel_error(left, right) if rel_tolerance is not None else None
        max_abs_error = max(max_abs_error, abs_error)
        if rel_error is not None:
            max_rel_error = max(max_rel_error, rel_error)
        if abs_error > abs_tolerance or (rel_tolerance is not None and rel_error > rel_tolerance):
            mismatch_count += 1

    result = {
        "max_abs_error": max_abs_error,
        "mismatch_count": mismatch_count,
        "length_equal": True,
        "pass": mismatch_count == 0,
    }
    if rel_tolerance is not None:
        result["max_rel_error"] = max_rel_error
    return result


def compare_time_series(left_values, right_values):
    mismatch_count = 0
    if len(left_values) != len(right_values):
        return {"mismatch_count": abs(len(left_values) - len(right_values)), "pass": False}
    for left, right in zip(left_values, right_values):
        if left == right:
            continue
        if left is None or right is None:
            mismatch_count += 1
            continue
        left_dt = datetime.fromisoformat(left.replace("Z", "+00:00"))
        right_dt = datetime.fromisoformat(right.replace("Z", "+00:00"))
        if abs((left_dt - right_dt).total_seconds()) > 1e-3:
            mismatch_count += 1
    return {"mismatch_count": mismatch_count, "pass": mismatch_count == 0}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--left", required=True)
    parser.add_argument("--right", required=True)
    parser.add_argument("--report", required=True)
    args = parser.parse_args()

    left = load_json(args.left)
    right = load_json(args.right)

    report = {
        "frame_count_equal": left.get("frame_count") == right.get("frame_count"),
        "series": {},
    }

    report["frame_elapsed_seconds"] = compare_series(
        left.get("frame_elapsed_seconds", []),
        right.get("frame_elapsed_seconds", []),
        1e-6,
    )
    report["frame_distance_progress"] = compare_series(
        left.get("frame_distance_progress", []),
        right.get("frame_distance_progress", []),
        1e-6,
    )

    for key in ["speed", "elevation", "gradient", "heartrate", "cadence", "power", "temperature"]:
        report["series"][key] = compare_series(
            left.get("series", {}).get(key, []),
            right.get("series", {}).get(key, []),
            1e-3,
            1e-4,
        )

    for key in ["course_lat", "course_lon"]:
        report["series"][key] = compare_series(
            left.get("series", {}).get(key, []),
            right.get("series", {}).get(key, []),
            1e-6,
        )

    report["series"]["time"] = compare_time_series(
        left.get("series", {}).get("time", []),
        right.get("series", {}).get("time", []),
    )

    report["pass"] = report["frame_count_equal"] and report["frame_elapsed_seconds"]["pass"] and report[
        "frame_distance_progress"
    ]["pass"] and all(item["pass"] for item in report["series"].values())

    report_path = Path(args.report)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
