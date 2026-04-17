from dataclasses import dataclass, field
from time import perf_counter
from typing import Any


@dataclass
class FontCache:
    by_key: dict[tuple[str, int], object] = field(default_factory=dict)


@dataclass
class DirtyRegion:
    box: tuple[int, int, int, int]
    background: Any


class FrameBufferPool:
    def __init__(self, buffers):
        import queue

        self.available = queue.Queue(maxsize=len(buffers))
        self.buffer_ids = {id(buffer) for buffer in buffers}
        for buffer in buffers:
            self.available.put(buffer)

    def acquire(self):
        return self.available.get()

    def release(self, image):
        if id(image) not in self.buffer_ids:
            return False
        self.available.put(image)
        return True


@dataclass
class RouteFrameState:
    progress01: float
    marker_x: float
    marker_y: float
    segment_index: int
    bucket_index: int


@dataclass
class ElevationFrameState:
    progress01: float
    marker_x: float
    marker_y: float
    elevation_m: float
    label_text: str | None


@dataclass
class ElevationLabelStyle:
    font_path: str
    font_size: int
    color: str
    x_offset: int
    y_offset: int
    units: list[str]
    decimal_rounding: int | None


@dataclass
class WidgetGeometry:
    points: list[tuple[float, float]]
    bbox: tuple[float, float, float, float]
    cumulative_progress: list[float]


@dataclass
class RouteWidgetCache:
    source_config: dict[str, Any] | None
    geometry: WidgetGeometry | None
    widget_x: int
    widget_y: int
    widget_width: int
    widget_height: int
    rotation_deg: float
    render_mode: str
    bucket_count: int
    background_layer: Any | None
    completed_layer: Any | None
    rotated_background_layer: Any | None
    rotated_completed_layer: Any | None
    marker_sprite: Any | None
    marker_anchor: tuple[int, int]
    line_width: int
    simplified_points: list[tuple[float, float]]
    cumulative_progress: list[float]
    display_points: list[tuple[float, float]]
    bucket_masks: list[Any] | None
    bucket_overlays: list[Any] | None
    frame_states: list[RouteFrameState] = field(default_factory=list)


@dataclass
class ElevationWidgetCache:
    source_config: dict[str, Any] | None
    geometry: WidgetGeometry | None
    widget_x: int
    widget_y: int
    widget_width: int
    widget_height: int
    rotation_deg: float
    background_layer: Any | None
    completed_layer: Any | None
    rotated_background_layer: Any | None
    rotated_completed_layer: Any | None
    marker_sprite: Any | None
    marker_anchor: tuple[int, int]
    simplified_points: list[tuple[float, float]]
    frame_states: list[ElevationFrameState] = field(default_factory=list)
    label_style: ElevationLabelStyle | None = None


@dataclass
class RenderAssets:
    base_image: Any | None = None
    font_cache: FontCache = field(default_factory=FontCache)
    route_cache: RouteWidgetCache | None = None
    elevation_cache: ElevationWidgetCache | None = None
    plot_backgrounds: dict[str, tuple[Any, dict]] = field(default_factory=dict)
    dirty_regions: list[DirtyRegion] = field(default_factory=list)
    frame_buffer_pool: FrameBufferPool | None = None

    def initialize_frame_buffer_pool(self, pool_size):
        if self.base_image is None or pool_size <= 0:
            return
        self.frame_buffer_pool = FrameBufferPool(
            [self.base_image.copy() for _ in range(pool_size)]
        )

    def acquire_frame_image(self, render_profiler=None):
        if self.base_image is None:
            return None
        if self.frame_buffer_pool is None:
            return self.base_image.copy()

        image = self.frame_buffer_pool.acquire()
        if self.dirty_regions:
            started_at = perf_counter()
            for region in self.dirty_regions:
                left, top, _, _ = region.box
                image.paste(region.background, (left, top))
            if render_profiler is not None:
                render_profiler.record("base.restore", perf_counter() - started_at)
        return image

    def release_frame_image(self, image):
        if self.frame_buffer_pool is None:
            return False
        return self.frame_buffer_pool.release(image)
