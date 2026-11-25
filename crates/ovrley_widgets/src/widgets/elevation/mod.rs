//! Elevation profile: configuration, sampling, preview and resize.
use crate::activity::ActivitySampler;
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_f32, require_hex_color, require_opacity, require_positive_f32,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::FontResolver;
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::series::interpolate_points;
use crate::widgets::elevation::geometry::{ElevationFrameState, ElevationGeometry, GeometryKey};
use crate::widgets::elevation::layout::ElevationLayout;
use crate::widgets::plot_marker::{frame_resize_marker_factor, MarkerVariant};
use crate::widgets::route::RouteWindow;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::Canvas;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod geometry;
pub mod layout;

/// Effective profile config. Presentation defaults are materialized by the host.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ElevationPlotConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub area_remaining_color: String,
    pub area_remaining_opacity: f32,
    pub area_completed_color: String,
    pub area_completed_opacity: f32,
    pub area_gradient: bool,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_elevation_metric: bool,
    pub show_elevation_imperial: bool,
    /// Optional label correction; null/absence leaves measured altitude unchanged.
    pub starting_altitude: Option<f64>,
    pub starting_altitude_unit: Option<String>,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub point_label: PointLabelConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PointLabelConfig {
    /// Omitted font inherits the scene's optional native font selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    pub font_size: f32,
    pub color: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedElevationPlot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub area_remaining_color: String,
    pub area_remaining_opacity: f32,
    pub area_completed_color: String,
    pub area_completed_opacity: f32,
    /// Fades both area fills from their opacity at the plot top to transparent at its bottom.
    pub area_gradient: bool,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_elevation_metric: bool,
    pub show_elevation_imperial: bool,
    pub starting_altitude_m: Option<f64>,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub label_font: Option<String>,
    pub label_font_size: f32,
    pub label_color: String,
}

pub fn validate_elevation_plot(
    plot: &ElevationPlotConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> WidgetResult<ValidatedElevationPlot> {
    let p = |field: &str| format!("plots[{index}].{field}");
    let number = |value, field| require_f32(Some(value), &p(field));
    let positive = |value, field| require_positive_f32(Some(value), &p(field));
    let color = |value: &str, field| require_hex_color(Some(value), &p(field));
    let opacity = |value, field| require_opacity(Some(value), &p(field));
    let ranged = |value, field, min, max| {
        let value = number(value, field)?;
        if !(min..=max).contains(&value) {
            return Err(WidgetError::Config(format!(
                "{}: must be between {min} and {max}",
                p(field)
            )));
        }
        Ok(value)
    };
    if plot.value != "elevation" {
        return Err(WidgetError::Config(format!(
            "{}: must be elevation",
            p("value")
        )));
    }
    let altitude_scale = match plot.starting_altitude_unit.as_deref() {
        Some("m") => 1.0,
        Some("ft") => 3.280_84,
        None if plot.starting_altitude.is_none() => 1.0,
        _ => {
            return Err(WidgetError::Config(format!(
                "{}: expected m or ft",
                p("starting_altitude_unit")
            )))
        }
    };
    if plot
        .starting_altitude
        .is_some_and(|value| !value.is_finite())
    {
        return Err(WidgetError::Config(format!(
            "{}: must be finite",
            p("starting_altitude")
        )));
    }
    if plot
        .point_label
        .font
        .as_ref()
        .is_some_and(|font| font.trim().is_empty())
    {
        return Err(WidgetError::Config(format!(
            "{}: must identify a font",
            p("point_label.font")
        )));
    }
    Ok(ValidatedElevationPlot {
        x: number(plot.x, "x")?,
        y: number(plot.y, "y")?,
        width: positive(plot.width, "width")?,
        height: positive(plot.height, "height")?,
        rotation: number(plot.rotation, "rotation")?,
        y_scale: ranged(plot.y_scale, "y_scale", 0.2, 4.0)?,
        simplify_tolerance_px: ranged(
            plot.simplify_tolerance_px,
            "simplify_tolerance_px",
            0.0,
            8.0,
        )?,
        target_density: ranged(plot.target_density, "target_density", 0.1, 2.0)?,
        completed_line_width: positive(plot.completed_line_width, "completed_line_width")?,
        completed_line_color: color(&plot.completed_line_color, "completed_line_color")?,
        completed_line_opacity: opacity(plot.completed_line_opacity, "completed_line_opacity")?,
        remaining_line_width: positive(plot.remaining_line_width, "remaining_line_width")?,
        remaining_line_color: color(&plot.remaining_line_color, "remaining_line_color")?,
        remaining_line_opacity: opacity(plot.remaining_line_opacity, "remaining_line_opacity")?,
        area_remaining_color: color(&plot.area_remaining_color, "area_remaining_color")?,
        area_remaining_opacity: opacity(plot.area_remaining_opacity, "area_remaining_opacity")?,
        area_completed_color: color(&plot.area_completed_color, "area_completed_color")?,
        area_completed_opacity: opacity(plot.area_completed_opacity, "area_completed_opacity")?,
        area_gradient: plot.area_gradient,
        marker_variant: MarkerVariant::parse(&plot.marker_variant, &p("marker_variant"))?,
        marker_variant_diameter: positive(plot.marker_variant_diameter, "marker_variant_diameter")?,
        marker_size: positive(plot.marker_size, "marker_size")?,
        marker_color: color(&plot.marker_color, "marker_color")?,
        marker_opacity: opacity(plot.marker_opacity, "marker_opacity")?,
        show_full_activity: plot.show_full_activity,
        show_elevation_metric: plot.show_elevation_metric,
        show_elevation_imperial: plot.show_elevation_imperial,
        starting_altitude_m: plot.starting_altitude.map(|value| value / altitude_scale),
        metric_label_offset_x: number(plot.metric_label_offset_x, "metric_label_offset_x")?,
        metric_label_offset_y: number(plot.metric_label_offset_y, "metric_label_offset_y")?,
        imperial_label_offset_x: number(plot.imperial_label_offset_x, "imperial_label_offset_x")?,
        imperial_label_offset_y: number(plot.imperial_label_offset_y, "imperial_label_offset_y")?,
        label_font: plot.point_label.font.clone().or_else(|| scene.font.clone()),
        label_font_size: positive(plot.point_label.font_size, "point_label.font_size")?,
        label_color: color(&plot.point_label.color, "point_label.color")?,
    })
}

pub struct ElevationActivity {
    times: Vec<f64>,
    preferred: Vec<(f64, f64)>,
    elevation_range: Option<(f64, f64)>,
    preferred_values: Vec<Option<f64>>,
    profile: Vec<(f64, f64)>,
    progress: Vec<(f64, f64)>,
    full: Arc<Vec<(f32, f64, f32)>>,
    pub end: f64,
    duration: f64,
}

impl ElevationActivity {
    pub fn new(
        times: &[f64],
        profile: &[Option<f64>],
        preferred: &[Option<f64>],
        progress: &[f64],
        end: f64,
    ) -> Self {
        let numeric = |values: &[Option<f64>]| {
            times
                .iter()
                .copied()
                .zip(values)
                .filter_map(|(time, value)| {
                    value.filter(|v| v.is_finite()).map(|value| (time, value))
                })
                .collect::<Vec<_>>()
        };
        let profile = numeric(profile);
        let preferred_values = preferred.to_vec();
        let preferred = numeric(preferred);
        let elevation_range = preferred.iter().fold(None, |range, &(_, value)| {
            Some(range.map_or((value, value), |(min, max): (f64, f64)| {
                (min.min(value), max.max(value))
            }))
        });
        let duration = times.last().copied().unwrap_or(1.0).max(1e-9);
        let progress = times
            .iter()
            .copied()
            .enumerate()
            .map(|(i, time)| {
                (
                    time,
                    progress
                        .get(i)
                        .copied()
                        .filter(|v| v.is_finite())
                        .unwrap_or(i as f64 / times.len().saturating_sub(1).max(1) as f64),
                )
            })
            .collect::<Vec<_>>();
        let full = Arc::new(
            profile
                .iter()
                .map(|&(time, elevation)| {
                    (
                        interpolate_points(&progress, time)
                            .unwrap_or(0.0)
                            .clamp(0.0, 1.0) as f32,
                        elevation,
                        (time / duration).clamp(0.0, 1.0) as f32,
                    )
                })
                .collect(),
        );
        Self {
            times: times.to_vec(),
            preferred,
            elevation_range,
            preferred_values,
            profile,
            progress,
            full,
            end,
            duration,
        }
    }

    pub fn samples(&self, window: Option<RouteWindow>) -> Arc<Vec<(f32, f64, f32)>> {
        let Some(window) = window else {
            return self.full.clone();
        };
        let start = self.times.partition_point(|t| *t <= window.start);
        let end = self.times.partition_point(|t| *t < window.end).max(start);
        let mut times = Vec::with_capacity(end - start + 2);
        times.push(window.start);
        times.extend_from_slice(&self.times[start..end]);
        times.push(window.end);
        let first = interpolate_points(&self.progress, window.start);
        let last = interpolate_points(&self.progress, window.end);
        let span = first.zip(last).filter(|(a, b)| b > a);
        Arc::new(
            times
                .iter()
                .enumerate()
                .filter_map(|(i, &time)| {
                    let elevation = if i == 0 || i + 1 == times.len() {
                        interpolate_points(&self.preferred, time)
                    } else {
                        self.preferred_values
                            .get(start + i - 1)
                            .copied()
                            .flatten()
                            .filter(|v| v.is_finite())
                    }?;
                    let progress = span
                        .and_then(|(a, b)| {
                            interpolate_points(&self.progress, time).map(|v| (v - a) / (b - a))
                        })
                        .unwrap_or(i as f64 / (times.len() - 1) as f64);
                    Some((
                        progress.clamp(0.0, 1.0) as f32,
                        elevation,
                        ((time - window.start) / (window.end - window.start)).clamp(0.0, 1.0)
                            as f32,
                    ))
                })
                .collect(),
        )
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let absolute = interpolate_points(&self.progress, second).unwrap_or(second / self.duration);
        window
            .map_or(absolute, |window| {
                window.relative_progress(&self.progress, absolute, second)
            })
            .clamp(0.0, 1.0) as f32
    }

    pub fn elapsed_fraction(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        window
            .map_or(second / self.duration, |window| {
                (second - window.start) / (window.end - window.start)
            })
            .clamp(0.0, 1.0) as f32
    }

    /// Native elevation interpolation bridges gaps and leaves outside coverage missing.
    pub fn elevation_at(&self, second: f64) -> Option<f64> {
        self.preferred
            .first()
            .zip(self.preferred.last())
            .filter(|(a, b)| second >= a.0 && second <= b.0)
            .and_then(|_| interpolate_points(&self.preferred, second))
    }

    /// The same progress, elevation and chronological fill drive both rendering hosts.
    pub fn frame_state(
        &self,
        geometry: &crate::widgets::elevation::geometry::ElevationGeometry,
        second: f64,
        window: Option<RouteWindow>,
        elevation: Option<f64>,
    ) -> crate::widgets::elevation::geometry::ElevationFrameState {
        geometry.state(
            self.progress_at(second, window),
            Some(elevation.unwrap_or(0.0)),
            self.elapsed_fraction(second, window),
        )
    }

    pub fn altitude_offset(&self, target: Option<f64>) -> f64 {
        target
            .zip(self.profile.first())
            .map_or(0.0, |(target, first)| target - first.1)
    }

    pub fn elevation_range(&self) -> Option<(f64, f64)> {
        self.elevation_range
    }
}

/// Selects barometric altitude when the source provides it, otherwise generic elevation.
pub fn preferred_elevation_series<'a>(
    barometric: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if barometric.iter().any(Option::is_some) {
        barometric
    } else {
        elevation
    }
}

/// Finalized profile samples are authoritative when present.
pub fn elevation_profile_series<'a>(
    profile: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if profile.is_empty() {
        elevation
    } else {
        profile
    }
}

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    /// Whole object: widget patches merge shallowly, so a partial label would drop its font.
    pub point_label: PointLabelConfig,
}

pub fn resize_elevation(
    config: &mut ValidatedElevationPlot,
    scene: &ValidatedScenePresentation,
    size: [f32; 2],
    fixed: [f32; 2],
) -> WidgetResult<()> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid elevation resize size".into()));
    }
    let stroke =
        (config.completed_line_width.max(config.remaining_line_width) * scene.scale).max(1.0);
    let width = ((size[0] - stroke) / scene.scale).clamp(8.0, 10_000.0);
    let height = ((size[1] - stroke) / scene.scale).clamp(8.0, 10_000.0);
    // Strokes keep their own widths; the marker and its labels follow the handle's resize policy.
    let factor = frame_resize_marker_factor([config.width, config.height], [width, height], fixed);
    config.marker_size = (config.marker_size * factor).clamp(0.01, 400.0);
    config.marker_variant_diameter = (config.marker_variant_diameter * factor).clamp(0.01, 400.0);
    config.metric_label_offset_x *= factor;
    config.metric_label_offset_y *= factor;
    config.imperial_label_offset_x *= factor;
    config.imperial_label_offset_y *= factor;
    config.label_font_size = (config.label_font_size * factor).clamp(1.0, 400.0);
    config.width = width;
    config.height = height;
    Ok(())
}

pub fn anchored_update(
    config: &ValidatedElevationPlot,
    scene: &ValidatedScenePresentation,
    fixed: [f32; 2],
    point: [f32; 2],
    point_label: &PointLabelConfig,
) -> ResizeUpdate {
    let bounds = ElevationLayout::measure_content(config, scene);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let (x, y) = (
        bounds.left + bounds.width() * fixed[0],
        bounds.top + bounds.height() * fixed[1],
    );
    ResizeUpdate {
        x: point[0] - (x * cos - y * sin),
        y: point[1] - (x * sin + y * cos),
        width: config.width,
        height: config.height,
        marker_size: config.marker_size,
        marker_variant_diameter: config.marker_variant_diameter,
        metric_label_offset_x: config.metric_label_offset_x,
        metric_label_offset_y: config.metric_label_offset_y,
        imperial_label_offset_x: config.imperial_label_offset_x,
        imperial_label_offset_y: config.imperial_label_offset_y,
        point_label: PointLabelConfig {
            font_size: config.label_font_size,
            ..point_label.clone()
        },
    }
}

pub struct PreviewElevation {
    config: ValidatedElevationPlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    samples: Option<Arc<Vec<(f32, f64, f32)>>>,
    pub prepared_geometry: Option<Arc<ElevationGeometry>>,
    placeholder_geometry: Option<Arc<ElevationGeometry>>,
    layout: Option<ElevationLayout>,
    last_state: Option<ElevationFrameState>,
    pub tile: Tile,
}

impl PreviewElevation {
    fn timing(
        show_full_activity: bool,
        host: &WidgetHost,
    ) -> WidgetResult<(Option<RouteWindow>, f64)> {
        Ok((
            host.plot_window(show_full_activity),
            host.placeholder_duration()?,
        ))
    }

    pub fn new(
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_elevation_plot(&widget, 0, &scene)?;
        Self::from_validated(config, scene, &host)
    }

    pub fn from_validated(
        config: ValidatedElevationPlot,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let (window, preview_duration) = Self::timing(config.show_full_activity, host)?;
        Ok(Self {
            config,
            scene,
            window,
            preview_duration,
            preview_raster_scale: host.raster_scale,
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
            layout: None,
            last_state: None,
            tile: Tile::default(),
        })
    }

    pub fn update(
        &mut self,
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        transient: bool,
    ) -> WidgetResult<()> {
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let (window, preview_duration) = Self::timing(widget.show_full_activity, &host)?;
        let preview_raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_elevation_plot(&widget, 0, &scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        if !transient
            && self
                .prepared_geometry
                .as_ref()
                .is_some_and(|g| g.key != GeometryKey::new(&config, scene.scale))
        {
            self.prepared_geometry = None;
            self.placeholder_geometry = None;
        }
        if self.config == config
            && self.scene == scene
            && self.window == window
            && self.preview_duration == preview_duration
            && self.preview_raster_scale == preview_raster_scale
        {
            return Ok(());
        }
        if let Some(layout) = self.layout.as_mut() {
            // The cached remaining raster sits on the position's sub-pixel grid, so a move rebuilds.
            if layout.config != config
                || layout.scene != scene
                || self.preview_raster_scale != preview_raster_scale
            {
                self.layout = None;
            }
        }
        self.config = config;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.preview_raster_scale = preview_raster_scale;
        self.last_state = None;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.samples = None;
        self.prepared_geometry = None;
        self.placeholder_geometry = None;
        self.layout = None;
        self.last_state = None;
    }

    fn prepare_geometry(&mut self, activity: Option<&ActivitySampler>) -> WidgetResult<()> {
        if self.prepared_geometry.is_some() {
            return Ok(());
        }
        if self.samples.is_none() {
            self.samples = Some(
                activity
                    .map(|a| a.elevation.samples(self.window))
                    .unwrap_or_else(|| Arc::new(Vec::new())),
            );
        }
        let samples = self.samples.as_ref().expect("elevation samples prepared");
        self.prepared_geometry = Some(Arc::new(if samples.is_empty() {
            ElevationGeometry::placeholder(&self.config, self.scene.scale)
        } else {
            ElevationGeometry::new(&self.config, self.scene.scale, samples)?
        }));
        Ok(())
    }

    fn geometry_at(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<Arc<ElevationGeometry>> {
        self.prepare_geometry(activity)?;
        if activity.is_some_and(|a| second < 0.0 || second > a.elevation.end) {
            if self.placeholder_geometry.is_none() {
                self.placeholder_geometry = Some(Arc::new(ElevationGeometry::placeholder(
                    &self.config,
                    self.scene.scale,
                )));
            }
            Ok(self
                .placeholder_geometry
                .as_ref()
                .expect("placeholder elevation prepared")
                .clone())
        } else {
            Ok(self
                .prepared_geometry
                .as_ref()
                .expect("elevation geometry prepared")
                .clone())
        }
    }

    pub fn resize(
        &mut self,
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        size: [f32; 2],
        gesture: ResizeGesture,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<ResizeUpdate> {
        let ResizeGesture {
            fixed,
            point,
            second,
            commit,
            ..
        } = gesture;
        let scene = validate_scene_presentation(scene)?;
        let mut config = validate_elevation_plot(&widget, 0, &scene)?;
        self.geometry_at(activity, second)?;
        resize_elevation(&mut config, &scene, size, fixed)?;
        if commit {
            let samples = self.samples.as_ref().expect("elevation samples prepared");
            self.prepared_geometry = Some(Arc::new(if samples.is_empty() {
                ElevationGeometry::placeholder(&config, scene.scale)
            } else {
                ElevationGeometry::new(&config, scene.scale, samples)?
            }));
            if self.placeholder_geometry.is_some() {
                self.placeholder_geometry = Some(Arc::new(ElevationGeometry::placeholder(
                    &config,
                    scene.scale,
                )));
            }
            self.layout = None;
            self.last_state = None;
        }
        Ok(anchored_update(
            &config,
            &scene,
            fixed,
            point,
            &widget.point_label,
        ))
    }

    /// Altitude correction and label range for the activity, shared by layout and its bounds.
    fn label_range(&self, activity: Option<&ActivitySampler>) -> (f64, Option<(f64, f64)>) {
        (
            activity.map_or(0.0, |a| {
                a.elevation.altitude_offset(self.config.starting_altitude_m)
            }),
            activity.and_then(|a| a.elevation.elevation_range()),
        )
    }

    fn state(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        geometry: &ElevationGeometry,
    ) -> ElevationFrameState {
        if geometry.source_point_count == 0 {
            let progress = (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32;
            geometry.state(progress, None, progress)
        } else {
            let activity = &activity.expect("elevation activity prepared").elevation;
            activity.frame_state(geometry, second, self.window, activity.elevation_at(second))
        }
    }

    /// Scene rectangle a render at `second` draws into, without drawing the remaining profile.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity, second)?;
        let (offset, range) = self.label_range(activity);
        Ok(ElevationLayout::measure_bounds(
            &self.config,
            &self.scene,
            &geometry,
            offset,
            range,
            fonts,
        )?
        .into())
    }

    /// Prepares the layout for the geometry at `second`; reports whether it was rebuilt.
    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity, second)?;
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            let (offset, range) = self.label_range(activity);
            self.layout = Some(ElevationLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
                offset,
                range,
                fonts,
                self.preview_raster_scale,
            )?);
        }
        Ok(changed)
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        let frame = layout.frame(self.state(activity, second, &layout.geometry))?;
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on: the drawn geometry, which the widget retains, and
    /// its state. Equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<(usize, ElevationFrameState)> {
        self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        Ok((
            Arc::as_ptr(&layout.geometry) as usize,
            self.state(activity, second, &layout.geometry),
        ))
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let changed = self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        let state = self.state(activity, second, &layout.geometry);
        if !changed && self.last_state == Some(state) {
            return Ok(());
        }
        let frame = layout.frame(state)?;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            frame.bounds,
            Presented::content(frame.content_bounds).rotated(self.config.rotation),
            |canvas| {
                layout.draw(canvas, &frame);
                Ok(())
            },
        )?;
        self.last_state = Some(state);
        Ok(())
    }
}
