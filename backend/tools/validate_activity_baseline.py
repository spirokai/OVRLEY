import argparse
import json
from bisect import bisect_left, bisect_right
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Optional


def parse_payload(path: Path):
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload.get("parsed_activity", payload)


def parse_iso(value):
    if not value:
        return None
    normalized = value.replace("Z", "+00:00")
    return datetime.fromisoformat(normalized)


def to_iso_millis(value: Optional[datetime]):
    if value is None:
        return None
    return value.astimezone(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def collect_points(x_values, y_values):
    points = []
    for x_value, y_value in zip(x_values, y_values):
        if y_value is None:
            continue
        points.append((float(x_value), float(y_value)))
    return points


def interpolate_points(points, target_x):
    if not points:
        return None
    if len(points) == 1:
        return points[0][1]
    if target_x <= points[0][0]:
        return points[0][1]
    if target_x >= points[-1][0]:
        return points[-1][1]
    right_index = bisect_left([point[0] for point in points], target_x)
    if right_index < len(points) and abs(points[right_index][0] - target_x) <= 1e-9:
        return points[right_index][1]
    left_x, left_y = points[right_index - 1]
    right_x, right_y = points[right_index]
    if abs(right_x - left_x) <= 1e-12:
        return right_y
    ratio = (target_x - left_x) / (right_x - left_x)
    return left_y + (right_y - left_y) * ratio


def interpolate_numeric(x_values, y_values, target_x):
    return interpolate_points(collect_points(x_values, y_values), target_x)


def interpolate_course(x_values, course_values, target_x):
    latitudes = [point[0] if isinstance(point, list) and len(point) >= 2 else None for point in course_values]
    longitudes = [point[1] if isinstance(point, list) and len(point) >= 2 else None for point in course_values]
    return [
        interpolate_numeric(x_values, latitudes, target_x),
        interpolate_numeric(x_values, longitudes, target_x),
    ]


def interpolate_time(x_values, time_values, target_x):
    points = []
    for x_value, raw_value in zip(x_values, time_values):
        parsed = parse_iso(raw_value)
        if parsed is None:
            continue
        points.append((float(x_value), parsed.timestamp() * 1000.0))
    millis = interpolate_points(points, target_x)
    if millis is None:
        return None
    return to_iso_millis(datetime.fromtimestamp(millis / 1000.0, tz=timezone.utc))


def trim_activity(activity, start, end):
    elapsed = [float(value) for value in activity["sample_elapsed_seconds"]]
    start_inner_index = bisect_right(elapsed, start)
    end_inner_index = bisect_left(elapsed, end)
    trimmed_elapsed = [0.0]
    trimmed_elapsed.extend(value - start for value in elapsed[start_inner_index:end_inner_index])
    trimmed_elapsed.append(end - start)

    def trim_numeric_series(key):
        source = activity.get(key, [])
        trimmed = [interpolate_numeric(elapsed, source, start)]
        trimmed.extend(source[start_inner_index:end_inner_index])
        trimmed.append(interpolate_numeric(elapsed, source, end))
        return trimmed

    trimmed_course = [interpolate_course(elapsed, activity.get("course", []), start)]
    trimmed_course.extend(activity.get("course", [])[start_inner_index:end_inner_index])
    trimmed_course.append(interpolate_course(elapsed, activity.get("course", []), end))

    source_progress = activity.get("sample_distance_progress", [])
    trimmed_progress = []
    if source_progress:
        start_progress = interpolate_numeric(elapsed, source_progress, start) or 0.0
        end_progress = interpolate_numeric(elapsed, source_progress, end) or start_progress
        span = max(end_progress - start_progress, 1e-9)
        trimmed_progress = [start_progress]
        trimmed_progress.extend(source_progress[start_inner_index:end_inner_index])
        trimmed_progress.append(end_progress)
        trimmed_progress = [(value - start_progress) / span for value in trimmed_progress]

    source_start_time = parse_iso(activity.get("source_start_time"))
    trimmed_start_time = None
    if source_start_time is not None:
        trimmed_start_time = source_start_time + timedelta(seconds=start)

    time_series = activity.get("time", [])
    trimmed_time = [interpolate_time(elapsed, time_series, start)]
    trimmed_time.extend(time_series[start_inner_index:end_inner_index])
    trimmed_time.append(interpolate_time(elapsed, time_series, end))

    return {
        "source_start_time": to_iso_millis(trimmed_start_time),
        "sample_elapsed_seconds": trimmed_elapsed,
        "sample_distance_progress": trimmed_progress,
        "course": trimmed_course,
        "elevation": trim_numeric_series("elevation"),
        "speed": trim_numeric_series("speed"),
        "heartrate": trim_numeric_series("heartrate"),
        "cadence": trim_numeric_series("cadence"),
        "power": trim_numeric_series("power"),
        "temperature": trim_numeric_series("temperature"),
        "gradient": trim_numeric_series("gradient"),
        "time": trimmed_time,
    }


def densify_activity(trimmed, fps):
    duration = trimmed["sample_elapsed_seconds"][-1]
    frame_elapsed_seconds = []
    frame_index = 0
    while True:
        target_x = frame_index / fps
        if target_x + 1e-9 >= duration and frame_index > 0:
            break
        frame_elapsed_seconds.append(min(target_x, duration))
        frame_index += 1

    def dense_numeric(key):
        return [
            interpolate_numeric(trimmed["sample_elapsed_seconds"], trimmed[key], target_x)
            for target_x in frame_elapsed_seconds
        ]

    source_start_time = parse_iso(trimmed.get("source_start_time"))
    if source_start_time is not None:
        dense_time = [
            to_iso_millis(source_start_time + timedelta(seconds=target_x))
            for target_x in frame_elapsed_seconds
        ]
    else:
        dense_time = [
            interpolate_time(trimmed["sample_elapsed_seconds"], trimmed["time"], target_x)
            for target_x in frame_elapsed_seconds
        ]

    course_lat = []
    course_lon = []
    for target_x in frame_elapsed_seconds:
        latitude, longitude = interpolate_course(
            trimmed["sample_elapsed_seconds"], trimmed["course"], target_x
        )
        course_lat.append(latitude)
        course_lon.append(longitude)

    frame_distance_progress = (
        [
            interpolate_numeric(
                trimmed["sample_elapsed_seconds"],
                trimmed["sample_distance_progress"],
                target_x,
            )
            for target_x in frame_elapsed_seconds
        ]
        if trimmed["sample_distance_progress"]
        else []
    )

    return {
        "frame_count": len(frame_elapsed_seconds),
        "frame_elapsed_seconds": frame_elapsed_seconds,
        "frame_distance_progress": frame_distance_progress,
        "series": {
            "speed": dense_numeric("speed"),
            "elevation": dense_numeric("elevation"),
            "gradient": dense_numeric("gradient"),
            "heartrate": dense_numeric("heartrate"),
            "cadence": dense_numeric("cadence"),
            "power": dense_numeric("power"),
            "temperature": dense_numeric("temperature"),
            "course_lat": course_lat,
            "course_lon": course_lon,
            "time": dense_time,
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--payload", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    activity = parse_payload(Path(args.payload))
    config = json.loads(Path(args.config).read_text(encoding="utf-8"))
    trimmed = trim_activity(activity, float(config["scene"]["start"]), float(config["scene"]["end"]))
    report = densify_activity(trimmed, float(config["scene"]["fps"]))

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
