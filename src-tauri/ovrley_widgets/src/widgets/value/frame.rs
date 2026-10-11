//! Layout is shared by drawing and geometry reporting. All coordinates are scene pixels.
use crate::widgets::value::layout::{
    metric_horizontal_layout, metric_icon_top_from_value_layout, metric_vertical_metrics_text,
    METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
};
use crate::{
    error::WidgetResult,
    render::text::{measure_text, parse_color, FontResolver, ResolvedTextStyle},
    widgets::value::{format::MetricDisplayParts, validation::ValidatedValueWidget},
};
use skia_safe::Rect;

pub struct MetricFrame {
    pub value_style: ResolvedTextStyle,
    pub unit: Option<(ResolvedTextStyle, String)>,
    pub icon: Option<Rect>,
    pub bounds: Rect,
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
    let icon = horizontal.icon_x.map(|x| {
        Rect::from_xywh(
            x + widget.icon_offset_x * scale,
            metric_icon_top_from_value_layout(bottom, value_height, &vertical, icon_size)
                + widget.icon_offset_y * scale,
            icon_size,
            icon_size,
        )
    });
    if let Some(rect) = icon {
        bounds.join(rect);
    }
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
    // Include the line box as well as glyph overhangs, icon offsets, strokes, and shadows.
    bounds.join(Rect::from_xywh(
        horizontal.row_origin_x,
        style.y,
        horizontal.content_width,
        row_height,
    ));
    let padding = 2.0
        + style.border_thickness * 0.5
        + if style.shadow_color.is_some() {
            style.shadow_strength * 3.0 + style.shadow_distance.abs()
        } else {
            0.0
        };
    bounds.outset((padding, padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    Ok(MetricFrame {
        value_style,
        unit,
        icon,
        bounds,
        content_width: horizontal.content_width,
    })
}
