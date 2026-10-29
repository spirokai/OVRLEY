//! Min/max label measurement and placement for linear gauges.
//!
//! Layout is computed in widget-local coordinates before the static surface is
//! allocated. The measured overflow becomes surface padding, which prevents
//! labels from being clipped while keeping them attached to widget rotation.

use super::super::metric::format_gauge_label;
use crate::render::text::origin_x_for_centered_text;
use crate::widgets::gauges::validation::{
    ValidatedLinearGaugeLabelPosition, ValidatedLinearGaugeOrientation, ValidatedLinearGaugeWidget,
};
use skia_safe::Point;

const LABEL_GAP_PX: f32 = 8.0;

#[derive(Clone, Debug)]
/// Resolved text and baselines for both range endpoints.
pub(super) struct LinearGaugeLabelLayout {
    pub min_label: String,
    pub max_label: String,
    pub min_origin: Point,
    pub max_origin: Point,
}

fn label_gap(font_size: f32) -> f32 {
    (font_size * 0.35).max(LABEL_GAP_PX)
}

/// Places range labels according to the validated orientation/side pairing.
pub(super) fn label_layout(
    gauge: &ValidatedLinearGaugeWidget,
    width: f32,
    height: f32,
    scale: f32,
    font: &skia_safe::Font,
    min_value: f64,
    max_value: f64,
    altitude_offset_m: f64,
) -> LinearGaugeLabelLayout {
    let gap = label_gap(gauge.min_max_label_font_size * scale);
    let min_label = format_gauge_label(
        gauge.metric,
        gauge.display_unit.as_deref(),
        min_value + altitude_offset_m,
    );
    let max_label = format_gauge_label(
        gauge.metric,
        gauge.display_unit.as_deref(),
        max_value + altitude_offset_m,
    );
    let (_, metrics) = font.metrics();
    let (_, min_bounds) = font.measure_str(&min_label, None);
    let (_, max_bounds) = font.measure_str(&max_label, None);

    match (gauge.orientation, gauge.min_max_label_position) {
        (ValidatedLinearGaugeOrientation::Horizontal, ValidatedLinearGaugeLabelPosition::Top) => {
            let baseline = -gap - metrics.descent;
            LinearGaugeLabelLayout {
                min_origin: Point::new(origin_x_for_centered_text(&min_label, 0.0, font), baseline),
                max_origin: Point::new(
                    origin_x_for_centered_text(&max_label, width, font),
                    baseline,
                ),
                min_label,
                max_label,
            }
        }
        (
            ValidatedLinearGaugeOrientation::Horizontal,
            ValidatedLinearGaugeLabelPosition::Bottom,
        ) => {
            let baseline = height + gap - metrics.ascent;
            LinearGaugeLabelLayout {
                min_origin: Point::new(origin_x_for_centered_text(&min_label, 0.0, font), baseline),
                max_origin: Point::new(
                    origin_x_for_centered_text(&max_label, width, font),
                    baseline,
                ),
                min_label,
                max_label,
            }
        }
        (ValidatedLinearGaugeOrientation::Vertical, ValidatedLinearGaugeLabelPosition::Left) => {
            LinearGaugeLabelLayout {
                min_origin: Point::new(
                    -gap - min_bounds.right,
                    height - (min_bounds.top + min_bounds.bottom) * 0.5,
                ),
                max_origin: Point::new(
                    -gap - max_bounds.right,
                    -(max_bounds.top + max_bounds.bottom) * 0.5,
                ),
                min_label,
                max_label,
            }
        }
        (ValidatedLinearGaugeOrientation::Vertical, ValidatedLinearGaugeLabelPosition::Right) => {
            LinearGaugeLabelLayout {
                min_origin: Point::new(
                    width + gap - min_bounds.left,
                    height - (min_bounds.top + min_bounds.bottom) * 0.5,
                ),
                max_origin: Point::new(
                    width + gap - max_bounds.left,
                    -(max_bounds.top + max_bounds.bottom) * 0.5,
                ),
                min_label,
                max_label,
            }
        }
        _ => unreachable!("linear gauge label position must match validated orientation"),
    }
}
