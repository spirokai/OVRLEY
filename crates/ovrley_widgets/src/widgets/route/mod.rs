//! Route projection, prepared vector geometry and drawing shared by both hosts.
use crate::activity::ActivitySampler;
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_f32, require_hex_color, require_non_negative_f32, require_opacity, require_positive_f32,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::series::interpolate_points;
use crate::widgets::plot_marker::MarkerVariant;
use crate::widgets::route::geometry::{GeometryKey, RouteGeometry, RouteLayout, RouteSample};
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::Canvas;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod geometry;

/// Effective course config; every output-affecting field is required at ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CoursePlotConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub show_full_activity: bool,
    pub completed_line_width: f32,
    pub remaining_line_width: f32,
    #[serde(flatten)]
    pub path: RoutePathConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The line colour and marker fields shared by every family that draws a route. Each family owns
/// its line widths.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RoutePathConfig {
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedRoutePlot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub remaining_line_width: f32,
    pub path: ValidatedRoutePath,
    pub show_full_activity: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedRoutePath {
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
}

pub fn validate_route_plot(
    plot: &CoursePlotConfig,
    index: usize,
) -> WidgetResult<ValidatedRoutePlot> {
    let p = |f: &str| format!("plots[{index}].{f}");
    if plot.value != "course" {
        return Err(WidgetError::Config(format!(
            "{}: must be course",
            p("value")
        )));
    }

    let simplify_tolerance_px = require_non_negative_f32(
        Some(plot.simplify_tolerance_px),
        &p("simplify_tolerance_px"),
    )?;
    let target_density = require_f32(Some(plot.target_density), &p("target_density"))?;
    if !(0.1..=2.0).contains(&target_density) {
        return Err(WidgetError::Config(format!(
            "{}: must be between 0.1 and 2.0",
            p("target_density")
        )));
    }

    Ok(ValidatedRoutePlot {
        x: require_f32(Some(plot.x), &p("x"))?,
        y: require_f32(Some(plot.y), &p("y"))?,
        width: require_positive_f32(Some(plot.width), &p("width"))?,
        height: require_positive_f32(Some(plot.height), &p("height"))?,
        rotation: require_f32(Some(plot.rotation), &p("rotation"))?,
        simplify_tolerance_px,
        target_density,
        completed_line_width: require_positive_f32(
            Some(plot.completed_line_width),
            &p("completed_line_width"),
        )?,
        remaining_line_width: require_positive_f32(
            Some(plot.remaining_line_width),
            &p("remaining_line_width"),
        )?,
        path: validate_route_path(&plot.path, &p)?,
        show_full_activity: plot.show_full_activity,
    })
}

/// The one validation of the fields every route-drawing family shares.
pub fn validate_route_path(
    path: &RoutePathConfig,
    p: &dyn Fn(&str) -> String,
) -> WidgetResult<ValidatedRoutePath> {
    Ok(ValidatedRoutePath {
        completed_line_color: require_hex_color(
            Some(&path.completed_line_color),
            &p("completed_line_color"),
        )?,
        completed_line_opacity: require_opacity(
            Some(path.completed_line_opacity),
            &p("completed_line_opacity"),
        )?,
        remaining_line_color: require_hex_color(
            Some(&path.remaining_line_color),
            &p("remaining_line_color"),
        )?,
        remaining_line_opacity: require_opacity(
            Some(path.remaining_line_opacity),
            &p("remaining_line_opacity"),
        )?,
        marker_variant: MarkerVariant::parse_on_route(&path.marker_variant, &p("marker_variant"))?,
        marker_variant_diameter: require_positive_f32(
            Some(path.marker_variant_diameter),
            &p("marker_variant_diameter"),
        )?,
        marker_size: require_positive_f32(Some(path.marker_size), &p("marker_size"))?,
        marker_color: require_hex_color(Some(&path.marker_color), &p("marker_color"))?,
        marker_opacity: require_opacity(Some(path.marker_opacity), &p("marker_opacity"))?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteWindow {
    pub start: f64,
    pub end: f64,
}

impl RouteWindow {
    /// `absolute` progress at `second` relative to this window, from the activity's `progress`
    /// samples; elapsed time stands in when they do not span the window.
    pub(crate) fn relative_progress(
        self,
        progress: &[(f64, f64)],
        absolute: f64,
        second: f64,
    ) -> f64 {
        interpolate_points(progress, self.start)
            .zip(interpolate_points(progress, self.end))
            .filter(|(start, end)| end > start)
            .map_or(
                (second - self.start) / (self.end - self.start),
                |(start, end)| (absolute - start) / (end - start),
            )
    }
}

/// One GPS sample in geographic coordinates, unlike the equirectangular `RouteSample`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackSample {
    pub second: f64,
    pub lat: f64,
    pub lon: f64,
}

pub struct RouteActivity {
    times: Vec<f64>,
    course: Vec<(Option<f64>, Option<f64>)>,
    latitude: Vec<(f64, f64)>,
    longitude: Vec<(f64, f64)>,
    progress: Vec<(f64, f64)>,
    full: Arc<Vec<RouteSample>>,
    pub end: f64,
}

impl RouteActivity {
    pub fn new(
        times: &[f64],
        course: &[(Option<f64>, Option<f64>)],
        progress: &[f64],
        end: f64,
    ) -> Self {
        let full = Arc::new(project_course_samples(course, progress));
        let column = |component: usize| {
            times
                .iter()
                .copied()
                .zip(course)
                .filter_map(|(time, p)| {
                    let value = if component == 0 { p.0 } else { p.1 };
                    value.filter(|v| v.is_finite()).map(|value| (time, value))
                })
                .collect()
        };
        Self {
            times: times.to_vec(),
            course: course.to_vec(),
            end,
            full,
            latitude: column(0),
            longitude: column(1),
            progress: times
                .iter()
                .copied()
                .zip(progress.iter().copied())
                .filter(|p| p.1.is_finite())
                .collect(),
        }
    }

    pub fn samples(&self, window: Option<RouteWindow>) -> Arc<Vec<RouteSample>> {
        let Some(window) = window else {
            return self.full.clone();
        };
        let start = self.times.partition_point(|value| *value <= window.start);
        let end = self
            .times
            .partition_point(|value| *value < window.end)
            .max(start);
        let coordinate = |points: &[(f64, f64)], second| {
            points
                .first()
                .zip(points.last())
                .filter(|(a, b)| second >= a.0 && second <= b.0)
                .and_then(|_| interpolate_points(points, second))
        };
        let boundary = |second| {
            (
                coordinate(&self.latitude, second),
                coordinate(&self.longitude, second),
            )
        };
        let mut course = Vec::with_capacity(end - start + 2);
        let mut progress = Vec::with_capacity(end - start + 2);
        course.push(boundary(window.start));
        progress.push(interpolate_points(&self.progress, window.start));
        for index in start..end {
            course.push(self.course.get(index).copied().unwrap_or((None, None)));
            progress.push(interpolate_points(&self.progress, self.times[index]));
        }
        course.push(boundary(window.end));
        progress.push(interpolate_points(&self.progress, window.end));
        let first = progress.iter().find_map(|value| *value);
        let last = progress.iter().rev().find_map(|value| *value);
        let span = first.zip(last).filter(|(a, b)| b > a);
        let normalized = progress
            .iter()
            .enumerate()
            .map(|(index, value)| {
                span.and_then(|(a, b)| value.map(|v| ((v - a) / (b - a)).clamp(0.0, 1.0)))
                    .unwrap_or(index as f64 / (progress.len() - 1) as f64)
            })
            .collect::<Vec<_>>();
        Arc::new(project_course_samples(&course, &normalized))
    }

    /// Every GPS sample in `window`, with its edges interpolated, as `samples` selects them.
    pub fn track_samples(&self, window: Option<RouteWindow>) -> Vec<TrackSample> {
        let inner = |start: f64, end: f64| {
            self.times
                .iter()
                .copied()
                .filter(move |second| *second > start && *second < end)
        };
        let seconds: Vec<f64> = match window {
            None => self.times.clone(),
            Some(window) => std::iter::once(window.start)
                .chain(inner(window.start, window.end))
                .chain(std::iter::once(window.end))
                .collect(),
        };
        let coordinate = |points: &[(f64, f64)], second| {
            points
                .first()
                .zip(points.last())
                .filter(|(a, b)| second >= a.0 && second <= b.0)
                .and_then(|_| interpolate_points(points, second))
        };
        seconds
            .into_iter()
            .filter_map(|second| {
                let lat = coordinate(&self.latitude, second)?;
                let lon = coordinate(&self.longitude, second)?;
                Some(TrackSample { second, lat, lon })
            })
            .collect()
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let absolute = interpolate_points(&self.progress, second)
            .unwrap_or_else(|| (second / self.end.max(1e-9)).clamp(0.0, 1.0));
        window
            .map_or(absolute, |window| {
                window.relative_progress(&self.progress, absolute, second)
            })
            .clamp(0.0, 1.0) as f32
    }
}

/// Native and preview share equirectangular projection and missing-GPS handling.
pub fn project_course_samples(
    course: &[(Option<f64>, Option<f64>)],
    progress: &[f64],
) -> Vec<RouteSample> {
    let valid = course
        .iter()
        .enumerate()
        .filter_map(|(index, (lat, lon))| {
            lat.zip(*lon)
                .filter(|(lat, lon)| lat.is_finite() && lon.is_finite())
                .map(|(lat, lon)| (index, lat, lon))
        })
        .collect::<Vec<_>>();
    if valid.is_empty() {
        return Vec::new();
    }
    let latitude = valid.iter().map(|p| p.1).sum::<f64>() / valid.len() as f64;
    valid
        .into_iter()
        .map(|(index, lat, lon)| RouteSample {
            point: (lon * latitude.to_radians().cos(), lat),
            progress01: progress
                .get(index)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or_else(|| index as f64 / course.len().saturating_sub(1).max(1) as f64)
                .clamp(0.0, 1.0) as f32,
        })
        .collect()
}

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub completed_line_width: f32,
    pub remaining_line_width: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
}

pub fn scale_route(config: &mut ValidatedRoutePlot, factor: f32) {
    // Preserve the existing minimum widget size and a uniform aspect ratio.
    let factor = factor.max(8.0 / config.width.min(config.height));
    config.width *= factor;
    config.height *= factor;
    config.completed_line_width *= factor;
    config.remaining_line_width *= factor;
    config.path.marker_size *= factor;
    config.path.marker_variant_diameter *= factor;
}

pub fn resize_route(
    config: &ValidatedRoutePlot,
    scene: &ValidatedScenePresentation,
    geometry: &RouteGeometry,
    fixed: [f32; 2],
    point: [f32; 2],
) -> ResizeUpdate {
    let bounds = RouteLayout::measure_content(config, scene, geometry);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let local_x = bounds.left + bounds.width() * fixed[0];
    let local_y = bounds.top + bounds.height() * fixed[1];
    ResizeUpdate {
        x: point[0] - (local_x * cos - local_y * sin),
        y: point[1] - (local_x * sin + local_y * cos),
        width: config.width,
        height: config.height,
        completed_line_width: config.completed_line_width,
        remaining_line_width: config.remaining_line_width,
        marker_size: config.path.marker_size,
        marker_variant_diameter: config.path.marker_variant_diameter,
    }
}

pub struct PreviewRoute {
    config: ValidatedRoutePlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    samples: Option<Arc<Vec<RouteSample>>>,
    pub prepared_geometry: Option<Arc<RouteGeometry>>,
    placeholder_geometry: Option<Arc<RouteGeometry>>,
    layout: Option<RouteLayout>,
    placeholder: bool,
    last_progress: Option<f32>,
    pub tile: Tile,
}

impl PreviewRoute {
    pub fn new(
        widget: CoursePlotConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_route_plot(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(config, validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: ValidatedRoutePlot,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        Ok(Self {
            window: host.plot_window(config.show_full_activity),
            preview_duration: host.placeholder_duration()?,
            preview_raster_scale: host.raster_scale,
            config,
            scene,
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
            layout: None,
            placeholder: true,
            last_progress: None,
            tile: Tile::default(),
        })
    }

    pub fn update(
        &mut self,
        widget: CoursePlotConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        transient: bool,
    ) -> WidgetResult<()> {
        let config = validate_route_plot(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let window = host.plot_window(config.show_full_activity);
        let preview_duration = host.placeholder_duration()?;
        let preview_raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        if !transient
            && self
                .prepared_geometry
                .as_ref()
                .is_some_and(|geometry| geometry.key != GeometryKey::new(&config))
        {
            self.prepared_geometry = None;
            self.placeholder_geometry = None;
            self.layout = None;
        }
        if let Some(layout) = self.layout.as_mut() {
            // Prepared paths ignore x/y, but the cached shadow raster sits on the position's
            // sub-pixel grid, so a move rebuilds the layout from them.
            if layout.config != config
                || self.preview_raster_scale != preview_raster_scale
                || self.scene.scale != scene.scale
                || self.scene.opacity != scene.opacity
                || self.scene.shadow_color != scene.shadow_color
                || self.scene.shadow_strength != scene.shadow_strength
                || self.scene.shadow_distance != scene.shadow_distance
            {
                self.layout = None;
            }
        }
        self.config = config;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.preview_raster_scale = preview_raster_scale;
        self.last_progress = None;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.samples = None;
        self.prepared_geometry = None;
        self.placeholder_geometry = None;
        self.layout = None;
        self.last_progress = None;
    }

    fn prepare_geometry(&mut self, activity: Option<&ActivitySampler>) -> WidgetResult<()> {
        if self.prepared_geometry.is_some() {
            return Ok(());
        }
        if self.samples.is_none() {
            self.samples = Some(
                activity
                    .map(|activity| activity.route.samples(self.window))
                    .unwrap_or_else(|| Arc::new(Vec::new())),
            );
        }
        let samples = self.samples.as_ref().expect("route samples prepared");
        self.placeholder = samples.len() < 2;
        self.prepared_geometry = Some(Arc::new(if self.placeholder {
            RouteGeometry::placeholder(&self.config)
        } else {
            RouteGeometry::new(&self.config, samples)?
        }));
        Ok(())
    }

    fn geometry_at(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<Arc<RouteGeometry>> {
        self.prepare_geometry(activity)?;
        if !self.placeholder
            && activity.is_some_and(|activity| second < 0.0 || second > activity.route.end)
        {
            if self.placeholder_geometry.is_none() {
                self.placeholder_geometry =
                    Some(Arc::new(RouteGeometry::placeholder(&self.config)));
            }
            Ok(self
                .placeholder_geometry
                .as_ref()
                .expect("placeholder route prepared")
                .clone())
        } else {
            Ok(self
                .prepared_geometry
                .as_ref()
                .expect("route geometry prepared")
                .clone())
        }
    }

    fn progress(&self, activity: Option<&ActivitySampler>, second: f64, placeholder: bool) -> f32 {
        if placeholder {
            (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32
        } else {
            activity
                .expect("route telemetry activity")
                .route
                .progress_at(second, self.window)
        }
    }

    pub fn resize(
        &mut self,
        widget: CoursePlotConfig,
        scene: ScenePresentationConfig,
        factor: f32,
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
        let mut config = validate_route_plot(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        self.prepare_geometry(activity)?;
        scale_route(&mut config, factor);
        if commit {
            let geometry = if self.placeholder {
                RouteGeometry::placeholder(&config)
            } else {
                RouteGeometry::new(
                    &config,
                    self.samples.as_ref().expect("route samples prepared"),
                )?
            };
            self.prepared_geometry = Some(Arc::new(geometry));
            if self.placeholder_geometry.is_some() {
                self.placeholder_geometry = Some(Arc::new(RouteGeometry::placeholder(&config)));
            }
            self.layout = None;
            self.last_progress = None;
        }
        let geometry = self.geometry_at(activity, second)?;
        Ok(resize_route(&config, &scene, &geometry, fixed, point))
    }

    /// Scene rectangle a render at `second` draws into; marker overflow is reserved for the whole path.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity, second)?;
        Ok(RouteLayout::measure(&self.config, &self.scene, &geometry).into())
    }

    /// Prepares the layout for the geometry at `second`; reports whether it was rebuilt.
    fn layout(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity, second)?;
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            self.layout = Some(RouteLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
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
    ) -> WidgetResult<TileBounds> {
        self.layout(activity, second)?;
        let layout = self.layout.as_ref().expect("route layout prepared");
        let progress = self.progress(activity, second, layout.geometry.source_point_count == 0);
        let frame = layout.frame(progress);
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on: the drawn geometry, which the widget retains, and
    /// the marker progress. Equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<(usize, f32)> {
        self.layout(activity, second)?;
        let geometry = &self
            .layout
            .as_ref()
            .expect("route layout prepared")
            .geometry;
        let progress = self.progress(activity, second, geometry.source_point_count == 0);
        Ok((Arc::as_ptr(geometry) as usize, progress))
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let layout_changed = self.layout(activity, second)?;
        let geometry = &self
            .layout
            .as_ref()
            .expect("route layout prepared")
            .geometry;
        let progress = self.progress(activity, second, geometry.source_point_count == 0);
        if !layout_changed && self.last_progress == Some(progress) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("route layout prepared");
        let frame = layout.frame(progress);
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
        self.last_progress = Some(progress);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::route::geometry::{simplify_route_samples, RouteSample};

    fn sample(x: f32, y: f32, progress01: f32) -> RouteSample {
        RouteSample {
            point: (x as f64, y as f64),
            progress01,
        }
    }

    #[test]
    fn straight_line_keeps_endpoints() {
        let points = vec![sample(0.0, 0.0, 0.0), sample(100.0, 100.0, 1.0)];
        let simplified = simplify_route_samples(&points, 0.0);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn collinear_removes_middle() {
        let points = vec![
            sample(0.0, 0.0, 0.0),
            sample(50.0, 50.0, 0.5),
            sample(100.0, 100.0, 1.0),
        ];
        let simplified = simplify_route_samples(&points, 0.0);
        assert_eq!(simplified.len(), 2);
        assert_eq!(simplified[0].point, (0.0, 0.0));
        assert_eq!(simplified[1].point, (100.0, 100.0));
    }

    #[test]
    fn empty_returns_empty() {
        let points: Vec<RouteSample> = vec![];
        let simplified = simplify_route_samples(&points, 1.0);
        assert!(simplified.is_empty());
    }

    #[test]
    fn single_point_returns_same() {
        let points = vec![sample(42.0, 42.0, 0.0)];
        let simplified = simplify_route_samples(&points, 1.0);
        assert_eq!(simplified.len(), 1);
    }

    #[test]
    fn preserves_peak() {
        let points = vec![
            sample(0.0, 0.0, 0.0),
            sample(50.0, 100.0, 0.5),
            sample(100.0, 0.0, 1.0),
        ];
        let simplified = simplify_route_samples(&points, 0.0);
        assert_eq!(simplified.len(), 3);
        let simplified = simplify_route_samples(&points, 101.0);
        assert_eq!(simplified.len(), 2);
    }
}
