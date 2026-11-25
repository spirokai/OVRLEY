//! Time-rolling line chart of several metrics, each scaled to its own activity range.
use crate::activity::ActivitySampler;
use crate::model::catalog::{chart_limits, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::telemetry::is_plottable_metric;
use crate::model::validation::{
    require_f32, require_hex_color, require_percentage, require_positive_f32,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle, TextPaints};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::widgets::plot_marker::{MarkerVariant, PlotMarker};
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::paint::Style;
use skia_safe::{
    gradient, Canvas, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, Rect, TextBlob,
    TileMode,
};
use std::collections::BTreeMap;

/// One plotted metric. `label` is legend text the host localizes at render time; templates omit it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ChartSeriesConfig {
    pub metric: MetricKind,
    pub label: Option<String>,
    pub color: String,
}

/// Presentation defaults are materialized by the host before this ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RollingChartConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    /// Plot area; the legend row sits below it.
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub window_seconds: f32,
    pub line_width: f32,
    pub line_opacity: f32,
    pub line_curve: f32,
    pub area_fill: bool,
    pub area_gradient: bool,
    pub area_opacity: f32,
    /// Drawn back to front.
    pub series: Vec<ChartSeriesConfig>,
    pub show_marker: bool,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_opacity: f32,
    pub show_legend: bool,
    pub legend_font: String,
    pub legend_font_size: f32,
    pub legend_color: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedChartSeries {
    pub metric: MetricKind,
    pub label: Option<String>,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedChartMarker {
    pub variant: MarkerVariant,
    pub size: f32,
    pub variant_diameter: f32,
    /// 0..=1.
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedChartLegend {
    pub font: String,
    pub font_size: f32,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedRollingChart {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub window_seconds: f64,
    pub line_width: f32,
    /// 0..=1.
    pub line_opacity: f32,
    /// 0..=2: up to 1 the share of each half segment a corner cut spans; beyond, how far
    /// samples also pull toward their neighbours first.
    pub line_curve: f32,
    /// Area opacity in 0..=1; none without an area fill.
    pub area_opacity: Option<f32>,
    pub area_gradient: bool,
    pub series: Vec<ValidatedChartSeries>,
    pub marker: Option<ValidatedChartMarker>,
    pub legend: Option<ValidatedChartLegend>,
}

pub fn validate_rolling_chart(
    chart: &RollingChartConfig,
    index: usize,
) -> WidgetResult<ValidatedRollingChart> {
    let p = |field: &str| format!("plots[{index}].{field}");
    let invalid =
        |field: &str, message: &str| WidgetError::Config(format!("{}: {message}", p(field)));
    let number = |value, field: &str| require_f32(Some(value), &p(field));
    let positive = |value, field: &str| require_positive_f32(Some(value), &p(field));
    let percent = |value, field: &str| Ok(require_percentage(Some(value), &p(field))? / 100.0);
    let ranged = |value, field: &str, min: f32, max: f32| {
        let value = number(value, field)?;
        if !(min..=max).contains(&value) {
            return Err(invalid(field, &format!("must be between {min} and {max}")));
        }
        Ok(value)
    };
    if chart.value != "rolling_chart" {
        return Err(invalid("value", "must be rolling_chart"));
    }
    let limits = chart_limits();
    if chart.series.is_empty() || chart.series.len() > limits.max_series {
        return Err(invalid(
            "series",
            &format!("must list 1 to {} metrics", limits.max_series),
        ));
    }
    let mut series: Vec<ValidatedChartSeries> = Vec::with_capacity(chart.series.len());
    for (row, item) in chart.series.iter().enumerate() {
        let field = |name: &str| format!("series[{row}].{name}");
        if !is_plottable_metric(item.metric) {
            return Err(invalid(&field("metric"), "metric cannot be plotted"));
        }
        if series.iter().any(|plotted| plotted.metric == item.metric) {
            return Err(invalid(&field("metric"), "metric is already plotted"));
        }
        if item.label.as_deref().is_some_and(|label| label.trim().is_empty()) {
            return Err(invalid(&field("label"), "must name the metric"));
        }
        series.push(ValidatedChartSeries {
            metric: item.metric,
            label: item.label.clone(),
            color: require_hex_color(Some(&item.color), &p(&field("color")))?,
        });
    }
    let marker = ValidatedChartMarker {
        variant: MarkerVariant::parse(&chart.marker_variant, &p("marker_variant"))?,
        size: positive(chart.marker_size, "marker_size")?,
        variant_diameter: positive(chart.marker_variant_diameter, "marker_variant_diameter")?,
        opacity: percent(chart.marker_opacity, "marker_opacity")?,
    };
    if chart.legend_font.trim().is_empty() {
        return Err(invalid("legend_font", "must identify a font"));
    }
    let legend = ValidatedChartLegend {
        font: chart.legend_font.clone(),
        font_size: positive(chart.legend_font_size, "legend_font_size")?,
        color: require_hex_color(Some(&chart.legend_color), &p("legend_color"))?,
    };
    let area_opacity = percent(chart.area_opacity, "area_opacity")?;
    Ok(ValidatedRollingChart {
        x: number(chart.x, "x")?,
        y: number(chart.y, "y")?,
        width: positive(chart.width, "width")?,
        height: positive(chart.height, "height")?,
        opacity: ranged(chart.opacity, "opacity", 0.0, 1.0)?,
        window_seconds: f64::from(ranged(
            chart.window_seconds,
            "window_seconds",
            limits.min_window_seconds,
            limits.max_window_seconds,
        )?),
        line_width: ranged(chart.line_width, "line_width", 0.5, 50.0)?,
        line_opacity: percent(chart.line_opacity, "line_opacity")?,
        // The first half of the percentage cuts corners; the second also pulls samples inward.
        line_curve: percent(chart.line_curve, "line_curve")? * 2.0,
        area_opacity: chart.area_fill.then_some(area_opacity),
        area_gradient: chart.area_gradient,
        series,
        marker: chart.show_marker.then_some(marker),
        legend: chart.show_legend.then_some(legend),
    })
}

/// Legend row height, swatch length and gaps, relative to the legend font size.
pub const LEGEND_ROW_EM: f32 = 1.6;
const LEGEND_SWATCH_EM: f32 = 1.0;
const LEGEND_SWATCH_GAP_EM: f32 = 0.35;
const LEGEND_ENTRY_GAP_EM: f32 = 1.2;
/// Scene pixels between consecutive samples of a line.
pub const SAMPLE_SPACING: f32 = 2.0;

/// Seconds sampled across the window ending at `second`: both window edges, and the
/// time grid between them, so sample positions slide smoothly instead of shimmering.
pub fn sample_times(second: f64, window: f64, step: f64) -> Vec<f64> {
    let start = second - window;
    let first = (start / step).floor() as i64 + 1;
    let last = (second / step).ceil() as i64 - 1;
    let mut times = Vec::with_capacity((last - first + 3).max(2) as usize);
    times.push(start);
    times.extend((first..=last).map(|index| index as f64 * step));
    times.push(second);
    times
}

/// Height share of `value` in the metric's activity range; flat metrics sit in the middle.
pub fn normalized(value: f64, (min, max): (f64, f64)) -> f32 {
    if max - min <= f64::EPSILON {
        0.5
    } else {
        ((value - min) / (max - min)) as f32
    }
}

/// Left edge of each entry, in rows: an entry that would cross `width` starts a new row,
/// unless it is the first in its row. `widths` are the entries' extents without trailing gaps.
pub fn wrap_entries(widths: &[f32], gap: f32, width: f32) -> Vec<(usize, f32)> {
    let mut row = 0;
    let mut x = 0.0;
    widths
        .iter()
        .map(|entry| {
            if x > 0.0 && x + entry > width {
                row += 1;
                x = 0.0;
            }
            let placed = (row, x);
            x += entry + gap;
            placed
        })
        .collect()
}

struct LegendEntry {
    series: usize,
    swatch: (Point, Point),
    text_x: f32,
    baseline: f32,
    blob: TextBlob,
    ink: Rect,
}

/// Legend entries wrapped below the plot, in scene pixels.
pub struct LegendLayout {
    entries: Vec<LegendEntry>,
    /// Height of all rows.
    pub height: f32,
}

/// Lays out the legend rows starting at `top`, wrapping within the plot's `left..left + width`.
pub fn layout_legend(
    chart: &ValidatedRollingChart,
    legend: &ValidatedChartLegend,
    scale: f32,
    fonts: &dyn FontResolver,
    (left, top, width): (f32, f32, f32),
) -> WidgetResult<LegendLayout> {
    let font_size = legend.font_size * scale;
    let font = resolve_font(fonts, Some(&legend.font), font_size)?;
    let (_, metrics) = font.metrics();
    let stroke = chart.line_width * scale;
    let swatch = font_size * LEGEND_SWATCH_EM + stroke;
    let text_offset = swatch + font_size * LEGEND_SWATCH_GAP_EM;
    let measured = chart
        .series
        .iter()
        .map(|series| {
            let label = series.label.as_deref().ok_or_else(|| {
                WidgetError::Config(format!(
                    "rolling chart legend: {} needs a label",
                    series.metric.as_str()
                ))
            })?;
            let blob = TextBlob::from_str(label, &font)
                .ok_or_else(|| WidgetError::Render("unable to lay out chart legend".into()))?;
            let (advance, ink) = font.measure_str(label, None);
            Ok((blob, advance, ink))
        })
        .collect::<WidgetResult<Vec<_>>>()?;
    let widths: Vec<f32> = measured
        .iter()
        .map(|(_, advance, _)| text_offset + advance)
        .collect();
    let placed = wrap_entries(&widths, font_size * LEGEND_ENTRY_GAP_EM, width);
    let row_height = font_size * LEGEND_ROW_EM;
    let rows = placed.last().map_or(0, |(row, _)| row + 1);
    let entries = measured
        .into_iter()
        .zip(placed)
        .enumerate()
        .map(|(series, ((blob, _, ink), (row, x)))| {
            let center = top + (row as f32 + 0.5) * row_height;
            let start = left + x + stroke * 0.5;
            LegendEntry {
                series,
                swatch: (
                    Point::new(start, center),
                    Point::new(start + font_size * LEGEND_SWATCH_EM, center),
                ),
                text_x: left + x + text_offset,
                baseline: center - (metrics.ascent + metrics.descent) * 0.5,
                blob,
                ink,
            }
        })
        .collect();
    Ok(LegendLayout {
        entries,
        height: rows as f32 * row_height,
    })
}

#[derive(Clone, Debug)]
pub struct SeriesStyle {
    pub metric: MetricKind,
    pub line: Paint,
    pub area: Option<Paint>,
    pub marker: Option<PlotMarker>,
}

#[derive(Clone, Debug)]
pub struct RollingChartCache {
    /// Scene rectangle the lines span; values map inside it by half a stroke.
    pub(crate) plot: Rect,
    pub(crate) stroke: f32,
    pub(crate) window: f64,
    /// Seconds between grid samples.
    pub(crate) step: f64,
    /// 0..=2: corner cut share of each half segment, then pull toward neighbours beyond 1.
    pub(crate) curve: f32,
    pub(crate) series: Vec<SeriesStyle>,
    pub(crate) legend: Option<StaticRaster>,
    /// Stable raster bounds: lines, areas and markers never leave them.
    pub(crate) bounds: Rect,
    /// The editor frame: the plot area and the legend rows below it.
    pub(crate) content_bounds: Rect,
}

impl RollingChartCache {
    /// Scene y of a normalized value.
    pub fn y(&self, share: f32) -> f32 {
        let half = self.stroke * 0.5;
        self.plot.bottom - half - share * (self.plot.height() - self.stroke).max(0.0)
    }

    /// Scene x of a second inside the window ending at `second`.
    pub fn x(&self, time: f64, second: f64) -> f32 {
        self.plot.left + ((time - second + self.window) / self.window) as f32 * self.plot.width()
    }
}

fn line_paint(color: &str, width: f32, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(parse_color(color, opacity));
    paint
}

/// The optional gradient fades from full area opacity at the plot top to transparent at its bottom.
fn area_paint(color: &str, opacity: f32, gradient: Option<(f32, f32)>) -> WidgetResult<Paint> {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(color, opacity));
    if let Some((top, bottom)) = gradient {
        let solid = Color4f::from(parse_color(color, opacity));
        let colors = [solid, Color4f::new(solid.r, solid.g, solid.b, 0.0)];
        let gradient = gradient::Gradient::new(
            gradient::Colors::new(&colors, None, TileMode::Clamp, None),
            gradient::Interpolation::default(),
        );
        paint.set_shader(
            gradient::shaders::linear_gradient(((0.0, top), (0.0, bottom)), &gradient, None)
                .ok_or_else(|| {
                    WidgetError::Render("unable to prepare chart area gradient".into())
                })?,
        );
    }
    Ok(paint)
}

/// Lines, areas and markers carry no scene shadow; only legend text does.
pub fn prepare_rolling_chart_cache(
    chart: &ValidatedRollingChart,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<RollingChartCache> {
    let scale = scene.scale;
    let opacity = chart.opacity * scene.opacity;
    let plot = Rect::from_xywh(chart.x, chart.y, chart.width * scale, chart.height * scale);
    let stroke = chart.line_width * scale;
    let series = chart
        .series
        .iter()
        .map(|series| {
            Ok(SeriesStyle {
                metric: series.metric,
                line: line_paint(&series.color, stroke, chart.line_opacity * opacity),
                area: chart
                    .area_opacity
                    .map(|area| {
                        area_paint(
                            &series.color,
                            area * opacity,
                            chart.area_gradient.then_some((plot.top, plot.bottom)),
                        )
                    })
                    .transpose()?,
                marker: chart.marker.as_ref().map(|marker| {
                    PlotMarker::new(
                        marker.variant,
                        marker.size,
                        marker.variant_diameter,
                        &series.color,
                        marker.opacity * opacity,
                        scale,
                    )
                }),
            })
        })
        .collect::<WidgetResult<Vec<_>>>()?;
    let marker_extent = series
        .iter()
        .filter_map(|series| series.marker.as_ref().map(PlotMarker::extent))
        .fold(stroke * 0.5, f32::max);
    let mut bounds = plot.with_outset((marker_extent, marker_extent));
    let mut content_bounds = plot;
    let legend = chart
        .legend
        .as_ref()
        .map(|legend| {
            let layout = layout_legend(
                chart,
                legend,
                scale,
                fonts,
                (plot.left, plot.bottom, plot.width()),
            )?;
            content_bounds.bottom += layout.height;
            draw_legend(
                chart,
                legend,
                scene,
                &series,
                &layout,
                content_bounds,
                raster_scale,
            )
        })
        .transpose()?;
    if let Some(legend) = &legend {
        bounds.join(legend.bounds());
    }
    let window = chart.window_seconds;
    Ok(RollingChartCache {
        plot,
        stroke,
        window,
        step: window / f64::from((plot.width() / SAMPLE_SPACING).max(1.0)),
        curve: chart.line_curve,
        series,
        legend,
        bounds: Rect::new(
            bounds.left.floor() - 1.0,
            bounds.top.floor() - 1.0,
            bounds.right.ceil() + 1.0,
            bounds.bottom.ceil() + 1.0,
        ),
        content_bounds,
    })
}

/// Swatch and label per series in series order, wrapped into rows; drawn once.
fn draw_legend(
    chart: &ValidatedRollingChart,
    legend: &ValidatedChartLegend,
    scene: &ValidatedScenePresentation,
    series: &[SeriesStyle],
    layout: &LegendLayout,
    rows: Rect,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<StaticRaster> {
    let scale = scene.scale;
    let opacity = chart.opacity * scene.opacity;
    let font_size = legend.font_size * scale;
    let style = ResolvedTextStyle {
        line_height: font_size,
        ..ResolvedTextStyle::widget(
            scene,
            scale,
            Some(legend.font.clone()),
            legend.font_size,
            parse_color(&legend.color, opacity),
            opacity,
        )
    };
    let paints = TextPaints::new(&style);
    let stroke = chart.line_width * scale;
    let mut extent = Rect::new(
        rows.left,
        rows.bottom - layout.height,
        rows.right,
        rows.bottom,
    );
    let mut text = Rect::new_empty();
    for entry in &layout.entries {
        let (start, end) = entry.swatch;
        extent.join(Rect::new(start.x, start.y, end.x, end.y).with_outset((stroke, stroke)));
        text.join(entry.ink.with_offset((entry.text_x, entry.baseline)));
    }
    // Text alone carries the scene shadow and border.
    let reach = style.border_thickness * 0.5 + style.shadow_strength * 3.0;
    let shadow = style.shadow_distance;
    let text = text.with_outset((reach, reach));
    extent.join(text);
    extent.join(text.with_offset((shadow, shadow)));
    StaticRaster::new(
        raster_scale,
        Point::default(),
        extent.with_outset((1.0, 1.0)),
        SHAPE_COLOR_TYPE,
        |canvas| {
            for entry in &layout.entries {
                canvas.draw_line(entry.swatch.0, entry.swatch.1, &series[entry.series].line);
                paints.draw(canvas, &entry.blob, (entry.text_x, entry.baseline));
            }
            Ok(())
        },
    )
}

/// Continues a path standing at `vertices[0]` through the rest. Each interior corner is cut
/// by a quadratic curve over `curve` of the half segments beside it, with the vertex as its
/// control point: the tip moves inward and the curve stays inside the corner. Beyond 1,
/// interior vertices first pull toward their neighbours' midpoint, up to halfway at 2.
pub fn cut_corners(path: &mut PathBuilder, vertices: &[Point], curve: f32) {
    let last = vertices.len() - 1;
    let pulled: Vec<Point>;
    let vertices = if curve > 1.0 {
        let pull = (curve - 1.0) / 2.0;
        pulled = (0..=last)
            .map(|i| match i {
                0 => vertices[0],
                i if i == last => vertices[last],
                i => {
                    let at = vertices[i];
                    at + ((vertices[i - 1] + vertices[i + 1]) * 0.5 - at) * pull
                }
            })
            .collect();
        &pulled
    } else {
        vertices
    };
    let curve = curve.min(1.0);
    if curve > 0.0 {
        for window in vertices.windows(3) {
            let (before, at, after) = (window[0], window[1], window[2]);
            path.line_to(at + (before - at) * (curve / 2.0));
            path.quad_to(at, at + (after - at) * (curve / 2.0));
        }
    } else {
        for point in &vertices[1..last] {
            path.line_to(*point);
        }
    }
    path.line_to(vertices[last]);
}

pub struct SeriesFrame {
    /// Contiguous observed stretches; gaps in the telemetry break the line. Curved charts
    /// join the recorded samples, others the time grid.
    runs: Vec<Vec<Point>>,
    /// The playhead's value at the right edge, when observed.
    marker: Option<Point>,
}

pub struct RollingChartFrame {
    series: Vec<SeriesFrame>,
}

impl RollingChartCache {
    /// Without an activity, or outside a metric's coverage, the plot stays empty.
    pub fn prepare_frame(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> RollingChartFrame {
        let times = sample_times(second, self.window, self.step);
        let series = self
            .series
            .iter()
            .map(|style| {
                let Some((activity, range)) = activity.and_then(|activity| {
                    activity
                        .plot_range(style.metric)
                        .map(|range| (activity, range))
                }) else {
                    return SeriesFrame {
                        runs: Vec::new(),
                        marker: None,
                    };
                };
                let point = |(time, value): (f64, f64)| {
                    Point::new(self.x(time, second), self.y(normalized(value, range)))
                };
                let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
                let mut open = false;
                let mut last = None;
                for &time in &times {
                    let sample = activity
                        .plot_value(style.metric, time)
                        .map(|value| (time, value));
                    match sample {
                        Some(sample) if open => runs.last_mut().expect("open run").push(sample),
                        Some(sample) => {
                            runs.push(vec![sample]);
                            open = true;
                        }
                        None => open = false,
                    }
                    last = sample;
                }
                let runs = runs
                    .into_iter()
                    .map(|run| {
                        let (first, end) = (run[0], run[run.len() - 1]);
                        // Corners exist only at recorded samples; denser data than the grid draws the grid.
                        let knots = (self.curve > 0.0 && run.len() > 1)
                            .then(|| activity.plot_knots(style.metric, first.0, end.0, run.len()))
                            .flatten();
                        match knots {
                            Some(knots) => [first]
                                .into_iter()
                                .chain(knots)
                                .chain([end])
                                .map(point)
                                .collect(),
                            None => run.into_iter().map(point).collect(),
                        }
                    })
                    .collect();
                SeriesFrame {
                    runs,
                    marker: last.map(point),
                }
            })
            .collect();
        RollingChartFrame { series }
    }
}

impl RollingChartFrame {
    /// Series paint back to front, each with its area, line and marker.
    pub fn draw(&self, canvas: &Canvas, cache: &RollingChartCache) {
        for (style, frame) in cache.series.iter().zip(&self.series) {
            for run in &frame.runs {
                let mut line = PathBuilder::new();
                line.move_to(run[0]);
                cut_corners(&mut line, run, cache.curve);
                if run.len() == 1 {
                    // A lone sample still paints a round dot.
                    line.line_to(run[0]);
                }
                let line = line.detach();
                if let Some(area) = &style.area {
                    let (first, last) = (run[0], run[run.len() - 1]);
                    let mut fill = PathBuilder::new();
                    fill.move_to((first.x, cache.plot.bottom));
                    fill.line_to(first);
                    cut_corners(&mut fill, run, cache.curve);
                    fill.line_to((last.x, cache.plot.bottom));
                    fill.close();
                    canvas.draw_path(&fill.detach(), area);
                }
                canvas.draw_path(&line, &style.line);
            }
            if let (Some(marker), Some(point)) = (&style.marker, frame.marker) {
                marker.draw(canvas, point);
            }
        }
        if let Some(legend) = &cache.legend {
            legend.draw(canvas, None);
        }
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel content size: the plot area and the legend rows below it.
/// The legend rewraps at the new width, so the plot takes the height the legend leaves.
pub fn resize_rolling_chart(
    widget: RollingChartConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid rolling chart resize size".into(),
        ));
    }
    let chart = validate_rolling_chart(&widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    let width = (size[0] / scale).clamp(8.0, 10_000.0);
    let legend = match &chart.legend {
        Some(legend) => {
            layout_legend(&chart, legend, scale, fonts, (0.0, 0.0, width * scale))?.height / scale
        }
        None => 0.0,
    };
    let height = (size[1] / scale - legend).clamp(8.0, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - (height + legend) * scale * fixed[1],
        width,
        height,
    })
}

pub struct PreviewRollingChart {
    cache: RollingChartCache,
    preview_raster_scale: PreviewRasterScale,
    last_second: Option<f64>,
    pub tile: Tile,
}

impl PreviewRollingChart {
    pub fn new(
        widget: RollingChartConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_rolling_chart(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedRollingChart,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_rolling_chart_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            last_second: None,
            tile: Tile::default(),
        })
    }

    /// The window slides with the playhead, so the frame depends on the second itself.
    pub fn frame_key(&self, second: f64) -> f64 {
        second
    }

    /// Scene rectangle a render draws into; lines and markers stay inside fixed bounds.
    pub fn layout_bounds(&self) -> TileBounds {
        self.cache.bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> TileBounds {
        self.cache
            .prepare_frame(activity, second)
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        if self.last_second == Some(second) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(activity, second);
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            self.cache.bounds,
            Presented::content(self.cache.content_bounds),
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        self.last_second = Some(second);
        Ok(())
    }

    /// A replaced activity redraws the current second.
    pub fn invalidate_activity(&mut self) {
        self.last_second = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{cut_corners, normalized, sample_times, wrap_entries};
    use skia_safe::{PathBuilder, Point};

    #[test]
    fn samples_span_both_window_edges_on_a_fixed_time_grid() {
        let times = sample_times(12.25, 10.0, 0.5);
        assert_eq!(times.first(), Some(&2.25));
        assert_eq!(times.last(), Some(&12.25));
        assert_eq!(
            times[1..times.len() - 1],
            (5..=24).map(|k| k as f64 * 0.5).collect::<Vec<_>>()
        );
        // Grid points never repeat an edge that falls on the grid.
        let aligned = sample_times(12.0, 10.0, 0.5);
        assert_eq!(aligned.len(), 21);
        assert!(aligned.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn grid_samples_keep_their_time_as_the_window_slides() {
        let earlier = sample_times(20.0, 5.0, 0.25);
        let later = sample_times(20.1, 5.0, 0.25);
        for time in [15.25, 17.5, 19.75] {
            assert!(earlier.contains(&time) && later.contains(&time));
        }
    }

    #[test]
    fn values_scale_to_their_activity_range_and_flat_metrics_sit_mid_height() {
        assert_eq!(normalized(100.0, (100.0, 200.0)), 0.0);
        assert_eq!(normalized(200.0, (100.0, 200.0)), 1.0);
        assert_eq!(normalized(125.0, (100.0, 200.0)), 0.25);
        assert_eq!(normalized(42.0, (42.0, 42.0)), 0.5);
    }

    #[test]
    fn legend_entries_wrap_onto_new_rows_at_the_plot_width() {
        // Gap 10, width 100: 40 + 10 + 40 fits; the third entry starts a new row.
        assert_eq!(
            wrap_entries(&[40.0, 40.0, 30.0], 10.0, 100.0),
            vec![(0, 0.0), (0, 50.0), (1, 0.0)]
        );
        // An entry wider than the plot still occupies its own row instead of an empty one.
        assert_eq!(
            wrap_entries(&[150.0, 20.0], 10.0, 100.0),
            vec![(0, 0.0), (1, 0.0)]
        );
        assert_eq!(
            wrap_entries(&[20.0, 150.0], 10.0, 100.0),
            vec![(0, 0.0), (1, 0.0)]
        );
    }

    #[test]
    fn cut_corners_stay_inside_the_polyline_and_vanish_at_zero() {
        // Screen y grows downward: the middle sample is a spike, then a ramp kink.
        let vertices = [
            Point::new(0.0, 100.0),
            Point::new(40.0, 0.0),
            Point::new(80.0, 100.0),
            Point::new(120.0, 60.0),
            Point::new(160.0, 50.0),
        ];
        let traced = |curve: f32| {
            let mut path = PathBuilder::new();
            path.move_to(vertices[0]);
            cut_corners(&mut path, &vertices, curve);
            path.detach().compute_tight_bounds()
        };
        assert_eq!(traced(0.0).top, 0.0);
        // Cutting more of each segment lowers the spike's tip further, never past its neighbours.
        let (half, full) = (traced(0.5), traced(1.0));
        assert!(0.0 < half.top && half.top < full.top && full.top < 100.0);
        assert_eq!(full.bottom, 100.0);
    }
}
