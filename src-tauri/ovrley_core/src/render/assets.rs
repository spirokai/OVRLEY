//! Native job preparation: shared widget layouts paired with frame-aligned activity samples.
use crate::{
    activity::schema::{DenseActivityReport, ParsedActivity},
    debug::RenderProfiler,
    error::CoreResult,
    normalize::{ValidatedRenderConfig, ValidatedSceneConfig},
};
use ovrley_widgets::{
    model::types::{DisplayType, MetricKind},
    render::text::FontResolver,
    widgets::{
        diving_profile::{
            geometry::{DivingFrameState, DivingGeometry},
            layout::DivingProfileLayout,
        },
        elevation::{
            geometry::{ElevationFrameState, ElevationGeometry},
            layout::ElevationLayout,
        },
        g_force::{
            format::{derive_max_g, GForceSample},
            layout::{prepare_g_force_cache, GForceCache},
        },
        gauges::{
            frame::GaugeLayout,
            metric::{gauge_frame_state, metric_range, GaugeFrameState},
            validation::ValidatedGauge,
        },
        gradient::validation::ValidatedGradientWidget,
        heading::layout::{prepare_heading_cache, HeadingCache},
        lap_timer::{
            layout::{prepare_lap_timer_layout, LapTimerLayout},
            validation::ValidatedLapTimer,
        },
        lean_angle::layout::{prepare_lean_angle_cache, LeanAngleCache},
        route::{geometry::RouteGeometry, layout::RouteLayout},
        time::validation::ValidatedTimeValue,
        value::{
            format::altitude_offset_m,
            validation::{ValidatedValue, ValidatedValueWidget},
        },
    },
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PreparedStandardText {
    pub validated: ValidatedValueWidget,
    pub altitude_offset_m: f64,
}

#[derive(Clone, Debug)]
pub struct PreparedLapTimer {
    pub validated: ValidatedLapTimer,
    pub layout: LapTimerLayout,
}

#[derive(Clone, Debug)]
pub struct GaugeCache {
    pub layout: GaugeLayout,
    pub frame_states: Vec<GaugeFrameState>,
}

#[derive(Clone, Debug)]
pub struct GForceWidgetCache {
    pub layout: GForceCache,
    pub samples: Vec<GForceSample>,
    pub(crate) source_point_count: usize,
}

/// Every drawable value owns its required layout; configuration never contains optional caches.
#[derive(Clone, Debug)]
pub enum PreparedWidget {
    StandardText(PreparedStandardText),
    TimeText(ValidatedTimeValue),
    Gradient(ValidatedGradientWidget),
    HeadingTape(HeadingCache),
    LeanAngle(LeanAngleCache),
    Gauge(GaugeCache),
    GForce(GForceWidgetCache),
    LapTimer(PreparedLapTimer),
}

/// Identity comes from the canonical validated widget, once during preparation.
#[derive(Clone, Debug)]
pub struct PreparedValue {
    pub metric_kind: MetricKind,
    pub display_type: DisplayType,
    pub widget: PreparedWidget,
}

#[derive(Clone, Debug)]
pub(crate) struct RouteWidgetCache {
    pub layout: RouteLayout,
    pub frame_progress: Vec<f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct ElevationWidgetCache {
    pub layout: ElevationLayout,
    pub frame_states: Vec<ElevationFrameState>,
}

#[derive(Clone, Debug)]
pub(crate) struct DivingProfileCache {
    pub layout: DivingProfileLayout,
    pub frame_states: Vec<DivingFrameState>,
}

#[derive(Clone, Debug)]
pub struct PreparedRenderAssets {
    pub(crate) scene: ValidatedSceneConfig,
    pub(crate) timezone: Option<chrono_tz::Tz>,
    pub values: Vec<PreparedValue>,
    pub(crate) route_caches: Vec<RouteWidgetCache>,
    pub(crate) elevation_caches: Vec<ElevationWidgetCache>,
    pub(crate) diving_caches: Vec<DivingProfileCache>,
    pub(crate) label_count: usize,
}

pub fn prepare_render_assets(
    fonts: &dyn FontResolver,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    profiler: &mut RenderProfiler,
) -> CoreResult<PreparedRenderAssets> {
    let scene = &config.scene;
    let mut route_caches = Vec::new();
    if !config.course_plots.is_empty() {
        let route = activity.route_activity();
        for widget in &config.course_plots {
            route_caches.push(profiler.measure("build_route_cache", || -> CoreResult<_> {
                let window = scene.plot_window(widget.show_full_activity);
                let geometry = Arc::new(RouteGeometry::new(widget, &route.samples(window))?);
                Ok(RouteWidgetCache {
                    layout: RouteLayout::new(widget.clone(), scene.presentation.clone(), geometry)?,
                    frame_progress: dense
                        .frame_elapsed_seconds
                        .iter()
                        .map(|second| route.progress_at(scene.start + second, window))
                        .collect(),
                })
            })?);
        }
    }
    let mut elevation_caches = Vec::new();
    if !config.elevation_plots.is_empty() {
        let elevation = activity.elevation_activity();
        let elevations = ovrley_widgets::widgets::elevation::sampling::preferred_elevation_series(
            &dense.series.barometric_altitude,
            &dense.series.elevation,
        );
        for widget in &config.elevation_plots {
            elevation_caches.push(profiler.measure(
                "build_elevation_cache",
                || -> CoreResult<_> {
                    let window = scene.plot_window(widget.show_full_activity);
                    let geometry = Arc::new(ElevationGeometry::new(
                        widget,
                        scene.presentation.scale,
                        &elevation.samples(window),
                    )?);
                    let frame_states = dense
                        .frame_elapsed_seconds
                        .iter()
                        .enumerate()
                        .map(|(index, elapsed)| {
                            let second = scene.start + elapsed;
                            let sample = if elevations.len() == dense.frame_count {
                                elevations[index]
                            } else {
                                elevation.elevation_at(second)
                            };
                            elevation.frame_state(&geometry, second, window, sample)
                        })
                        .collect();
                    Ok(ElevationWidgetCache {
                        layout: ElevationLayout::new(
                            widget.clone(),
                            scene.presentation.clone(),
                            geometry,
                            elevation.altitude_offset(widget.starting_altitude_m),
                            elevation.elevation_range(),
                            fonts,
                        )?,
                        frame_states,
                    })
                },
            )?);
        }
    }
    let mut diving_caches = Vec::new();
    if !config.diving_profiles.is_empty() {
        let diving = activity.diving_activity();
        for widget in &config.diving_profiles {
            diving_caches.push(profiler.measure(
                "build_diving_profile_cache",
                || -> CoreResult<_> {
                    let window = scene.plot_window(widget.show_full_activity);
                    let geometry = Arc::new(DivingGeometry::new(
                        widget,
                        scene.presentation.scale,
                        &diving.samples(window, widget.width * scene.presentation.scale),
                    ));
                    let frame_states = dense
                        .frame_elapsed_seconds
                        .iter()
                        .map(|elapsed| diving.frame_state(&geometry, scene.start + elapsed, window))
                        .collect();
                    Ok(DivingProfileCache {
                        layout: DivingProfileLayout::new(
                            widget.clone(),
                            scene.presentation.clone(),
                            geometry,
                            diving.depth_range(),
                            fonts,
                        )?,
                        frame_states,
                    })
                },
            )?);
        }
    }
    let altitude = ovrley_widgets::widgets::elevation::sampling::preferred_elevation_series(
        &activity.barometric_altitude,
        &activity.elevation,
    );
    let mut values = Vec::with_capacity(config.values.len());
    for value in &config.values {
        let widget = match value {
            ValidatedValue::StandardText(widget) => {
                PreparedWidget::StandardText(PreparedStandardText {
                    validated: widget.clone(),
                    altitude_offset_m: altitude_offset_m(
                        widget.starting_altitude_m,
                        altitude.iter().flatten().next().copied(),
                    ),
                })
            }
            ValidatedValue::TimeText(widget) => PreparedWidget::TimeText(widget.clone()),
            ValidatedValue::Gradient(widget) => PreparedWidget::Gradient(widget.clone()),
            ValidatedValue::HeadingTape(widget) => {
                PreparedWidget::HeadingTape(profiler.measure("heading_cache.total", || {
                    prepare_heading_cache(widget, &scene.presentation, fonts)
                })?)
            }
            ValidatedValue::LeanAngle(widget) => {
                PreparedWidget::LeanAngle(profiler.measure("lean_angle.prepare", || {
                    prepare_lean_angle_cache(widget, &scene.presentation)
                })?)
            }
            ValidatedValue::LinearGauge(widget) => PreparedWidget::Gauge(prepare_gauge(
                &ValidatedGauge::Linear(widget.clone()),
                activity,
                dense,
                scene,
                fonts,
                profiler,
            )?),
            ValidatedValue::ArcGauge(widget) => PreparedWidget::Gauge(prepare_gauge(
                &ValidatedGauge::Arc(widget.clone()),
                activity,
                dense,
                scene,
                fonts,
                profiler,
            )?),
            ValidatedValue::GForce(widget) => PreparedWidget::GForce(profiler.measure(
                "g_force.prepare",
                || -> CoreResult<_> {
                    let max_g = derive_max_g(
                        activity.g_force_axis(widget.axis_horizontal),
                        activity.g_force_axis(widget.axis_vertical),
                        widget.clip_percentile,
                    );
                    let horizontal = dense.g_force_axis(widget.axis_horizontal);
                    let vertical = dense.g_force_axis(widget.axis_vertical);
                    Ok(GForceWidgetCache {
                        layout: prepare_g_force_cache(widget, &scene.presentation)?,
                        source_point_count: horizontal.len(),
                        samples: (0..dense.frame_count)
                            .map(|index| {
                                GForceSample::new(
                                    widget,
                                    horizontal.get(index).copied().flatten(),
                                    vertical.get(index).copied().flatten(),
                                    max_g,
                                )
                            })
                            .collect(),
                    })
                },
            )?),
            ValidatedValue::LapTimer(widget) => PreparedWidget::LapTimer(PreparedLapTimer {
                validated: widget.clone(),
                layout: profiler.measure("lap_timer.prepare", || {
                    prepare_lap_timer_layout(
                        widget,
                        &scene.presentation,
                        activity.lap_history(),
                        fonts,
                    )
                })?,
            }),
        };
        values.push(PreparedValue {
            metric_kind: value.metric_kind(),
            display_type: value.display_type(),
            widget,
        });
    }
    Ok(PreparedRenderAssets {
        scene: scene.clone(),
        timezone: activity.timezone,
        values,
        route_caches,
        elevation_caches,
        diving_caches,
        label_count: config.labels.len(),
    })
}

fn prepare_gauge(
    widget: &ValidatedGauge,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    fonts: &dyn FontResolver,
    profiler: &mut RenderProfiler,
) -> CoreResult<GaugeCache> {
    let timing = match widget {
        ValidatedGauge::Linear(_) => "gauge.linear.prepare",
        ValidatedGauge::Arc(_) => "gauge.arc.prepare",
    };
    profiler.measure(timing, || -> CoreResult<_> {
        let metric = widget.metric();
        let range = metric_range(dense.full_activity_metric_ranges.get(&metric).copied());
        let offset = altitude_offset_m(
            widget.starting_altitude_m(),
            ovrley_widgets::widgets::elevation::sampling::preferred_elevation_series(
                &activity.barometric_altitude,
                &activity.elevation,
            )
            .iter()
            .flatten()
            .next()
            .copied(),
        );
        Ok(GaugeCache {
            layout: GaugeLayout::new(widget, &scene.presentation, range, offset, fonts)?,
            frame_states: (0..dense.frame_count)
                .map(|index| {
                    gauge_frame_state(
                        widget,
                        dense.metric_sample(metric, index, 0.0),
                        range,
                        offset,
                    )
                })
                .collect(),
        })
    })
}
