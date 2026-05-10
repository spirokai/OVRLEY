//! Shared widget cache and report types.
//!
//! Route and elevation widgets both normalize plot settings, project source
//! telemetry into widget-space geometry, cache static layers, and precompute
//! per-frame marker positions. This module keeps those shared data shapes in one
//! place.

use crate::config::MarkerPointConfig;
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

/// Prepared assets shared across frame rendering.
#[derive(Clone, Debug, Default)]
pub struct PreparedRenderAssets {
    pub(crate) route_cache: Option<RouteWidgetCache>,
    pub(crate) elevation_cache: Option<ElevationWidgetCache>,
    pub(crate) base_rgba: Option<Vec<u8>>,
}

/// Widget-local polyline geometry and progress mapping.
#[derive(Clone, Debug)]
pub(crate) struct WidgetGeometry {
    pub(crate) points: Vec<(f32, f32)>,
    pub(crate) bbox: (f32, f32, f32, f32),
    pub(crate) progress_values: Vec<f32>,
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
}

/// One visual layer of a configurable marker.
#[derive(Clone, Debug)]
pub(crate) struct MarkerLayer {
    pub(crate) radius: f32,
    pub(crate) color: String,
    pub(crate) opacity: f32,
    pub(crate) solid_fill: bool,
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
pub(crate) struct ShadowStyle {
    pub(crate) color: String,
    pub(crate) strength: f32,
    pub(crate) distance: f32,
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
    pub(crate) marker_size: f32,
    pub(crate) marker_color: String,
    pub(crate) marker_opacity: f32,
    pub(crate) marker_points: Vec<MarkerPointConfig>,
}

/// Normalized elevation plot settings after defaults and scale are applied.
#[derive(Clone, Debug)]
pub(crate) struct NormalizedElevationPlot {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rotation: f32,
    pub(crate) margin: f32,
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
    pub(crate) marker_size: f32,
    pub(crate) marker_color: String,
    pub(crate) marker_opacity: f32,
    pub(crate) marker_points: Vec<MarkerPointConfig>,
    pub(crate) show_elevation_metric: bool,
    pub(crate) show_elevation_imperial: bool,
    pub(crate) metric_label_offset_x: f32,
    pub(crate) metric_label_offset_y: f32,
    pub(crate) imperial_label_offset_x: f32,
    pub(crate) imperial_label_offset_y: f32,
    pub(crate) label_font: Option<String>,
    pub(crate) label_font_size: f32,
    pub(crate) label_color: String,
    pub(crate) label_decimal_rounding: Option<i32>,
    pub(crate) legacy_label_units: Vec<String>,
}

/// Projected route sample with distance progress.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RouteSample {
    pub(crate) point: (f32, f32),
    pub(crate) progress01: f32,
}

/// Returns an empty serde-compatible extra-field map.
pub(crate) fn empty_extra() -> BTreeMap<String, serde_json::Value> {
    BTreeMap::new()
}
