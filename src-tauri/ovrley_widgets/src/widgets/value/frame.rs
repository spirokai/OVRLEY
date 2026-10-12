//! Layout is shared by drawing and geometry reporting. All coordinates are scene pixels.
use crate::widgets::value::layout::{
    metric_horizontal_layout, metric_icon_top_from_value_layout, metric_vertical_metrics_text,
    METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
};
use crate::{
    error::WidgetResult,
    render::icons::metric_icon_bounds,
    render::text::{measure_text, parse_color, FontResolver, ResolvedTextStyle},
    widgets::value::{format::MetricDisplayParts, validation::ValidatedValueWidget},
};
use skia_safe::Rect;

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

fn text_bounds(
    text: &str,
    reference: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<Rect> {
    let ink = measure_text(text, style, fonts)?;
    let vertical = measure_text(reference, style, fonts)?;
    let baseline = style.y
        + (style.line_height - (vertical.bounds_bottom - vertical.bounds_top)) * 0.5
        - vertical.bounds_top;
    Ok(Rect::new(
        style.x + ink.bounds_left,
        baseline + ink.bounds_top,
        style.x + ink.bounds_right,
        baseline + ink.bounds_bottom,
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
    let value_height = style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let mut unit_style = style.clone();
    unit_style.font_size =
        (style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
    unit_style.line_height = unit_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
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
    let mut bounds = text_bounds(
        value,
        metric_vertical_metrics_text(value),
        &value_style,
        fonts,
    )?;
    // Cached left-aligned icons use the stable numeric reference in native composition.
    let icon_vertical = if crate::widgets::value::layout::static_metric_parts_for_value(widget).icon
    {
        measure_text(
            crate::widgets::value::layout::NUMERIC_VERTICAL_METRICS_TEXT,
            style,
            fonts,
        )?
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
        // Preserve the existing renderer's color/opacity semantics.
        unit_style.color = parse_color(
            &format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                widget.unit_color[0],
                widget.unit_color[1],
                widget.unit_color[2],
                widget.unit_color[3]
            ),
            style.opacity,
        );
        bounds.join(text_bounds(text, reference, &unit_style, fonts)?);
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
            content_bounds: bounds,
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
    let line_height = coordinate_font_size * METRIC_WIDGET_LINE_HEIGHT;
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
    for line in &coordinate.lines {
        let direction_measure = line
            .direction
            .as_deref()
            .map(|direction| measure_text(direction, &value_style, fonts))
            .transpose()?;
        let value_measure = measure_text(&line.value_text, &value_style, fonts)?;
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
    let unit_color_hex =
        crate::widgets::value::layout::ColorHexSlice(validated.unit_color).to_hex_string();
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
    for (index, line) in coordinate.lines.iter().enumerate() {
        let line_y = text_top + index as f32 * (line_height + line_gap);
        let (_, value_width) = line_measurements[index];
        let line_x = horizontal_layout.value_x;
        let line_vertical_metrics_text = metric_vertical_metrics_text(&line.value_text);
        if let Some(direction) = line.direction.as_deref() {
            let mut direction_style = value_style.clone();
            direction_style.x = line_x;
            direction_style.y = line_y;
            direction_style.color = parse_color(&unit_color_hex, base_style.opacity);
            bounds.join(text_bounds(
                direction,
                line_vertical_metrics_text,
                &direction_style,
                fonts,
            )?);
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
        bounds.join(text_bounds(
            &line.value_text,
            line_vertical_metrics_text,
            &number_style,
            fonts,
        )?);
        texts.push(PreparedText {
            text: line.value_text.clone(),
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
            content_bounds: bounds,
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
