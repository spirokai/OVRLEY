from dataclasses import dataclass, field
from typing import Any


@dataclass
class FontCache:
    by_key: dict[tuple[str, int], object] = field(default_factory=dict)


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
    reveal_mask: Any | None
    last_revealed_state_index: int = -1
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