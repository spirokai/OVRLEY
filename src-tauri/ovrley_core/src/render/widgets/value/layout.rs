/// Metric widget layout: text positioning, icon placement, unit sizing, and
/// vertical-metrics helpers.
///
/// The metric row is manually laid out so icon, value, and units can each use
/// their own size while sharing one top-left anchor.
use crate::config::ValueConfig;
use crate::render::text::{draw_text, measure_text, parse_color, ResolvedTextStyle};
use skia_safe::Canvas;
use std::path::PathBuf;

const METRIC_WIDGET_LINE_HEIGHT: f32 = 0.92;
const METRIC_WIDGET_OUTER_GAP_PX: f32 = 8.0;
const METRIC_WIDGET_UNITS_GAP_PX: f32 = 8.0;
const MIN_UNITS_FONT_SIZE: f32 = 12.0;

pub const NUMERIC_VERTICAL_METRICS_TEXT: &str = "0123456789-:.%";

/// Draws the icon, value text, and optional unit text for a metric widget.
pub(crate) fn draw_metric_parts(
    canvas: &Canvas,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    parts: &crate::render::format::MetricDisplayParts,
    scale: f32,
    font_dirs: &[PathBuf],
    static_icon_rendered: bool,
) {
    let value_measure = measure_text(&parts.value_text, base_style, font_dirs);
    let value_vertical_measure = measure_text(
        super::metric_vertical_metrics_text(&parts.value_text),
        base_style,
        font_dirs,
    );
    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;

    let mut units_style = base_style.clone();
    units_style.font_size = (base_style.font_size * 0.28).max(MIN_UNITS_FONT_SIZE * scale);
    units_style.line_height = units_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let units_measure = parts
        .unit_text
        .as_deref()
        .map(|unit_text| measure_text(unit_text, &units_style, font_dirs));
    let units_line_height = units_style.font_size * METRIC_WIDGET_LINE_HEIGHT;

    let icon_size = value.icon_size.unwrap_or(28.0) * scale;
    let show_units = parts.unit_text.is_some();
    let show_icon = parts.show_icon && parts.icon_kind.is_some();
    let icon_margin_right = (base_style.font_size * 0.08).max(METRIC_WIDGET_OUTER_GAP_PX * scale);
    let text_group_height = if show_units {
        value_line_height.max(units_line_height)
    } else {
        value_line_height
    };
    let row_height = if show_icon {
        icon_size.max(text_group_height)
    } else {
        text_group_height
    };
    let text_group_left = if show_icon {
        icon_size + (METRIC_WIDGET_OUTER_GAP_PX * scale) + icon_margin_right
    } else {
        0.0
    };
    let text_group_top = base_style.y + ((row_height - text_group_height) * 0.5);
    let text_group_bottom = text_group_top + text_group_height;
    let value_glyph_height =
        (value_vertical_measure.bounds_bottom - value_vertical_measure.bounds_top).abs();
    let value_top = text_group_bottom - (value_line_height + value_glyph_height) * 0.5;

    let mut value_style = base_style.clone();
    value_style.x = base_style.x + text_group_left;
    value_style.y = value_top;
    value_style.line_height = value_line_height;

    if show_icon && !static_icon_rendered {
        super::icons::draw_metric_icon(
            canvas,
            parts.icon_kind,
            value.icon_color.as_deref().unwrap_or("#40e0d0"),
            base_style.opacity,
            base_style.shadow_color,
            base_style.shadow_strength,
            base_style.shadow_distance,
            base_style.x + value.icon_offset_x.unwrap_or(0.0) * scale,
            metric_icon_top_from_value_layout(
                text_group_bottom,
                value_line_height,
                &value_vertical_measure,
                icon_size,
            ) + value.icon_offset_y.unwrap_or(0.0) * scale,
            icon_size,
        );
    }

    draw_text(canvas, &parts.value_text, &value_style, font_dirs);

    if let (Some(unit_text), Some(unit_measure)) = (parts.unit_text.as_deref(), units_measure) {
        let mut units_style = units_style;
        units_style.color = parse_color(
            value.unit_color.as_deref().unwrap_or("#ffffff"),
            base_style.opacity,
        );
        units_style.x = value_style.x + value_measure.width + (METRIC_WIDGET_UNITS_GAP_PX * scale);
        let units_glyph_height = (unit_measure.bounds_bottom - unit_measure.bounds_top).abs();
        units_style.y = text_group_bottom - (units_line_height + units_glyph_height) * 0.5;
        draw_text(canvas, unit_text, &units_style, font_dirs);
    }
}

/// Returns whether a value contributes an icon that can be cached statically.
pub(crate) fn has_static_metric_icon(value: &ValueConfig) -> bool {
    value
        .show_icon
        .unwrap_or(value.value != crate::MetricKind::Gradient)
        && super::icons::metric_icon_kind_for_value(value.value).is_some()
        && metric_icon_has_stable_static_vertical_metrics(value)
}

/// Draws one static metric icon for caching in the base layer.
///
/// Static icon drawing is used for the cached base layer. Dynamic text is
/// drawn later, but the icon can be reused every frame.
pub(crate) fn draw_static_metric_icon_for_value(
    canvas: &Canvas,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    scale: f32,
    font_dirs: &[PathBuf],
) -> bool {
    let Some(icon_kind) = super::icons::metric_icon_kind_for_value(value.value) else {
        return false;
    };
    if !value
        .show_icon
        .unwrap_or(value.value != crate::MetricKind::Gradient)
    {
        return false;
    }

    let icon_size = value.icon_size.unwrap_or(28.0) * scale;
    if icon_size <= 0.0 {
        return false;
    }

    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let row_height = icon_size.max(value_line_height);
    let text_group_top = base_style.y + ((row_height - value_line_height) * 0.5);
    let text_group_bottom = text_group_top + value_line_height;
    let value_vertical_measure = measure_text(NUMERIC_VERTICAL_METRICS_TEXT, base_style, font_dirs);
    super::icons::draw_metric_icon(
        canvas,
        Some(icon_kind),
        value.icon_color.as_deref().unwrap_or("#40e0d0"),
        base_style.opacity,
        base_style.shadow_color,
        base_style.shadow_strength,
        base_style.shadow_distance,
        base_style.x + value.icon_offset_x.unwrap_or(0.0) * scale,
        metric_icon_top_from_value_layout(
            text_group_bottom,
            value_line_height,
            &value_vertical_measure,
            icon_size,
        ) + value.icon_offset_y.unwrap_or(0.0) * scale,
        icon_size,
    );
    true
}

/// Returns the text used for vertical alignment measurements.
///
/// Numeric metrics (digits, `:`, `.`, `%`, `+`, `-`) use a stable reference
/// string (`"888:88"`) so vertical layout does not jump when the displayed
/// value changes. Non-numeric text passes through unchanged.
pub fn metric_vertical_metrics_text(text: &str) -> &str {
    if !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, ':' | '.' | '%' | '+' | '-'))
    {
        NUMERIC_VERTICAL_METRICS_TEXT
    } else {
        text
    }
}

/// Static icons are cached without the current frame value. Keep that fast
/// path only when the preview also uses stable numeric metrics for vertical
/// layout.
fn metric_icon_has_stable_static_vertical_metrics(value: &ValueConfig) -> bool {
    value.value != crate::MetricKind::Time
        || value.format.as_deref().unwrap_or("time-24") == "time-24"
}

/// Computes the icon top position so the icon is visually centered on the
/// value glyphs rather than on the row line box. This matches the frontend
/// preview layout so the Rust renderer produces identical icon placement.
pub fn metric_icon_top_from_value_layout(
    text_group_bottom: f32,
    value_line_height: f32,
    value_measure: &crate::render::text::MeasuredText,
    icon_size: f32,
) -> f32 {
    let value_glyph_height = (value_measure.bounds_bottom - value_measure.bounds_top).abs();
    let value_top = text_group_bottom - (value_line_height + value_glyph_height) * 0.5;
    value_top + (value_line_height * 0.5) - (icon_size * 0.5)
}
