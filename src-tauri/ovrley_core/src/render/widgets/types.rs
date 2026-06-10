//! Shared widget cache and report types.
//!
//! Route and elevation widgets both normalize plot settings, project source
//! telemetry into widget-space geometry, cache static layers, and precompute
//! per-frame marker positions. This module keeps those shared data shapes in one
//! place.

use crate::normalize::{
    ValidatedGradientWidget, ValidatedHeading, ValidatedLabel, ValidatedSceneConfig,
    ValidatedTimeValue, ValidatedValueWidget,
};
use crate::types::{DisplayType, MetricKind};
use skia_safe::Image;
use std::collections::BTreeMap;
use std::fmt;

/// Geometry diagnostics emitted for preview reports.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetGeometryReport {
    pub point_count: usize,
    pub source_point_count: usize,
    pub simplification: String,
    pub bbox: [f32; 4],
    pub widget_width: u32,
    pub widget_height: u32,
    pub rotation_deg: f32,
}

/// Per-frame widget diagnostics emitted for preview reports.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetFrameReport {
    pub progress01: f32,
    pub marker_x: f32,
    pub marker_y: f32,
    pub marker_abs_x: f32,
    pub marker_abs_y: f32,
}

/// Combined widget diagnostics for a rendered preview frame.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetRenderReport {
    pub geometry: WidgetGeometryReport,
    pub frame: WidgetFrameReport,
}

/// One metric-presentation report with enough identity to map diagnostics back
/// to the source widget in a multi-presentation template.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MetricPresentationReport {
    pub value_idx: usize,
    pub metric_kind: crate::types::MetricKind,
    pub display_type: crate::types::DisplayType,
    pub widget: WidgetRenderReport,
}

/// Per-widget presentation cache, keyed by value index in the config array.
#[derive(Clone, Debug)]
pub enum PresentationCache {
    HeadingTape(HeadingWidgetCache),
}

/// One validated render value, keyed implicitly by its index in the config array.
#[derive(Clone, Debug)]
pub enum PreparedValue {
    StandardText(ValidatedValueWidget),
    TimeText(ValidatedTimeValue),
    Gradient(ValidatedGradientWidget),
    HeadingTape(ValidatedHeading),
}

impl PreparedValue {
    pub fn metric_kind(&self) -> MetricKind {
        match self {
            Self::StandardText(value) => value.metric,
            Self::TimeText(_) => MetricKind::Time,
            Self::Gradient(_) => MetricKind::Gradient,
            Self::HeadingTape(_) => MetricKind::Heading,
        }
    }

    pub fn display_type(&self) -> DisplayType {
        match self {
            Self::StandardText(value) => value.display_type,
            Self::TimeText(value) => value.base.display_type,
            Self::Gradient(_) => DisplayType::Text,
            Self::HeadingTape(_) => DisplayType::Tape,
        }
    }

    pub fn x(&self) -> f32 {
        match self {
            Self::StandardText(value) => value.x,
            Self::TimeText(value) => value.base.x,
            Self::Gradient(value) => value.x,
            Self::HeadingTape(value) => value.x,
        }
    }

    pub fn y(&self) -> f32 {
        match self {
            Self::StandardText(value) => value.y,
            Self::TimeText(value) => value.base.y,
            Self::Gradient(value) => value.y,
            Self::HeadingTape(value) => value.y,
        }
    }
}

/// Prepared assets shared across frame rendering.
#[derive(Clone, Debug)]
pub struct PreparedRenderAssets {
    pub(crate) scene: ValidatedSceneConfig,
    pub(crate) labels: Vec<ValidatedLabel>,
    pub(crate) values: Vec<PreparedValue>,
    pub(crate) route_cache: Option<RouteWidgetCache>,
    pub(crate) elevation_cache: Option<ElevationWidgetCache>,
    pub presentation_caches: BTreeMap<usize, PresentationCache>,
    pub(crate) base_rgba: Option<Vec<u8>>,
}

impl PreparedRenderAssets {
    /// Returns the elevation geometry as a JSON value for parity tests.
    ///
    /// The JSON shape matches `ElevationGeometryResponse` — points as
    /// `[[x,y], ...]` arrays and progressValues as a flat `f32` array.
    /// Returns `None` when no elevation widget is configured.
    pub fn elevation_geometry_json(&self) -> Option<serde_json::Value> {
        let cache = self.elevation_cache.as_ref()?;
        let geom = &cache.geometry;
        Some(serde_json::json!({
            "points": geom.points.iter().map(|(x, y)| [x, y]).collect::<Vec<_>>(),
            "progressValues": geom.progress_values,
            "elapsedFractions": geom.elapsed_fractions,
            "dataRange": geom.elevation_data_range.map(|(min, max)| [min, max]),
            "bbox": [geom.bbox.0, geom.bbox.1, geom.bbox.2, geom.bbox.3],
            "sourcePointCount": geom.source_point_count,
            "simplification": geom.simplification,
        }))
    }

    /// Returns the route geometry as a JSON value for parity tests.
    ///
    /// The JSON shape matches `RouteGeometryResponse` — points as
    /// `[[x,y], ...]` arrays and progressValues as a flat `f32` array.
    /// Returns `None` when no route widget is configured.
    pub fn route_geometry_json(&self) -> Option<serde_json::Value> {
        let cache = self.route_cache.as_ref()?;
        let geom = &cache.geometry;
        Some(serde_json::json!({
            "points": geom.points.iter().map(|(x, y)| [x, y]).collect::<Vec<_>>(),
            "progressValues": geom.progress_values,
            "bbox": [geom.bbox.0, geom.bbox.1, geom.bbox.2, geom.bbox.3],
            "sourcePointCount": geom.source_point_count,
            "simplification": geom.simplification,
        }))
    }
}

/// Widget-local polyline geometry and progress mapping.
#[derive(Clone, Debug)]
pub(crate) struct WidgetGeometry {
    pub(crate) points: Vec<(f32, f32)>,
    pub(crate) bbox: (f32, f32, f32, f32),
    pub(crate) progress_values: Vec<f32>,
    pub(crate) elapsed_fractions: Vec<f32>,
    pub(crate) elevation_data_range: Option<(f64, f64)>,
    pub(crate) source_point_count: usize,
    pub(crate) simplification: String,
}

/// Precomputed route marker state for one frame.
#[derive(Clone, Debug)]
pub(crate) struct RouteFrameState {
    pub(crate) progress01: f32,
    pub(crate) marker_x: f32,
    pub(crate) marker_y: f32,
    pub(crate) segment_index: usize,
}

/// Precomputed elevation marker state for one frame.
#[derive(Clone, Debug)]
pub(crate) struct ElevationFrameState {
    pub(crate) progress01: f32,
    pub(crate) marker_x: f32,
    pub(crate) marker_y: f32,
    pub(crate) elevation_m: f64,
    pub(crate) frame_elapsed_fraction: f32,
}

/// One visual layer of a configurable marker.
#[derive(Clone, Debug)]
pub(crate) struct MarkerLayer {
    pub(crate) radius: f32,
    pub(crate) color: String,
    pub(crate) opacity: f32,
    pub(crate) solid_fill: bool,
    pub(crate) stroke_width: f32,
}

/// Prepared route widget cache.
#[derive(Clone, Debug)]
pub(crate) struct RouteWidgetCache {
    pub(crate) plot: NormalizedRoutePlot,
    pub(crate) geometry: WidgetGeometry,
    pub(crate) frame_states: Vec<RouteFrameState>,
    pub(crate) marker_layers: Vec<MarkerLayer>,
    pub(crate) remaining_layer: Option<StaticLayer>,
}

/// Prepared heading widget cache.
#[derive(Clone, Debug)]
pub struct HeadingWidgetCache {
    /// The cached 360° tape image (ticks + labels + shadows baked in).
    pub tape_image: Image,
    /// Tape image width in pixels (360 × pixels_per_degree).
    pub tape_width: f32,
    /// Widget position and dimensions.
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    /// Horizontal scale in pixels per degree.
    pub pixels_per_degree: f32,
    /// Whether to draw the indicator.
    pub show_indicator: bool,
    /// Indicator style: "chevron" or "highlight_bar".
    pub indicator_style: String,
    /// Indicator placement: "top", "bottom", or "both".
    pub indicator_placement: String,
    /// Indicator color as hex string.
    pub indicator_color: String,
    /// Indicator size in pixels (chevron height or bar width).
    pub indicator_size: f32,
    /// Shadow style for the indicator (inherited from widget config).
    pub indicator_shadow: Option<ShadowStyle>,
    /// Visual representation mode. When `Text`, the tape is not drawn.
    pub display_type: DisplayType,
}

/// Prepared elevation widget cache.
#[derive(Clone, Debug)]
pub(crate) struct ElevationWidgetCache {
    pub(crate) plot: NormalizedElevationPlot,
    pub(crate) geometry: WidgetGeometry,
    pub(crate) frame_states: Vec<ElevationFrameState>,
    pub(crate) marker_layers: Vec<MarkerLayer>,
    pub(crate) remaining_layer: Option<StaticLayer>,
}

/// Static Skia image positioned relative to a widget.
#[derive(Clone)]
pub(crate) struct StaticLayer {
    pub(crate) image: Image,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl fmt::Debug for StaticLayer {
    // Formats static layers without dumping the underlying image pixels.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticLayer")
            .field("x", &self.x)
            .field("y", &self.y)
            .finish_non_exhaustive()
    }
}

/// Drop-shadow style normalized from scene/template fields.
#[derive(Clone, Debug)]
pub struct ShadowStyle {
    pub(crate) color: String,
    pub(crate) strength: f32,
    pub(crate) distance: f32,
    pub(crate) offset_x: f32,
    pub(crate) offset_y: f32,
}

/// Normalized route plot settings after defaults and scale are applied.
#[derive(Clone, Debug)]
pub(crate) struct NormalizedRoutePlot {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rotation: f32,
    pub(crate) simplify_tolerance_px: f32,
    pub(crate) target_density: f32,
    pub(crate) remaining_line_width: f32,
    pub(crate) remaining_line_color: String,
    pub(crate) remaining_line_opacity: f32,
    pub(crate) remaining_line_shadow: Option<ShadowStyle>,
    pub(crate) completed_line_width: f32,
    pub(crate) completed_line_color: String,
    pub(crate) completed_line_opacity: f32,
    pub(crate) marker_variant: String,
    pub(crate) marker_variant_diameter: f32,
    pub(crate) marker_size: f32,
    pub(crate) marker_color: String,
    pub(crate) marker_opacity: f32,
}

/// Normalized elevation plot settings after defaults and scale are applied.
#[derive(Clone, Debug)]
pub(crate) struct NormalizedElevationPlot {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rotation: f32,
    pub(crate) y_scale: f32,
    pub(crate) simplify_tolerance_px: f32,
    pub(crate) target_density: f32,
    pub(crate) remaining_line_width: f32,
    pub(crate) remaining_line_color: String,
    pub(crate) remaining_line_opacity: f32,
    pub(crate) remaining_line_shadow: Option<ShadowStyle>,
    pub(crate) completed_line_width: f32,
    pub(crate) completed_line_color: String,
    pub(crate) completed_line_opacity: f32,
    pub(crate) area_remaining_color: String,
    pub(crate) area_remaining_opacity: f32,
    pub(crate) area_completed_color: String,
    pub(crate) area_completed_opacity: f32,
    pub(crate) marker_variant: String,
    pub(crate) marker_variant_diameter: f32,
    pub(crate) marker_size: f32,
    pub(crate) marker_color: String,
    pub(crate) marker_opacity: f32,
    pub(crate) show_elevation_metric: bool,
    pub(crate) show_elevation_imperial: bool,
    pub(crate) metric_label_offset_x: f32,
    pub(crate) metric_label_offset_y: f32,
    pub(crate) imperial_label_offset_x: f32,
    pub(crate) imperial_label_offset_y: f32,
    pub(crate) label_font: Option<String>,
    pub(crate) label_font_size: f32,
    pub(crate) label_color: String,
}

/// Projected route sample with distance progress.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RouteSample {
    pub(crate) point: (f64, f64),
    pub(crate) progress01: f32,
}
