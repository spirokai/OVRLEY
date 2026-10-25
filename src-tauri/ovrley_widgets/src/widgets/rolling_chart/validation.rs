use super::config::RollingChartConfig;
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        standard_widgets::chart_limits,
        telemetry::is_plottable_metric,
        types::MetricKind,
        validation::{require_f32, require_hex_color, require_percentage, require_positive_f32},
    },
    widgets::plot_marker::MarkerVariant,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedChartSeries {
    pub metric: MetricKind,
    pub label: String,
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
    let invalid = |field: &str, message: &str| WidgetError::Config(format!("{}: {message}", p(field)));
    let number = |value, field: &str| require_f32(Some(value), &p(field));
    let positive = |value, field: &str| require_positive_f32(Some(value), &p(field));
    let percent = |value, field: &str| Ok(require_percentage(value, &p(field))? / 100.0);
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
        if item.label.trim().is_empty() {
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
        area_opacity: chart.area_fill.then_some(area_opacity),
        area_gradient: chart.area_gradient,
        series,
        marker: chart.show_marker.then_some(marker),
        legend: chart.show_legend.then_some(legend),
    })
}
