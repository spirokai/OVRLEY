//! Value/unit stack layout shared by arc-shaped gauges.
//!
//! Value and optional unit text are measured as one centered vertical stack.
//! The unit is frame-invariant and belongs to the cached static layer; only the
//! formatted metric value is painted during per-frame composition.

use super::geometry::ArcGaugeGeometry;
use crate::render::text::{origin_x_for_centered_text, ResolvedTextStyle};
use crate::widgets::gauges::validation::ValidatedArcGaugeWidget;

pub(crate) const LINE_HEIGHT: f32 = 0.92;
const UNIT_RATIO: f32 = 0.28;
const MIN_UNIT_FONT_SIZE: f32 = 12.0;
pub(crate) const DEFAULT_GAP_PX: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
/// Baselines and total bounds for the centered value/unit stack.
pub(crate) struct ArcInnerWidgetLayout {
    pub(crate) center_x: f32,
    pub(crate) value_top: f32,
    pub(crate) unit_top: Option<f32>,
}

/// Resolves the unit font size relative to the validated value style.
pub(crate) fn unit_font_size(text_style: &ResolvedTextStyle, scale: f32) -> f32 {
    (text_style.font_size * UNIT_RATIO).max(MIN_UNIT_FONT_SIZE * scale)
}

/// Measures and centers the value and optional unit around the inner anchor.
pub(crate) fn inner_widget_layout(
    geometry: ArcGaugeGeometry,
    offset_x: f32,
    offset_y: f32,
    text_style: &ResolvedTextStyle,
    unit_font_size: Option<f32>,
    minimum_gap: f32,
) -> ArcInnerWidgetLayout {
    let value_line_height = text_style.font_size * LINE_HEIGHT;
    let unit_line_height = unit_font_size.unwrap_or(0.0) * LINE_HEIGHT;
    let gap = (text_style.font_size * 0.08).max(minimum_gap);
    let group_height = if unit_font_size.is_some() {
        value_line_height + gap + unit_line_height
    } else {
        value_line_height
    };
    let top = geometry.inner_widget_center_y + offset_y - group_height * 0.5;

    ArcInnerWidgetLayout {
        center_x: geometry.inner_widget_center_x + offset_x,
        value_top: top,
        unit_top: unit_font_size.map(|_| top + value_line_height + gap),
    }
}

/// Draws the optional unit into the frame-invariant static layer.
pub(super) fn unit_style(
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    scale: f32,
    unit_text: &str,
    font: &skia_safe::Font,
    text_style: &ResolvedTextStyle,
) -> ResolvedTextStyle {
    let mut unit_style = text_style.clone();
    unit_style.font_size = unit_font_size(text_style, scale);
    unit_style.line_height = unit_style.font_size * LINE_HEIGHT;
    unit_style.color = skia_safe::Color::from_argb(
        gauge.inner_value.unit_color[3],
        gauge.inner_value.unit_color[0],
        gauge.inner_value.unit_color[1],
        gauge.inner_value.unit_color[2],
    );
    let layout = inner_widget_layout(
        geometry,
        gauge.inner_widget_offset_x * scale,
        gauge.inner_widget_offset_y * scale,
        text_style,
        Some(unit_style.font_size),
        DEFAULT_GAP_PX * scale,
    );
    unit_style.x = origin_x_for_centered_text(unit_text, layout.center_x, font);
    unit_style.y = layout
        .unit_top
        .expect("unit layout must include a top when a unit exists");
    unit_style
}
