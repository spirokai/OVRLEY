import json
import os
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime
from time import perf_counter

import constant


DEFAULT_SAMPLE_POINTS = (0.0, 0.25, 0.5, 0.75, 1.0)


def _to_bool(value):
    if isinstance(value, bool):
        return value
    if value is None:
        return False
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


@dataclass
class TimingBucket:
    total_seconds: float = 0.0
    count: int = 0
    max_seconds: float = 0.0

    def add(self, duration_seconds):
        self.total_seconds += duration_seconds
        self.count += 1
        if duration_seconds > self.max_seconds:
            self.max_seconds = duration_seconds

    def to_dict(self):
        average_seconds = self.total_seconds / self.count if self.count else 0.0
        return {
            "count": self.count,
            "total_ms": round(self.total_seconds * 1000, 3),
            "avg_ms": round(average_seconds * 1000, 3),
            "max_ms": round(self.max_seconds * 1000, 3),
        }


@dataclass
class RenderProfiler:
    buckets: dict[str, TimingBucket] = field(default_factory=dict)

    def record(self, name, duration_seconds):
        if name not in self.buckets:
            self.buckets[name] = TimingBucket()
        self.buckets[name].add(duration_seconds)

    @contextmanager
    def measure(self, name):
        start = perf_counter()
        try:
            yield
        finally:
            self.record(name, perf_counter() - start)

    def summary(self):
        return {
            name: bucket.to_dict()
            for name, bucket in sorted(self.buckets.items(), key=lambda item: item[0])
        }


@dataclass
class RenderPreparationTrace:
    started_at_iso: str
    events: list[dict] = field(default_factory=list)

    def add_event(self, name, started_at, ended_at):
        self.events.append(
            {
                "name": name,
                "started_at": started_at.isoformat(timespec="milliseconds"),
                "ended_at": ended_at.isoformat(timespec="milliseconds"),
                "duration_ms": round((ended_at.timestamp() - started_at.timestamp()) * 1000, 3),
            }
        )

    def payload(self):
        return {
            "started_at": self.started_at_iso,
            "events": self.events,
        }


@dataclass
class RenderDebugOptions:
    enabled: bool = False
    output_dir: str | None = None
    save_sample_frames: bool = False
    write_timing_summary: bool = False
    sample_points: tuple[float, ...] = DEFAULT_SAMPLE_POINTS
    phase_name: str = "phase_1"

    @classmethod
    def from_scene_config(cls, scene_config):
        scene_config = scene_config or {}
        debug_config = scene_config.get("render_debug") or {}

        env_enabled = _to_bool(os.environ.get("CYCLEMETRY_RENDER_DEBUG"))
        enabled = _to_bool(debug_config.get("enabled", True)) or env_enabled

        sample_points = debug_config.get("sample_points") or DEFAULT_SAMPLE_POINTS
        if isinstance(sample_points, (int, float)):
            sample_points = DEFAULT_SAMPLE_POINTS
        else:
            sample_points = tuple(sample_points)

        phase_name = debug_config.get("phase") or "phase_1"
        output_dir = debug_config.get("output_dir")
        if enabled and not output_dir:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            output_dir = os.path.join(
                constant.WRITE_DIR(), "debug_render", phase_name, timestamp
            )

        save_sample_frames = enabled and debug_config.get(
            "save_sample_frames", True
        )
        write_timing_summary = enabled and debug_config.get(
            "write_timing_summary", True
        )

        return cls(
            enabled=enabled,
            output_dir=output_dir,
            save_sample_frames=save_sample_frames,
            write_timing_summary=write_timing_summary,
            sample_points=sample_points,
            phase_name=phase_name,
        )

    def ensure_output_dir(self):
        if self.enabled and self.output_dir:
            os.makedirs(self.output_dir, exist_ok=True)

    def sample_frame_indices(self, total_frames):
        if total_frames <= 0:
            return set()

        indices = set()
        last_index = total_frames - 1
        for point in self.sample_points:
            try:
                point = float(point)
            except (TypeError, ValueError):
                continue
            point = min(max(point, 0.0), 1.0)
            indices.add(min(last_index, round(last_index * point)))
        return indices

    def save_json(self, filename, payload):
        if not self.enabled or not self.output_dir:
            return
        self.ensure_output_dir()
        path = os.path.join(self.output_dir, filename)
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)

    def save_image(self, filename, image):
        if not self.enabled or not self.output_dir:
            return
        self.ensure_output_dir()
        path = os.path.join(self.output_dir, filename)
        image.save(path)

    def save_sample_frame(self, frame_index, image):
        if not self.enabled or not self.save_sample_frames or not self.output_dir:
            return
        self.save_image(f"sample_frame_{frame_index:04d}.png", image)


def build_timing_payload(
    profiler,
    scene_config,
    overlay_filename,
    total_frames,
    rendered_frames,
    sample_frame_indices,
):
    return {
        "phase": "phase_1",
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "overlay_filename": overlay_filename,
        "fps": scene_config.get("fps"),
        "width": scene_config.get("width"),
        "height": scene_config.get("height"),
        "total_frames": total_frames,
        "rendered_frames": rendered_frames,
        "sample_frame_indices": sorted(sample_frame_indices),
        "timings": profiler.summary(),
    }