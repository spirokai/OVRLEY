/// Gradient widget rendering and triangle math.
///
/// Draws the percentage text with a slope triangle. The triangle uses a fixed
/// maximum visual height (based on the largest allowed grade) so text and
/// triangle layout remain stable across frames.
use crate::config::{RenderConfig, ValueConfig};
use crate::render::text::{draw_text, measure_text, parse_color, ResolvedTextStyle};
use skia_safe::{paint::Style, Canvas, Paint, PaintCap, Path, Point};
use std::path::PathBuf;

const GRADIENT_TRIANGLE_GAP_PX: f32 = 8.0;
const GRADIENT_ZERO_EPSILON: f64 = 0.05;
const MAX_GRADIENT_ABS_PERCENT: f64 = 25.0;
const GRADIENT_ZERO_LINE_WIDTH_PX: f32 = 1.0;
const METRIC_WIDGET_LINE_HEIGHT: f32 = 0.92;

use crate::activity::schema::DenseActivityReport;

/// Draws the gradient value widget — percentage text with a slope triangle.
///
/// # Three-phase layout
///
/// 1. **Layout** — resolve gradient data, measure text, and compute position for
///    the value text and slope triangle so both stay stable across frames.
/// 2. **Text** — draw the formatted percentage at the computed position.
/// 3. **Triangle** — render a zero-line stroke or filled triangle depending on
///    whether the raw grade is near zero, positive, or negative.
///
/// Color switches between positive and negative config colors based on the
/// sign of the raw gradient value. The triangle is filled for non-zero grades
/// and drawn as a flat zero-line stroke when the gradient is within epsilon
/// of zero.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_gradient_value_widget(
    canvas: &Canvas,
    config: &RenderConfig,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[PathBuf],
) -> bool {
    // Phase 1: resolve gradient data, measure text, and compute stable layout coordinates.
    let raw_gradient = dense_activity
        .series
        .gradient
        .get(frame_index)
        .copied()
        .flatten();
    let value_text =
        crate::render::format::format_value(config, value, dense_activity, frame_index);
    let value_measure = measure_text(&value_text, base_style, font_dirs);
    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let value_offset = value.value_offset.unwrap_or(0.0);
    let triangle_width = value.triangle_width.unwrap_or(72.0).max(0.0) * scale;
    let max_triangle_height =
        gradient_triangle_height(Some(MAX_GRADIENT_ABS_PERCENT), triangle_width);
    let show_triangle = value.show_triangle.unwrap_or(true) && triangle_width > 0.0;
    let content_width = value_measure
        .width
        .max(if show_triangle { triangle_width } else { 0.0 });
    let value_left = base_style.x + ((content_width - value_measure.width) * 0.5);
    let triangle_top = base_style.y + value_line_height + (GRADIENT_TRIANGLE_GAP_PX * scale);
    let zero_baseline_y = triangle_top + max_triangle_height;
    let value_top = if show_triangle {
        zero_baseline_y
            - (value_line_height + (GRADIENT_TRIANGLE_GAP_PX * scale) + max_triangle_height)
            - value_offset
    } else {
        base_style.y - value_offset
    };

    let mut value_style = base_style.clone();
    value_style.x = value_left;
    value_style.y = value_top;
    value_style.line_height = value_line_height;
    // Phase 2: draw the formatted percentage text.
    let (gradient_value_prefix, gradient_unit_suffix) = split_gradient_unit_suffix(&value_text);
    if gradient_value_prefix.is_empty() || gradient_unit_suffix.is_none() {
        draw_text(canvas, &value_text, &value_style, font_dirs);
    } else {
        draw_text(canvas, gradient_value_prefix, &value_style, font_dirs);

        let prefix_measure = measure_text(gradient_value_prefix, &value_style, font_dirs);
        let mut unit_style = value_style.clone();
        unit_style.x += prefix_measure.width;
        unit_style.color = parse_color(
            value.unit_color.as_deref().unwrap_or("#ffffff"),
            base_style.opacity,
        );
        draw_text(
            canvas,
            gradient_unit_suffix.unwrap_or_default(),
            &unit_style,
            font_dirs,
        );
    }

    if !show_triangle {
        return true;
    }

    // Phase 3: render the slope triangle — zero-line stroke or filled triangle.
    let triangle_left = base_style.x + ((content_width - triangle_width) * 0.5);
    let triangle_color = if raw_gradient.unwrap_or(0.0) < 0.0 {
        value
            .triangle_negative_color
            .as_deref()
            .unwrap_or("#c65102")
    } else {
        value
            .triangle_positive_color
            .as_deref()
            .unwrap_or("#40e0d0")
    };

    if gradient_is_zero(raw_gradient) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Stroke);
        paint.set_stroke_width((GRADIENT_ZERO_LINE_WIDTH_PX * scale).max(1.0));
        paint.set_stroke_cap(PaintCap::Round);
        paint.set_color(parse_color(triangle_color, base_style.opacity));
        canvas.draw_line(
            Point::new(triangle_left, zero_baseline_y),
            Point::new(triangle_left + triangle_width, zero_baseline_y),
            &paint,
        );
    } else if let Some(path) =
        build_gradient_triangle_path(raw_gradient, triangle_left, zero_baseline_y, triangle_width)
    {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Fill);
        paint.set_color(parse_color(triangle_color, base_style.opacity));
        canvas.draw_path(&path, &paint);
    }

    true
}

fn split_gradient_unit_suffix(text: &str) -> (&str, Option<&str>) {
    text.strip_suffix('%')
        .map(|prefix| (prefix, Some("%")))
        .unwrap_or((text, None))
}

/// Returns whether a gradient should be rendered as a flat zero line.
fn gradient_is_zero(raw_gradient: Option<f64>) -> bool {
    let Some(gradient) = raw_gradient else {
        return true;
    };
    gradient.abs() <= GRADIENT_ZERO_EPSILON
}

/// Computes the visual triangle height for a gradient percentage display.
///
/// Converts a grade percentage (e.g., 10% = 5.7°) to a visual triangle height
/// using `tan(gradient / 2) × width`. The half-angle mapping gives a visually
/// proportional height that feels natural at typical road grades (0–20%).
/// Returns 0.0 when the gradient is missing, zero, or the triangle width is 0.
pub fn gradient_triangle_height(raw_gradient: Option<f64>, triangle_width: f32) -> f32 {
    if triangle_width <= 0.0 {
        return 0.0;
    }

    let Some(gradient) = raw_gradient else {
        return 0.0;
    };
    let magnitude = gradient.abs().min(MAX_GRADIENT_ABS_PERCENT);
    if magnitude <= GRADIENT_ZERO_EPSILON {
        return 0.0;
    }

    let half_angle_radians = ((magnitude * 0.5) as f32).to_radians();
    triangle_width * half_angle_radians.tan()
}

/// Builds the filled triangle path that visualizes positive or negative grade.
fn build_gradient_triangle_path(
    raw_gradient: Option<f64>,
    left: f32,
    baseline_y: f32,
    width: f32,
) -> Option<Path> {
    let height = gradient_triangle_height(raw_gradient, width);
    if height <= 0.0 {
        return None;
    }

    let mut path = Path::new();
    if raw_gradient.unwrap_or(0.0) > 0.0 {
        path.move_to(Point::new(left, baseline_y));
        path.line_to(Point::new(left + width, baseline_y));
        path.line_to(Point::new(left + width, baseline_y - height));
    } else if raw_gradient.unwrap_or(0.0) < 0.0 {
        path.move_to(Point::new(left, baseline_y));
        path.line_to(Point::new(left + width, baseline_y));
        path.line_to(Point::new(left + width, baseline_y + height));
    } else {
        return None;
    }
    path.close();
    Some(path)
}
