//! Time-based depth profile and a prepared visual estimate of the decompression ceiling.
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
use crate::widgets::diving_profile::layout::{
    DivingFrameState, DivingGeometry, DivingProfileLayout, GeometryKey,
};
use crate::widgets::elevation::PointLabelConfig;
use crate::widgets::plot_marker::{frame_resize_marker_factor, MarkerVariant};
use crate::widgets::route::RouteWindow;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::Canvas;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod layout;

/// Presentation defaults are materialized by the host before this ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct DivingProfileConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub ceiling_line_width: f32,
    pub ceiling_line_color: String,
    pub ceiling_line_opacity: f32,
    pub ceiling_area_color: String,
    pub ceiling_area_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_depth_metric: bool,
    pub show_depth_imperial: bool,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub point_label: PointLabelConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedDivingProfile {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub ceiling_line_width: f32,
    pub ceiling_line_color: String,
    pub ceiling_line_opacity: f32,
    pub ceiling_area_color: String,
    pub ceiling_area_opacity: f32,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_depth_metric: bool,
    pub show_depth_imperial: bool,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub label_font: Option<String>,
    pub label_font_size: f32,
    pub label_color: String,
}

pub fn validate_diving_profile(
    plot: &DivingProfileConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> WidgetResult<ValidatedDivingProfile> {
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
    if plot.value != "diving_profile" {
        return Err(WidgetError::Config(format!(
            "{}: must be diving_profile",
            p("value")
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
    Ok(ValidatedDivingProfile {
        x: number(plot.x, "x")?,
        y: number(plot.y, "y")?,
        width: positive(plot.width, "width")?,
        height: positive(plot.height, "height")?,
        rotation: number(plot.rotation, "rotation")?,
        opacity: ranged(plot.opacity, "opacity", 0.0, 1.0)?,
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
        ceiling_line_width: ranged(plot.ceiling_line_width, "ceiling_line_width", 0.0, 100.0)?,
        ceiling_line_color: color(&plot.ceiling_line_color, "ceiling_line_color")?,
        ceiling_line_opacity: opacity(plot.ceiling_line_opacity, "ceiling_line_opacity")?,
        ceiling_area_color: color(&plot.ceiling_area_color, "ceiling_area_color")?,
        ceiling_area_opacity: opacity(plot.ceiling_area_opacity, "ceiling_area_opacity")?,
        marker_variant: MarkerVariant::parse(&plot.marker_variant, &p("marker_variant"))?,
        marker_variant_diameter: positive(plot.marker_variant_diameter, "marker_variant_diameter")?,
        marker_size: positive(plot.marker_size, "marker_size")?,
        marker_color: color(&plot.marker_color, "marker_color")?,
        marker_opacity: opacity(plot.marker_opacity, "marker_opacity")?,
        show_full_activity: plot.show_full_activity,
        show_depth_metric: plot.show_depth_metric,
        show_depth_imperial: plot.show_depth_imperial,
        metric_label_offset_x: number(plot.metric_label_offset_x, "metric_label_offset_x")?,
        metric_label_offset_y: number(plot.metric_label_offset_y, "metric_label_offset_y")?,
        imperial_label_offset_x: number(plot.imperial_label_offset_x, "imperial_label_offset_x")?,
        imperial_label_offset_y: number(plot.imperial_label_offset_y, "imperial_label_offset_y")?,
        label_font: plot.point_label.font.clone().or_else(|| scene.font.clone()),
        label_font_size: positive(plot.point_label.font_size, "point_label.font_size")?,
        label_color: color(&plot.point_label.color, "point_label.color")?,
    })
}

pub type DepthSample = (f32, f64, f32);

#[derive(Debug)]
pub struct DivingSamples {
    pub depth: Vec<Vec<DepthSample>>,
    pub ceiling: Vec<Vec<(f32, f64)>>,
}

pub struct DivingActivity {
    depth: Vec<Vec<(f64, f64)>>,
    ceiling: Vec<CeilingCurve>,
    pub end: f64,
    duration: f64,
    range: Option<(f64, f64)>,
}

/// Monotone cubic Hermite interpolation. Reversals have zero tangent; gaps are separate curves.
pub struct CeilingCurve {
    pub knots: Vec<(f64, f64)>,
    tangents: Vec<f64>,
}

impl CeilingCurve {
    pub fn new(points: &[(f64, f64)]) -> Self {
        let mut knots = vec![points[0]];
        for (index, pair) in points.windows(2).enumerate() {
            if pair[0].1 == pair[1].1 {
                continue;
            }
            // Ease into the first positive step over its duration; keep its recorded value/time exact.
            if pair[0].1 == 0.0 && knots.last().unwrap().0 < pair[0].0 {
                let first = pair[1];
                let end = points[index + 2..]
                    .iter()
                    .find(|p| p.1 != first.1)
                    .unwrap_or(points.last().unwrap())
                    .0;
                let zero_start = knots.last().unwrap().0;
                let onset = (first.0 - (end - first.0)).clamp(zero_start, pair[0].0);
                if onset > zero_start {
                    knots.push((onset, 0.0));
                }
            }
            knots.push(pair[1]);
        }
        if knots.last().unwrap().0 < points.last().unwrap().0 {
            knots.push(*points.last().unwrap());
        }
        let h: Vec<_> = knots.windows(2).map(|p| p[1].0 - p[0].0).collect();
        let slopes: Vec<_> = knots
            .windows(2)
            .zip(&h)
            .map(|(p, h)| (p[1].1 - p[0].1) / h)
            .collect();
        let mut tangents = vec![0.0; knots.len()];
        if slopes.len() == 1 {
            tangents.fill(slopes[0]);
        } else if slopes.len() > 1 {
            let endpoint = |h0: f64, h1: f64, d0: f64, d1: f64| {
                let m = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
                if m.signum() != d0.signum() {
                    0.0
                } else if d0.signum() != d1.signum() && m.abs() > 3.0 * d0.abs() {
                    3.0 * d0
                } else {
                    m
                }
            };
            tangents[0] = endpoint(h[0], h[1], slopes[0], slopes[1]);
            let last = slopes.len() - 1;
            tangents[last + 1] = endpoint(h[last], h[last - 1], slopes[last], slopes[last - 1]);
            for i in 1..knots.len() - 1 {
                let (a, b) = (slopes[i - 1], slopes[i]);
                if a * b > 0.0 {
                    let (w1, w2) = (2.0 * h[i] + h[i - 1], h[i] + 2.0 * h[i - 1]);
                    tangents[i] = (w1 + w2) / (w1 / a + w2 / b);
                }
            }
        }
        Self { knots, tangents }
    }

    pub fn at(&self, second: f64) -> Option<f64> {
        if second < self.knots[0].0 || second > self.knots.last().unwrap().0 {
            return None;
        }
        if self.knots.len() == 1 {
            return Some(self.knots[0].1);
        }
        let i = self
            .knots
            .partition_point(|p| p.0 <= second)
            .clamp(1, self.knots.len() - 1)
            - 1;
        let (a, b) = (self.knots[i], self.knots[i + 1]);
        let h = b.0 - a.0;
        let u = (second - a.0) / h;
        let value = (2.0 * u.powi(3) - 3.0 * u.powi(2) + 1.0) * a.1
            + (u.powi(3) - 2.0 * u.powi(2) + u) * h * self.tangents[i]
            + (-2.0 * u.powi(3) + 3.0 * u.powi(2)) * b.1
            + (u.powi(3) - u.powi(2)) * h * self.tangents[i + 1];
        Some(value)
    }
}

fn continuous_runs(times: &[f64], values: &[Option<f64>]) -> Vec<Vec<(f64, f64)>> {
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for (&time, value) in times.iter().zip(values) {
        if let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) {
            run.push((time, value));
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

impl DivingActivity {
    pub fn new(times: &[f64], depth: &[Option<f64>], ceiling: &[Option<f64>], end: f64) -> Self {
        let depth = continuous_runs(times, depth);
        let range = depth.iter().flatten().fold(None, |range, &(_, value)| {
            Some(range.map_or((value, value), |(min, max): (f64, f64)| {
                (min.min(value), max.max(value))
            }))
        });
        Self {
            depth,
            ceiling: continuous_runs(times, ceiling)
                .iter()
                .map(|run| CeilingCurve::new(run))
                .collect(),
            end,
            duration: times.last().copied().unwrap_or(0.0).max(1e-9),
            range,
        }
    }

    pub fn depth_range(&self) -> Option<(f64, f64)> {
        self.range
    }

    pub fn frame_state(
        &self,
        geometry: &crate::widgets::diving_profile::layout::DivingGeometry,
        second: f64,
        window: Option<RouteWindow>,
    ) -> crate::widgets::diving_profile::layout::DivingFrameState {
        let mut state = geometry.state(self.progress_at(second, window), self.depth_at(second));
        if state.depth_m.is_none() {
            state.marker = None;
        }
        state
    }

    pub fn depth_at(&self, second: f64) -> Option<f64> {
        if second < 0.0 || second > self.end {
            return None;
        }
        self.depth
            .iter()
            .find(|run| second >= run[0].0 && second <= run.last().unwrap().0)
            .and_then(|run| interpolate_points(run, second))
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let (start, end) = self.bounds(window);
        ((second - start) / (end - start)).clamp(0.0, 1.0) as f32
    }

    fn bounds(&self, window: Option<RouteWindow>) -> (f64, f64) {
        window.map_or((0.0, self.duration), |w| (w.start, w.end))
    }

    /// Interpolate crop boundaries within observed runs; never bridge a telemetry gap.
    pub fn samples(&self, window: Option<RouteWindow>, width: f32) -> DivingSamples {
        let (start, end) = self.bounds(window);
        let fraction = |t: f64| ((t - start) / (end - start)) as f32;
        let depth = self
            .depth
            .iter()
            .filter_map(|run| {
                let (a, b) = (start.max(run[0].0), end.min(run.last().unwrap().0));
                if a > b {
                    return None;
                }
                let mut samples = vec![(
                    fraction(a),
                    interpolate_points(run, a).unwrap(),
                    fraction(a),
                )];
                samples.extend(
                    run.iter()
                        .filter(|p| p.0 > a && p.0 < b)
                        .map(|&(t, d)| (fraction(t), d, fraction(t))),
                );
                if b > a {
                    samples.push((
                        fraction(b),
                        interpolate_points(run, b).unwrap(),
                        fraction(b),
                    ));
                }
                Some(samples)
            })
            .collect();
        let ceiling = self
            .ceiling
            .iter()
            .filter_map(|curve| {
                let (a, b) = (
                    start.max(curve.knots[0].0),
                    end.min(curve.knots.last().unwrap().0),
                );
                if a > b {
                    return None;
                }
                let count = ((fraction(b) - fraction(a)) * width).ceil().max(1.0) as usize;
                let mut times: Vec<_> = (0..=count)
                    .map(|i| a + (b - a) * i as f64 / count as f64)
                    .collect();
                times.extend(
                    curve
                        .knots
                        .iter()
                        .filter(|p| p.0 > a && p.0 < b)
                        .map(|p| p.0),
                );
                times.sort_by(f64::total_cmp);
                times.dedup();
                Some(
                    times
                        .into_iter()
                        .map(|t| (fraction(t), curve.at(t).unwrap()))
                        .collect(),
                )
            })
            .collect();
        DivingSamples { depth, ceiling }
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

pub fn resize_diving_profile(
    config: &mut ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    size: [f32; 2],
    fixed: [f32; 2],
) -> WidgetResult<()> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid diving profile resize size".into(),
        ));
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
    config: &ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    fixed: [f32; 2],
    point: [f32; 2],
    point_label: &PointLabelConfig,
) -> ResizeUpdate {
    let bounds = DivingProfileLayout::measure_content(config, scene);
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

pub struct PreviewDivingProfile {
    config: ValidatedDivingProfile,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    pub prepared_geometry: Option<Arc<DivingGeometry>>,
    layout: Option<DivingProfileLayout>,
    last_state: Option<DivingFrameState>,
    pub tile: Tile,
}

impl PreviewDivingProfile {
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
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_diving_profile(&widget, 0, &scene)?;
        Self::from_validated(config, scene, &host)
    }

    pub fn from_validated(
        config: ValidatedDivingProfile,
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
            prepared_geometry: None,
            layout: None,
            last_state: None,
            tile: Tile::default(),
        })
    }

    pub fn update(
        &mut self,
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        transient: bool,
    ) -> WidgetResult<()> {
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let (window, preview_duration) = Self::timing(widget.show_full_activity, &host)?;
        let preview_raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_diving_profile(&widget, 0, &scene)?;
        if window != self.window
            || (!transient
                && self
                    .prepared_geometry
                    .as_ref()
                    .is_some_and(|g| g.key != GeometryKey::new(&config, scene.scale)))
        {
            self.invalidate_activity();
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
        self.prepared_geometry = None;
        self.layout = None;
        self.last_state = None;
    }

    fn geometry_at(&mut self, activity: Option<&ActivitySampler>) -> Arc<DivingGeometry> {
        self.prepared_geometry
            .get_or_insert_with(|| {
                Arc::new(match activity {
                    Some(a) => DivingGeometry::new(
                        &self.config,
                        self.scene.scale,
                        &a.diving
                            .samples(self.window, self.config.width * self.scene.scale),
                    ),
                    None => DivingGeometry::placeholder(&self.config, self.scene.scale),
                })
            })
            .clone()
    }

    pub fn resize(
        &mut self,
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        size: [f32; 2],
        gesture: ResizeGesture,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<ResizeUpdate> {
        let ResizeGesture {
            fixed,
            point,
            commit,
            ..
        } = gesture;
        let scene = validate_scene_presentation(scene)?;
        let mut config = validate_diving_profile(&widget, 0, &scene)?;
        self.geometry_at(activity);
        resize_diving_profile(&mut config, &scene, size, fixed)?;
        if commit {
            self.prepared_geometry = Some(Arc::new(match activity {
                Some(a) => DivingGeometry::new(
                    &config,
                    scene.scale,
                    &a.diving.samples(self.window, config.width * scene.scale),
                ),
                None => DivingGeometry::placeholder(&config, scene.scale),
            }));
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

    fn state(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        geometry: &DivingGeometry,
    ) -> DivingFrameState {
        match activity {
            Some(a) => a.diving.frame_state(geometry, second, self.window),
            None => geometry.state(
                (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32,
                None,
            ),
        }
    }

    /// Scene rectangle a render at `second` draws into, without drawing the remaining profile.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity);
        let range = activity.and_then(|a| a.diving.depth_range());
        Ok(
            DivingProfileLayout::measure_bounds(
                &self.config,
                &self.scene,
                &geometry,
                range,
                fonts,
            )?
            .into(),
        )
    }

    /// Prepares the layout for the activity's geometry; reports whether it was rebuilt.
    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity);
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            self.layout = Some(DivingProfileLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
                activity.and_then(|a| a.diving.depth_range()),
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
        self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        let frame = layout.frame(self.state(activity, second, &layout.geometry))?;
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on; equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<DivingFrameState> {
        self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        Ok(self.state(activity, second, &layout.geometry))
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let changed = self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
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

#[cfg(test)]
mod tests {
    use super::{
        validate_diving_profile, CeilingCurve, DivingActivity, DivingProfileConfig, DivingSamples,
    };
    use crate::model::scene::{validate_scene_presentation, ScenePresentationConfig};
    use crate::widgets::diving_profile::layout::DivingGeometry;
    use crate::widgets::route::RouteWindow;

    fn config() -> super::ValidatedDivingProfile {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/standard-widgets.json"
        )))
        .unwrap();
        let widget = manifest["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["type"] == "diving_profile")
            .unwrap();
        let config: DivingProfileConfig =
            serde_json::from_value(widget["defaults"].clone()).unwrap();
        let scene: ScenePresentationConfig = serde_json::from_value(serde_json::json!({
        "width": 1920, "height": 1080, "scale": 1, "opacity": 1, "font": "Arial.ttf", "shadow_strength": 0,
        "shadow_distance": 0, "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000"
    })).unwrap();
        validate_diving_profile(&config, 0, &validate_scene_presentation(scene).unwrap()).unwrap()
    }

    #[test]
    fn ceiling_reaches_new_stop_at_transition_without_overshoot_and_preserves_surface_intervals() {
        let curve = CeilingCurve::new(&[
            (0.0, 9.0),
            (9.0, 9.0),
            (10.0, 6.0),
            (14.0, 6.0),
            (15.0, 3.0),
            (19.0, 3.0),
            (20.0, 0.0),
            (25.0, 0.0),
        ]);
        for (time, depth) in [
            (0.0, 9.0),
            (10.0, 6.0),
            (15.0, 3.0),
            (20.0, 0.0),
            (25.0, 0.0),
        ] {
            assert_eq!(curve.at(time), Some(depth));
        }
        let values: Vec<_> = (0..=250)
            .map(|i| curve.at(i as f64 / 10.0).unwrap())
            .collect();
        assert!(values.windows(2).all(|p| p[0] >= p[1] && p[1] >= 0.0));
        let reversal = CeilingCurve::new(&[
            (0.0, 0.0),
            (9.0, 0.0),
            (10.0, 6.0),
            (15.0, 3.0),
            (20.0, 9.0),
            (25.0, 0.0),
        ]);
        assert_eq!(reversal.at(5.0), Some(0.0));
        assert!(reversal.at(8.0).unwrap() > 0.0);
        assert_eq!(reversal.at(10.0), Some(6.0));
        for pair in reversal.knots.windows(2) {
            for i in 0..=100 {
                let time = pair[0].0 + (pair[1].0 - pair[0].0) * i as f64 / 100.0;
                let depth = reversal.at(time).unwrap();
                assert!(
                    depth >= pair[0].1.min(pair[1].1) - 1e-9
                        && depth <= pair[0].1.max(pair[1].1) + 1e-9
                );
            }
        }
    }

    #[test]
    fn smoothing_and_detail_change_depth_geometry_and_invalidate_its_cache_key() {
        let mut config = config();
        config.width = 40.0;
        config.simplify_tolerance_px = 0.0;
        config.target_density = 1.5;
        let samples = DivingSamples {
            depth: vec![(0..=100)
                .map(|i| {
                    let progress = i as f32 / 100.0;
                    (progress, 10.0 + (i as f64 * 0.4).sin(), progress)
                })
                .collect()],
            ceiling: Vec::new(),
        };
        let detailed = DivingGeometry::new(&config, 1.0, &samples);
        config.simplify_tolerance_px = 4.0;
        let smooth = DivingGeometry::new(&config, 1.0, &samples);
        assert!(smooth.segments[0].points.len() < detailed.segments[0].points.len());
        assert_ne!(smooth.key, detailed.key);
        config.simplify_tolerance_px = 0.0;
        config.target_density = 0.25;
        let coarse = DivingGeometry::new(&config, 1.0, &samples);
        assert!(coarse.segments[0].points.len() < detailed.segments[0].points.len());
        assert_ne!(coarse.key, detailed.key);
    }

    #[test]
    fn time_axis_crop_and_missing_telemetry_keep_surface_and_profile_gaps() {
        let activity = DivingActivity::new(
            &[0.0, 10.0, 20.0, 30.0, 40.0],
            &[Some(0.0), Some(20.0), None, Some(10.0), Some(0.0)],
            &[],
            40.0,
        );
        let config = config();
        let samples = activity.samples(None, config.width);
        let geometry = DivingGeometry::new(&config, 1.0, &samples);
        assert_eq!(geometry.segments.len(), 2);
        assert!(geometry.paths.lock().unwrap().ceiling_area.is_empty());
        let first = activity.frame_state(&geometry, 0.0, None);
        assert_eq!(first.marker, Some((0.0, 0.0)));
        let state = activity.frame_state(&geometry, 10.0, None);
        assert_eq!(state.marker, Some((config.width * 0.25, config.height)));
        assert!(activity.frame_state(&geometry, 20.0, None).marker.is_none());
        let window = Some(RouteWindow {
            start: 5.0,
            end: 35.0,
        });
        let cropped = activity.samples(window, config.width);
        assert_eq!(cropped.depth[0][0], (0.0, 10.0, 0.0));
        assert_eq!(*cropped.depth[1].last().unwrap(), (1.0, 5.0, 1.0));
        let mut exaggerated = config.clone();
        exaggerated.y_scale = 2.0;
        assert_eq!(
            DivingGeometry::new(&exaggerated, 1.0, &samples).segments[0].points[0].1,
            0.0
        );
        let ceiling_gaps = DivingActivity::new(
            &[0.0, 10.0, 20.0, 30.0, 40.0],
            &[],
            &[Some(6.0), Some(3.0), None, Some(3.0), Some(0.0)],
            40.0,
        );
        assert_eq!(ceiling_gaps.samples(None, config.width).ceiling.len(), 2);
    }
}
