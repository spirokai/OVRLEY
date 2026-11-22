//! Static chart style: per-series paints and markers, the cached legend, and stable bounds.
use super::validation::{ValidatedChartLegend, ValidatedRollingChart};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::ValidatedScenePresentation, standard_widgets::widget_font_weight, types::MetricKind,
    },
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle, TextPaints},
    },
    widgets::{plot_marker::PlotMarker, value::preview::SHAPE_COLOR_TYPE},
};
use skia_safe::{
    gradient, paint::Style, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, Rect, TextBlob, TileMode,
};

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
            let blob = TextBlob::from_str(&series.label, &font)
                .ok_or_else(|| WidgetError::Render("unable to lay out chart legend".into()))?;
            let (advance, ink) = font.measure_str(&series.label, None);
            Ok((blob, advance, ink))
        })
        .collect::<WidgetResult<Vec<_>>>()?;
    let widths: Vec<f32> = measured.iter().map(|(_, advance, _)| text_offset + advance).collect();
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
            draw_legend(chart, legend, scene, &series, &layout, content_bounds, raster_scale)
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
        x: 0.0,
        y: 0.0,
        font_name: Some(legend.font.clone()),
        font_size,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: font_size,
        color: parse_color(&legend.color, opacity),
        opacity,
        shadow_color: Some(parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: Some(parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scale,
        numerals: scene.numerals,
    };
    let paints = TextPaints::new(&style);
    let stroke = chart.line_width * scale;
    let mut extent = Rect::new(rows.left, rows.bottom - layout.height, rows.right, rows.bottom);
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
