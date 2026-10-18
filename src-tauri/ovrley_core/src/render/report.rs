//! Native preview diagnostics and timing reports.
use super::assets::PreparedRenderAssets;
use crate::debug::TimingBucket;
use std::collections::BTreeMap;

/// Indicates whether the static label layer was not needed, reused, or rebuilt.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelCacheStatus {
    /// No static labels or static metric parts were present.
    None,
    /// A previously rendered static label image was reused.
    Hit,
    /// Static label image was rendered and inserted into the cache.
    Miss,
}

/// Serializable performance and geometry report for one preview render.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PreviewRenderReport {
    pub second: f64,
    pub frame_index: usize,
    pub width: u32,
    pub height: u32,
    pub total_ms: f64,
    pub surface_ms: f64,
    pub label_layer_ms: f64,
    pub value_draw_ms: f64,
    pub png_write_ms: f64,
    pub value_count: usize,
    pub label_count: usize,
    pub label_cache_status: LabelCacheStatus,
    pub route_widgets: Vec<WidgetRenderReport>,
    pub elevation_widgets: Vec<WidgetRenderReport>,
    pub diving_profiles: Vec<WidgetRenderReport>,
    pub metric_presentations: Vec<MetricPresentationReport>,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub frame_timings: BTreeMap<String, TimingBucket>,
    pub preview_only_timings: BTreeMap<String, TimingBucket>,
}

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

#[derive(Default)]
pub struct FrameRenderReport {
    pub route_widgets: Vec<WidgetRenderReport>,
    pub elevation_widgets: Vec<WidgetRenderReport>,
    pub diving_profiles: Vec<WidgetRenderReport>,
    pub metric_presentations: Vec<MetricPresentationReport>,
}

impl PreparedRenderAssets {
    /// Returns the elevation geometry as a JSON value for parity tests.
    ///
    /// The JSON shape matches `ElevationGeometryResponse` — points as
    /// `[[x,y], ...]` arrays and progressValues as a flat `f32` array.
    /// Returns `None` when no elevation widget is configured.
    pub fn elevation_geometry_json(&self) -> Option<serde_json::Value> {
        let cache = self.elevation_caches.first()?;
        let geom = &cache.layout.geometry;
        Some(serde_json::json!({
            "points": geom.points.iter().map(|(x, y)| [x, y]).collect::<Vec<_>>(),
            "progressValues": geom.progress_values,
            "elapsedFractions": geom.elapsed_fractions,
            "dataRange": geom.elevation_data_range.map(|(min, max)| [min, max]),
        "bbox": [0.0, 0.0, geom.key.width, geom.key.height],
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
        let geom = &cache.layout.geometry;
        let scale = cache.layout.scene.scale;
        Some(serde_json::json!({
            "points": geom.points.iter().map(|(x, y)| [x * scale, y * scale]).collect::<Vec<_>>(),
            "progressValues": geom.progress_values,
            "bbox": [0.0, 0.0, cache.layout.config.width * scale, cache.layout.config.height * scale],
            "sourcePointCount": geom.source_point_count,
            "simplification": geom.simplification,
        }))
    }
}

/// Plot reports use the same native reporting contract for routes and elevation profiles.
pub(super) fn plot_report(
    geometry: WidgetGeometryReport,
    origin: (f32, f32),
    progress01: f32,
    marker: (f32, f32),
) -> WidgetRenderReport {
    let (sin, cos) = geometry.rotation_deg.to_radians().sin_cos();
    WidgetRenderReport {
        frame: WidgetFrameReport {
            progress01,
            marker_x: marker.0,
            marker_y: marker.1,
            marker_abs_x: origin.0 + marker.0 * cos - marker.1 * sin,
            marker_abs_y: origin.1 + marker.0 * sin + marker.1 * cos,
        },
        geometry,
    }
}

pub(super) fn boxed_report(
    bounds: [f32; 4],
    size: (u32, u32),
    rotation: f32,
    simplification: &str,
    source_point_count: usize,
    progress01: f32,
    marker: (f32, f32),
) -> WidgetRenderReport {
    WidgetRenderReport {
        geometry: WidgetGeometryReport {
            point_count: 0,
            source_point_count,
            simplification: simplification.into(),
            bbox: bounds,
            widget_width: size.0,
            widget_height: size.1,
            rotation_deg: rotation,
        },
        frame: WidgetFrameReport {
            progress01,
            marker_x: marker.0,
            marker_y: marker.1,
            marker_abs_x: bounds[0] + marker.0,
            marker_abs_y: bounds[1] + marker.1,
        },
    }
}
