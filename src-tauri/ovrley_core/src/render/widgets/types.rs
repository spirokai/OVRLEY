//! Shared widget cache and report types.
//!
//! Route and elevation widgets both normalize plot settings, project source
//! telemetry into widget-space geometry, cache static layers, and precompute
//! per-frame marker positions. This module keeps those shared data shapes in one
//! place.

use ovrley_widgets::widgets::label::validation::ValidatedLabel;
use ovrley_widgets::widgets::backdrop::validation::ValidatedBackdrop;
use ovrley_widgets::widgets::gradient::validation::ValidatedGradientWidget;
use ovrley_widgets::widgets::lean_angle::{layout::LeanAngleCache, validation::ValidatedLeanAngleWidget};
use ovrley_widgets::widgets::g_force::{layout::GForceCache, validation::ValidatedGForceWidget};
use ovrley_widgets::widgets::lap_timer::{layout::LapTimerLayout, validation::ValidatedLapTimer};
use ovrley_widgets::widgets::heading::{layout::HeadingCache, validation::ValidatedHeading};
use crate::normalize::ValidatedSceneConfig;
use ovrley_widgets::widgets::gauges::{frame::GaugeLayout, metric::GaugeFrameState, validation::{ValidatedArcGaugeWidget, ValidatedLinearGaugeWidget}};
use chrono_tz::Tz;
use ovrley_widgets::widgets::time::validation::ValidatedTimeValue;
use ovrley_widgets::model::types::{DisplayType, MetricKind};
use ovrley_widgets::widgets::value::validation::ValidatedValueWidget;
use skia_safe::Image;
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
    pub metric_kind: ovrley_widgets::model::types::MetricKind,
    pub display_type: ovrley_widgets::model::types::DisplayType,
    pub widget: WidgetRenderReport,
}

/// Validated heading-tape configuration together with its typed cache.
#[derive(Clone, Debug)]
pub struct PreparedHeadingTape {
    pub validated: ValidatedHeading,
    pub cache: Option<HeadingCache>,
}

/// Validated lean-angle configuration together with its typed cache.
#[derive(Clone, Debug)]
pub struct PreparedLeanAngle {
    pub validated: ValidatedLeanAngleWidget,
    pub cache: Option<LeanAngleCache>,
}

/// Validated linear-gauge configuration together with its typed cache.
#[derive(Clone, Debug)]
pub struct PreparedLinearGauge {
    pub validated: ValidatedLinearGaugeWidget,
    pub altitude_offset_m: f64,
    pub cache: Option<GaugeCache>,
}

/// Validated arc/corner-gauge configuration together with its typed cache.
#[derive(Clone, Debug)]
pub struct PreparedArcGauge {
    pub validated: ValidatedArcGaugeWidget,
    pub altitude_offset_m: f64,
    pub cache: Option<GaugeCache>,
}

/// Validated text metric together with activity-derived presentation state.
#[derive(Clone, Debug)]
pub struct PreparedStandardText {
    pub validated: ValidatedValueWidget,
    pub altitude_offset_m: f64,
}

/// Validated G-force configuration together with its typed cache.
#[derive(Clone, Debug)]
pub struct PreparedGForce {
    pub validated: ValidatedGForceWidget,
    pub cache: Option<GForceWidgetCache>,
}

/// Validated lap-timer configuration together with its render-owned cache.
#[derive(Clone, Debug)]
pub struct PreparedLapTimer {
    pub validated: ValidatedLapTimer,
    pub(crate) cache: Option<LapTimerLayout>,
}

/// One validated render value, keyed implicitly by its index in the config array.
#[derive(Clone, Debug)]
pub enum PreparedValue {
    StandardText(PreparedStandardText),
    TimeText(ValidatedTimeValue),
    Gradient(ValidatedGradientWidget),
    HeadingTape(PreparedHeadingTape),
    LeanAngle(PreparedLeanAngle),
    LinearGauge(PreparedLinearGauge),
    ArcGauge(PreparedArcGauge),
    GForce(PreparedGForce),
    LapTimer(PreparedLapTimer),
}

impl PreparedValue {
    pub fn metric_kind(&self) -> MetricKind {
        match self {
            Self::StandardText(value) => value.validated.metric,
            Self::TimeText(_) => MetricKind::Time,
            Self::Gradient(_) => MetricKind::Gradient,
            Self::HeadingTape(_) => MetricKind::Heading,
            Self::LeanAngle(_) => MetricKind::LeanAngle,
            Self::LinearGauge(value) => value.validated.metric,
            Self::ArcGauge(value) => value.validated.metric,
            Self::GForce(_) => MetricKind::GForce,
            Self::LapTimer(_) => MetricKind::LapTimer,
        }
    }

    pub fn display_type(&self) -> DisplayType {
        match self {
            Self::StandardText(value) => value.validated.display_type,
            Self::TimeText(value) => value.base.display_type,
            Self::Gradient(_) => DisplayType::Text,
            Self::HeadingTape(_) => DisplayType::Tape,
            Self::LeanAngle(_) => DisplayType::LeanAngle,
            Self::LinearGauge(_) => DisplayType::Linear,
            Self::ArcGauge(value) => value.validated.display_type,
            Self::GForce(_) => DisplayType::GForce,
            Self::LapTimer(_) => DisplayType::LapTimer,
        }
    }

    pub fn x(&self) -> f32 {
        match self {
            Self::StandardText(value) => value.validated.x,
            Self::TimeText(value) => value.base.x,
            Self::Gradient(value) => value.x,
            Self::HeadingTape(value) => value.validated.x,
            Self::LeanAngle(value) => value.validated.x,
            Self::LinearGauge(value) => value.validated.x,
            Self::ArcGauge(value) => value.validated.x,
            Self::GForce(value) => value.validated.x,
            Self::LapTimer(value) => value.validated.x,
        }
    }

    pub fn y(&self) -> f32 {
        match self {
            Self::StandardText(value) => value.validated.y,
            Self::TimeText(value) => value.base.y,
            Self::Gradient(value) => value.y,
            Self::HeadingTape(value) => value.validated.y,
            Self::LeanAngle(value) => value.validated.y,
            Self::LinearGauge(value) => value.validated.y,
            Self::ArcGauge(value) => value.validated.y,
            Self::GForce(value) => value.validated.y,
            Self::LapTimer(value) => value.validated.y,
        }
    }
}

/// Prepared assets shared across frame rendering.
#[derive(Clone, Debug)]
pub struct PreparedRenderAssets {
    pub(crate) scene: ValidatedSceneConfig,
    pub(crate) export_start_seconds: f64,
    pub(crate) timezone: Option<Tz>,
    pub(crate) backdrops: Vec<ValidatedBackdrop>,
    pub(crate) labels: Vec<ValidatedLabel>,
    pub(crate) values: Vec<PreparedValue>,
    pub(crate) route_caches: Vec<RouteWidgetCache>,
    pub(crate) elevation_caches: Vec<ElevationWidgetCache>,
    pub(crate) base_rgba: Option<Vec<u8>>,
}

impl PreparedRenderAssets {
    /// Returns values after widget-specific typed caches have been prepared.
    pub fn values(&self) -> &[PreparedValue] {
        &self.values
    }

    /// Returns the elevation geometry as a JSON value for parity tests.
    ///
    /// The JSON shape matches `ElevationGeometryResponse` — points as
    /// `[[x,y], ...]` arrays and progressValues as a flat `f32` array.
    /// Returns `None` when no elevation widget is configured.
    pub fn elevation_geometry_json(&self) -> Option<serde_json::Value> {
        let cache = self.elevation_caches.first()?;
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
        let cache = self.route_caches.first()?;
        let geom = &cache.drawing.geometry;
        let scale = cache.drawing.scene.scale;
        Some(serde_json::json!({
            "points": geom.points.iter().map(|(x, y)| [x * scale, y * scale]).collect::<Vec<_>>(),
            "progressValues": geom.progress_values,
            "bbox": [0.0, 0.0, cache.drawing.config.width * scale, cache.drawing.config.height * scale],
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
    pub(crate) drawing: ovrley_widgets::widgets::route::layout::RouteLayout,
    pub(crate) frame_progress: Vec<f32>,
}


/// Cached static circle and dynamic drawing contract for a G-force widget.
#[derive(Clone, Debug)]
pub struct GForceWidgetCache {
    pub drawing: GForceCache,
    pub max_g: f64,
    pub horizontal_values: Vec<Option<f64>>,
    pub vertical_values: Vec<Option<f64>>,
}

/// Shared layout paired with native export frame samples.
#[derive(Clone, Debug)]
pub struct GaugeCache {
    pub layout: GaugeLayout,
    pub frame_states: Vec<GaugeFrameState>,
}

/// Prepared elevation widget cache.
#[derive(Clone, Debug)]
pub(crate) struct ElevationWidgetCache {
    pub(crate) plot: NormalizedElevationPlot,
    pub(crate) label_altitude_offset_m: f64,
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
