//! Metric text layout: icon, value and unit placement, static parts and drawing.
use crate::model::catalog::DisplayType;
use crate::render::icons::metric_icon_bounds;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    draw_text_with_vertical_metrics_text, measure_text, resolve_styled_font, rgba_color,
    text_selection_bounds, FontResolver, ResolvedTextStyle,
};
use crate::widgets::value::format::MetricDisplayParts;
use crate::widgets::value::{ContentAlignment, ValidatedValueWidget};
use crate::WidgetResult;
use skia_safe::{Canvas, Font, Point, Rect};

// Metric widget layout: text positioning, icon placement, unit sizing, and
// vertical-metrics helpers.
//
// The metric row is manually laid out so icon, value, and units can each use
// their own size while sharing one configurable point anchor.

pub const METRIC_WIDGET_UNITS_GAP_PX: f32 = 8.0;
pub const MIN_UNITS_FONT_SIZE: f32 = 12.0;
pub const METRIC_WIDGET_UNIT_RATIO: f32 = 0.28;

pub const NUMERIC_VERTICAL_METRICS_TEXT: &str = "0123456789-:.%";
const COORDINATE_VERTICAL_METRICS_TEXT: &str = "NSEW88\u{00B0}88.888\u{2032}88\u{2033}";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StaticMetricParts {
    pub icon: bool,
    pub unit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MetricHorizontalLayout {
    pub(crate) content_width: f32,
    pub(crate) row_origin_x: f32,
    pub(crate) icon_x: Option<f32>,
    pub(crate) value_x: f32,
    pub(crate) unit_x: Option<f32>,
}

fn metric_row_origin_x(alignment: ContentAlignment, anchor_x: f32, content_width: f32) -> f32 {
    match alignment {
        ContentAlignment::Left => anchor_x,
        ContentAlignment::Center => anchor_x - content_width * 0.5,
        ContentAlignment::Right => anchor_x - content_width,
    }
}

pub(crate) fn metric_horizontal_layout(
    alignment: ContentAlignment,
    anchor_x: f32,
    icon_slot_width: Option<f32>,
    value_width: f32,
    unit_width: Option<f32>,
    unit_gap: f32,
) -> MetricHorizontalLayout {
    let icon_width = icon_slot_width.unwrap_or(0.0);
    let trailing_width = unit_width.map_or(0.0, |width| unit_gap + width);
    let content_width = icon_width + value_width + trailing_width;
    let row_origin_x = metric_row_origin_x(alignment, anchor_x, content_width);
    MetricHorizontalLayout {
        content_width,
        row_origin_x,
        icon_x: icon_slot_width.map(|_| row_origin_x),
        value_x: row_origin_x + icon_width,
        unit_x: unit_width.map(|_| row_origin_x + icon_width + value_width + unit_gap),
    }
}

/// Draws the icon, value text, and optional unit text for a metric widget.
pub fn draw_metric_parts(
    canvas: &Canvas,
    base_style: &ResolvedTextStyle,
    parts: &crate::widgets::value::format::MetricDisplayParts,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
    static_parts: StaticMetricParts,
    validated: &ValidatedValueWidget,
) -> WidgetResult<()> {
    let frame = prepare_metric_frame(base_style, parts, scale, fonts, validated)?;
    draw_prepared_metric_frame(
        canvas,
        base_style,
        parts,
        fonts,
        static_parts,
        validated,
        &frame,
    )
}

pub fn draw_prepared_metric_frame(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    parts: &crate::widgets::value::format::MetricDisplayParts,
    fonts: &dyn crate::render::text::FontResolver,
    static_parts: StaticMetricParts,
    validated: &ValidatedValueWidget,
    frame: &MetricFrame,
) -> WidgetResult<()> {
    if let Some(icon) = frame.icon.filter(|_| !static_parts.icon) {
        crate::render::icons::draw_metric_icon(
            canvas,
            parts.icon_asset_file,
            rgba_color(validated.icon_color, scene_opacity(style, validated)),
            style,
            Point::new(icon.left, icon.top),
            icon.width(),
        );
    }
    if matches!(
        parts.content,
        crate::widgets::value::format::MetricDisplayContent::Coordinates(_)
    ) {
        for text in &frame.coordinate_text {
            draw_text_with_vertical_metrics_text(
                canvas,
                &text.text,
                &text.reference,
                &text.style,
                fonts,
            )?;
        }
        return Ok(());
    }
    let (value_text, unit_text) = parts.standard_text();
    draw_text_with_vertical_metrics_text(
        canvas,
        value_text,
        metric_vertical_metrics_text(value_text),
        &frame.value_style,
        fonts,
    )?;
    if let (Some(text), Some((unit_style, reference))) = (unit_text, &frame.unit) {
        if !static_parts.unit {
            draw_text_with_vertical_metrics_text(canvas, text, reference, unit_style, fonts)?;
        }
    }
    Ok(())
}

/// The scene opacity a resolved style applies on top of the widget opacity. Validated colours
/// already carry the widget opacity, so the unit and icon take only this factor, as the value's
/// colour does (`validated_value_style`). A transparent widget's colours are transparent already.
pub(crate) fn scene_opacity(style: &ResolvedTextStyle, validated: &ValidatedValueWidget) -> f32 {
    if validated.opacity == 0.0 {
        0.0
    } else {
        style.opacity / validated.opacity
    }
}

/// Returns the independently cacheable parts contributed by a validated value.
pub fn static_metric_parts_for_value(validated: &ValidatedValueWidget) -> StaticMetricParts {
    if validated.display_type != DisplayType::Text {
        return StaticMetricParts::default();
    }

    let icon = validated.content_alignment == ContentAlignment::Left
        && validated.show_icon
        && validated.icon_size > 0.0
        && crate::render::icons::metric_icon_asset_file(validated.metric).is_some();
    let unit = validated.content_alignment == ContentAlignment::Right
        && validated.metric != crate::model::catalog::MetricKind::GpsCoordinates
        && validated.show_units
        && !crate::model::catalog::catalog()
            .unit_label(validated.metric, Some(&validated.display_unit))
            .is_empty();

    StaticMetricParts { icon, unit }
}

/// Draws a metric widget's static parts: its icon and unit.
pub fn draw_static_metric_parts_for_value(
    canvas: &Canvas,
    validated: &ValidatedValueWidget,
    base_style: &ResolvedTextStyle,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
) -> WidgetResult<StaticMetricParts> {
    let static_parts = static_metric_parts_for_value(validated);
    if !static_parts.icon {
        if static_parts.unit {
            draw_static_metric_unit(canvas, validated, base_style, scale, fonts)?;
        }
        return Ok(static_parts);
    }
    let Some(icon_asset_file) = crate::render::icons::metric_icon_asset_file(validated.metric)
    else {
        return Ok(static_parts);
    };
    if !validated.show_icon {
        return Ok(static_parts);
    }

    let icon_size = validated.icon_size * scale;
    if icon_size <= 0.0 {
        return Ok(static_parts);
    }

    let icon_top = if validated.metric == crate::model::catalog::MetricKind::GpsCoordinates {
        let is_stacked = validated.display_unit == "both";
        let coordinate_font_size = if is_stacked {
            base_style.font_size * 0.4
        } else {
            base_style.font_size
        };
        let coordinate_line_height = coordinate_font_size * LINE_HEIGHT_RATIO;
        let coordinate_line_gap = if is_stacked {
            coordinate_font_size * 0.08
        } else {
            0.0
        };
        let line_count = if is_stacked { 2.0 } else { 1.0 };
        let text_height =
            coordinate_line_height * line_count + coordinate_line_gap * (line_count - 1.0);
        let row_height = icon_size.max(text_height);
        base_style.y + (row_height - icon_size) * 0.5
    } else {
        let value_line_height = base_style.font_size * LINE_HEIGHT_RATIO;
        let units_font_size =
            (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        let text_group_height = if validated.show_units {
            value_line_height.max(units_font_size * LINE_HEIGHT_RATIO)
        } else {
            value_line_height
        };
        let row_height = icon_size.max(text_group_height);
        let text_group_top = base_style.y + ((row_height - text_group_height) * 0.5);
        let text_group_bottom = text_group_top + text_group_height;
        let value_vertical_measure =
            measure_text(NUMERIC_VERTICAL_METRICS_TEXT, base_style, fonts)?;
        metric_icon_top_from_value_layout(
            text_group_bottom,
            value_line_height,
            &value_vertical_measure,
            icon_size,
        )
    };
    crate::render::icons::draw_metric_icon(
        canvas,
        Some(icon_asset_file),
        rgba_color(validated.icon_color, scene_opacity(base_style, validated)),
        base_style,
        Point::new(
            metric_row_origin_x(validated.content_alignment, base_style.x, 0.0)
                + validated.icon_offset_x * scale,
            icon_top + validated.icon_offset_y * scale,
        ),
        icon_size,
    );
    Ok(static_parts)
}

fn draw_static_metric_unit(
    canvas: &Canvas,
    validated: &ValidatedValueWidget,
    base_style: &ResolvedTextStyle,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
) -> WidgetResult<()> {
    let unit_text = crate::model::catalog::catalog()
        .unit_label(validated.metric, Some(&validated.display_unit));
    let mut unit_style = base_style.clone();
    unit_style.font_size =
        (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
    unit_style.line_height = unit_style.font_size * LINE_HEIGHT_RATIO;
    unit_style.color = rgba_color(validated.unit_color, scene_opacity(base_style, validated));
    let unit_measure = measure_text(unit_text, &unit_style, fonts)?;
    let horizontal_layout = metric_horizontal_layout(
        validated.content_alignment,
        base_style.x,
        None,
        0.0,
        Some(unit_measure.width),
        0.0,
    );
    unit_style.x = horizontal_layout
        .unit_x
        .expect("static unit must have a horizontal position");

    let value_line_height = base_style.font_size * LINE_HEIGHT_RATIO;
    let text_group_height = value_line_height.max(unit_style.line_height);
    let show_icon = validated.show_icon
        && validated.icon_size > 0.0
        && crate::render::icons::metric_icon_asset_file(validated.metric).is_some();
    let row_height = if show_icon {
        (validated.icon_size * scale).max(text_group_height)
    } else {
        text_group_height
    };
    let text_group_bottom =
        base_style.y + (row_height - text_group_height) * 0.5 + text_group_height;
    let unit_vertical_metrics_text = if unit_text == "\u{00B0}" {
        "\u{00B0}C"
    } else {
        unit_text
    };
    let vertical_measure = measure_text(unit_vertical_metrics_text, &unit_style, fonts)?;
    let glyph_height = (vertical_measure.bounds_bottom - vertical_measure.bounds_top).abs();
    unit_style.y = text_group_bottom - (unit_style.line_height + glyph_height) * 0.5;
    draw_text_with_vertical_metrics_text(
        canvas,
        unit_text,
        unit_vertical_metrics_text,
        &unit_style,
        fonts,
    )
}

/// Returns the text used for vertical alignment measurements.
///
/// Numeric metrics (digits, `:`, `.`, `%`, `+`, `-`) use a stable reference
/// string (`"888:88"`) so vertical layout does not jump when the displayed
/// value changes. Coordinate values use a stable reference containing the
/// direction letters and DMS/DDM symbols. Neutral gear uses the numeric
/// reference; other text passes through unchanged.
pub fn metric_vertical_metrics_text(text: &str) -> &str {
    if text.contains('\u{00B0}') && (text.contains('\u{2032}') || text.contains('\u{2033}')) {
        COORDINATE_VERTICAL_METRICS_TEXT
    } else if text == "N"
        || (!text.is_empty()
            && text
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, ':' | '.' | '%' | '+' | '-' | '/')))
    {
        NUMERIC_VERTICAL_METRICS_TEXT
    } else {
        text
    }
}

/// Computes the icon top position so the icon is visually centered on the
/// value glyphs rather than on the row line box.
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

pub struct PreparedText {
    pub text: String,
    pub reference: String,
    pub style: ResolvedTextStyle,
}

pub struct MetricFrame {
    pub coordinate_text: Vec<PreparedText>,
    pub value_style: ResolvedTextStyle,
    pub unit: Option<(ResolvedTextStyle, String)>,
    pub icon: Option<Rect>,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
}

/// The ink and selection bounds of `text`, its baseline placed by the `reference` text.
pub(crate) fn text_bounds(
    text: &str,
    reference: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<(Rect, Rect)> {
    let ink = measure_text(text, style, fonts)?;
    let vertical = measure_text(reference, style, fonts)?;
    let baseline = style.y
        + (style.line_height - (vertical.bounds_bottom - vertical.bounds_top)) * 0.5
        - vertical.bounds_top;
    Ok((
        Rect::new(
            style.x + ink.bounds_left,
            baseline + ink.bounds_top,
            style.x + ink.bounds_right,
            baseline + ink.bounds_bottom,
        ),
        text_selection_bounds(&ink, style.x, baseline),
    ))
}

pub fn prepare_metric_frame(
    style: &ResolvedTextStyle,
    parts: &MetricDisplayParts,
    scale: f32,
    fonts: &dyn FontResolver,
    widget: &ValidatedValueWidget,
) -> WidgetResult<MetricFrame> {
    if let crate::widgets::value::format::MetricDisplayContent::Coordinates(coordinate) =
        &parts.content
    {
        return prepare_coordinate_frame(style, parts, coordinate, scale, fonts, widget);
    }
    let (value, unit) = parts.standard_text();
    let value_measure = measure_text(value, style, fonts)?;
    let vertical = measure_text(metric_vertical_metrics_text(value), style, fonts)?;
    let value_height = style.font_size * LINE_HEIGHT_RATIO;
    let mut unit_style = style.clone();
    unit_style.font_size =
        (style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
    unit_style.line_height = unit_style.font_size * LINE_HEIGHT_RATIO;
    let unit_measure = unit
        .map(|s| measure_text(s, &unit_style, fonts))
        .transpose()?;
    let icon_size = widget.icon_size * scale;
    let show_icon = parts.show_icon && parts.icon_asset_file.is_some();
    let text_height = if unit.is_some() {
        value_height.max(unit_style.line_height)
    } else {
        value_height
    };
    let row_height = if show_icon {
        icon_size.max(text_height)
    } else {
        text_height
    };
    let icon_slot =
        show_icon.then_some(icon_size + 8.0 * scale + (style.font_size * 0.08).max(8.0 * scale));
    let horizontal = metric_horizontal_layout(
        widget.content_alignment,
        style.x,
        icon_slot,
        value_measure.width,
        unit_measure.as_ref().map(|m| m.width),
        8.0 * scale,
    );
    let bottom = style.y + (row_height - text_height) * 0.5 + text_height;
    let mut value_style = style.clone();
    value_style.x = horizontal.value_x;
    value_style.y =
        bottom - (value_height + (vertical.bounds_bottom - vertical.bounds_top).abs()) * 0.5;
    value_style.line_height = value_height;
    let (mut bounds, mut content_bounds) = text_bounds(
        value,
        metric_vertical_metrics_text(value),
        &value_style,
        fonts,
    )?;
    // Cached left-aligned icons use the stable numeric reference in native composition.
    let icon_vertical = if static_metric_parts_for_value(widget).icon {
        measure_text(NUMERIC_VERTICAL_METRICS_TEXT, style, fonts)?
    } else {
        vertical
    };
    let icon = horizontal.icon_x.map(|x| {
        Rect::from_xywh(
            x + widget.icon_offset_x * scale,
            metric_icon_top_from_value_layout(bottom, value_height, &icon_vertical, icon_size)
                + widget.icon_offset_y * scale,
            icon_size,
            icon_size,
        )
    });
    let unit = if let Some(text) = unit {
        let reference = if text == "\u{00b0}" {
            "\u{00b0}C"
        } else {
            text
        };
        let vertical = measure_text(reference, &unit_style, fonts)?;
        unit_style.x = horizontal.unit_x.expect("visible unit has a position");
        unit_style.y = bottom
            - (unit_style.line_height + (vertical.bounds_bottom - vertical.bounds_top).abs()) * 0.5;
        unit_style.color = rgba_color(widget.unit_color, scene_opacity(style, widget));
        let (unit_bounds, unit_content) = text_bounds(text, reference, &unit_style, fonts)?;
        bounds.join(unit_bounds);
        content_bounds.join(unit_content);
        Some((unit_style, reference.to_owned()))
    } else {
        None
    };
    Ok(finish_frame(
        MetricFrame {
            value_style,
            unit,
            icon,
            bounds,
            content_bounds,
            content_width: horizontal.content_width,
            coordinate_text: Vec::new(),
        },
        style,
        parts,
        Rect::from_xywh(
            horizontal.row_origin_x,
            style.y,
            horizontal.content_width,
            row_height,
        ),
    ))
}

/// One bounds policy for both standard rows and coordinate text runs.
fn finish_frame(
    mut frame: MetricFrame,
    style: &ResolvedTextStyle,
    parts: &MetricDisplayParts,
    line_box: Rect,
) -> MetricFrame {
    if style.border_color.is_some() {
        frame
            .content_bounds
            .outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
    }
    if let Some(rect) = frame.icon {
        frame.bounds.join(rect);
        frame.content_bounds.join(metric_icon_bounds(
            parts.icon_asset_file.expect("visible icon asset"),
            rect,
        ));
    }
    frame.bounds.join(line_box);
    let padding = 2.0
        + style.border_thickness * 0.5
        + if style.shadow_color.is_some() {
            style.shadow_strength * 3.0 + style.shadow_distance.abs()
        } else {
            0.0
        };
    frame.bounds.outset((padding, padding));
    frame.bounds = Rect::new(
        frame.bounds.left.floor(),
        frame.bounds.top.floor(),
        frame.bounds.right.ceil(),
        frame.bounds.bottom.ceil(),
    );
    frame
}

/// Many display fonts lack the prime marks (U+2032, U+2033), and Skia draws a missing glyph as a
/// box; such fonts get the ASCII apostrophe and quote instead. Both marks switch together so a
/// line never mixes styles.
fn coordinate_marks_for_font(text: &str, font: &Font) -> String {
    let has_primes = ['\u{2032}', '\u{2033}']
        .iter()
        .all(|mark| font.unichar_to_glyph(*mark as i32) != 0);
    if has_primes {
        text.to_owned()
    } else {
        text.replace('\u{2032}', "'").replace('\u{2033}', "\"")
    }
}

fn prepare_coordinate_frame(
    base_style: &ResolvedTextStyle,
    parts: &MetricDisplayParts,
    coordinate: &crate::widgets::value::format::MetricCoordinateDisplay,
    scale: f32,
    fonts: &dyn FontResolver,
    validated: &ValidatedValueWidget,
) -> WidgetResult<MetricFrame> {
    let is_stacked = coordinate.lines.len() == 2;
    let coordinate_font_size = if is_stacked {
        base_style.font_size * 0.4
    } else {
        base_style.font_size
    };
    let line_height = coordinate_font_size * LINE_HEIGHT_RATIO;
    let line_gap = if is_stacked {
        coordinate_font_size * 0.08
    } else {
        0.0
    };
    let total_height = (line_height * coordinate.lines.len() as f32)
        + (line_gap * coordinate.lines.len().saturating_sub(1) as f32);
    let direction_gap = (coordinate_font_size * 0.08).max(8.0 * scale);
    let mut line_measurements = Vec::with_capacity(coordinate.lines.len());
    let mut value_style = base_style.clone();
    value_style.font_size = coordinate_font_size;
    value_style.line_height = line_height;
    let font = resolve_styled_font(
        fonts,
        value_style.font_name.as_deref(),
        value_style.font_size,
        value_style.font_weight,
        value_style.italic,
    )?;
    let value_texts: Vec<String> = coordinate
        .lines
        .iter()
        .map(|line| coordinate_marks_for_font(&line.value_text, &font))
        .collect();
    for (line, value_text) in coordinate.lines.iter().zip(&value_texts) {
        let direction_measure = line
            .direction
            .as_deref()
            .map(|direction| measure_text(direction, &value_style, fonts))
            .transpose()?;
        let value_measure = measure_text(value_text, &value_style, fonts)?;
        let direction_width = direction_measure.map_or(0.0, |measure| measure.width);
        line_measurements.push((direction_width, value_measure.width));
    }
    let direction_column_width = line_measurements
        .iter()
        .map(|(direction_width, _)| *direction_width)
        .fold(0.0, f32::max);
    let value_column_width = line_measurements
        .iter()
        .map(|(_, value_width)| *value_width)
        .fold(0.0, f32::max);
    let icon_size = validated.icon_size * scale;
    let show_icon = parts.show_icon && parts.icon_asset_file.is_some();
    let text_group_left = if show_icon {
        icon_size + (8.0 * scale) + (base_style.font_size * 0.08).max(8.0 * scale)
    } else {
        0.0
    };
    let text_width = value_column_width
        + if direction_column_width > 0.0 {
            direction_column_width + direction_gap
        } else {
            0.0
        };
    let horizontal_layout = metric_horizontal_layout(
        validated.content_alignment,
        base_style.x,
        show_icon.then_some(text_group_left),
        text_width,
        None,
        0.0,
    );
    let row_height = total_height.max(if show_icon { icon_size } else { 0.0 });
    let text_top = base_style.y + (row_height - total_height) * 0.5;

    let icon = horizontal_layout.icon_x.map(|x| {
        Rect::from_xywh(
            x + validated.icon_offset_x * scale,
            base_style.y + (row_height - icon_size) * 0.5 + validated.icon_offset_y * scale,
            icon_size,
            icon_size,
        )
    });
    let mut texts = Vec::new();
    let mut bounds = Rect::new_empty();
    let mut content_bounds = Rect::new_empty();
    for (index, (line, value_text)) in coordinate.lines.iter().zip(&value_texts).enumerate() {
        let line_y = text_top + index as f32 * (line_height + line_gap);
        let (_, value_width) = line_measurements[index];
        let line_x = horizontal_layout.value_x;
        let line_vertical_metrics_text = metric_vertical_metrics_text(value_text);
        if let Some(direction) = line.direction.as_deref() {
            let mut direction_style = value_style.clone();
            direction_style.x = line_x;
            direction_style.y = line_y;
            direction_style.color =
                rgba_color(validated.unit_color, scene_opacity(base_style, validated));
            let (direction_bounds, direction_content) = text_bounds(
                direction,
                line_vertical_metrics_text,
                &direction_style,
                fonts,
            )?;
            bounds.join(direction_bounds);
            content_bounds.join(direction_content);
            texts.push(PreparedText {
                text: direction.to_owned(),
                reference: line_vertical_metrics_text.to_owned(),
                style: direction_style,
            });
        }

        let mut number_style = value_style.clone();
        number_style.x = line_x
            + if direction_column_width > 0.0 {
                direction_column_width + direction_gap + (value_column_width - value_width)
            } else {
                value_column_width - value_width
            };
        number_style.y = line_y;
        let (number_bounds, number_content) =
            text_bounds(value_text, line_vertical_metrics_text, &number_style, fonts)?;
        bounds.join(number_bounds);
        content_bounds.join(number_content);
        texts.push(PreparedText {
            text: value_text.clone(),
            reference: line_vertical_metrics_text.to_owned(),
            style: number_style,
        });
    }
    Ok(finish_frame(
        MetricFrame {
            value_style,
            unit: None,
            icon,
            bounds,
            content_bounds,
            content_width: horizontal_layout.content_width,
            coordinate_text: texts,
        },
        base_style,
        parts,
        Rect::from_xywh(
            horizontal_layout.row_origin_x,
            base_style.y,
            horizontal_layout.content_width,
            row_height,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::catalog::MetricKind;
    use crate::model::validation::Precision;

    fn value_widget(alignment: ContentAlignment) -> ValidatedValueWidget {
        ValidatedValueWidget {
            metric: MetricKind::Speed,
            x: 300.0,
            y: 0.0,
            display_type: DisplayType::Text,
            content_alignment: alignment,
            font_name: "Arial.ttf".to_string(),
            font_size: 100.0,
            color: [255; 4],
            opacity: 1.0,
            show_icon: true,
            icon_color: [255; 4],
            icon_size: 45.0,
            icon_offset_x: 0.0,
            icon_offset_y: 0.0,
            show_units: true,
            show_full_distance: None,
            show_full_ascent: None,
            coordinate_format: None,
            unit_color: [255; 4],
            display_unit: "kmh".to_string(),
            starting_altitude_m: None,
            prefix: String::new(),
            suffix: String::new(),
            precision: Precision::Decimals(0),
            balance_format: None,
            hours_offset: None,
            format: None,
        }
    }

    #[test]
    fn units_and_icons_take_the_widget_opacity_once() {
        let mut validated = value_widget(ContentAlignment::Left);
        validated.opacity = 0.5;
        let scene = crate::model::scene::ValidatedScenePresentation {
            width: 100,
            height: 100,
            scale: 1.0,
            font: None,
            font_size: None,
            opacity: 0.8,
            decimal_rounding: None,
            time_format: None,
            shadow_color: String::new(),
            shadow_strength: 0.0,
            shadow_distance: 0.0,
            border_color: String::new(),
            border_thickness: 0.0,
            numerals: crate::model::scene::Numerals::Default,
            overlay_filename: None,
        };
        let style = crate::render::text::validated_value_style(&validated, &scene, 1.0);
        assert!((scene_opacity(&style, &validated) - 0.8).abs() < 1e-6);
        // Validated colours carry the widget opacity, so the unit fades with the value.
        validated.unit_color = [255, 255, 255, 128];
        let unit = rgba_color(validated.unit_color, scene_opacity(&style, &validated));
        assert_eq!(unit.a(), (128.0f32 * 0.8).round() as u8);
        validated.opacity = 0.0;
        assert_eq!(scene_opacity(&style, &validated), 0.0);
    }

    #[test]
    fn point_anchor_origins_follow_the_canonical_formula() {
        for (alignment, expected) in [
            (ContentAlignment::Left, 300.0),
            (ContentAlignment::Center, 240.0),
            (ContentAlignment::Right, 180.0),
        ] {
            let layout =
                metric_horizontal_layout(alignment, 300.0, Some(20.0), 60.0, Some(20.0), 20.0);
            assert_eq!(layout.content_width, 120.0);
            assert_eq!(layout.row_origin_x, expected);
            assert_eq!(layout.icon_x, Some(expected));
            assert_eq!(layout.value_x, expected + 20.0);
            assert_eq!(layout.unit_x, Some(expected + 100.0));
        }
    }

    #[test]
    fn static_metric_parts_follow_alignment_and_visibility() {
        let mut widget = value_widget(ContentAlignment::Left);
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts {
                icon: true,
                unit: false
            }
        );

        widget.content_alignment = ContentAlignment::Center;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );

        widget.content_alignment = ContentAlignment::Right;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts {
                icon: false,
                unit: true
            }
        );

        widget.metric = MetricKind::Time;
        widget.show_units = false;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );
        widget.content_alignment = ContentAlignment::Left;
        assert!(static_metric_parts_for_value(&widget).icon);

        widget.metric = MetricKind::Speed;
        widget.show_icon = false;
        widget.show_units = false;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );
    }

    #[test]
    fn static_eligibility_remains_per_widget() {
        let cached = value_widget(ContentAlignment::Left);
        let dynamic = value_widget(ContentAlignment::Center);

        assert!(static_metric_parts_for_value(&cached).icon);
        assert_eq!(
            static_metric_parts_for_value(&dynamic),
            StaticMetricParts::default()
        );
    }
}
