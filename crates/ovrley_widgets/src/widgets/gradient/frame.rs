//! One gradient layout for native drawing, preview pixels and interaction bounds.
use super::{format::format_validated_gradient, validation::ValidatedGradientWidget};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{
        draw_text_with_vertical_metrics_text, measure_text, parse_color, rgba_color,
        text_selection_bounds, FontResolver, ResolvedTextStyle,
    },
    widgets::value::layout::{
        metric_vertical_metrics_text, METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNIT_RATIO,
        MIN_UNITS_FONT_SIZE,
    },
};
use skia_safe::{paint::Style, Canvas, Paint, PaintCap, Path, PathBuilder, Point, Rect};

const TRIANGLE_GAP: f32 = 8.0;
const ZERO_EPSILON: f64 = 0.05;
const MAX_GRADIENT: f64 = 30.0;

pub fn validated_gradient_style(
    widget: &ValidatedGradientWidget,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    let opacity = widget.opacity * scene.opacity;
    ResolvedTextStyle {
        x: widget.x,
        y: widget.y,
        font_name: Some(widget.font_name.clone()),
        font_size: widget.font_size * scale,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: widget.font_size * scale * 0.92,
        color: rgba_color(widget.color, scene.opacity),
        opacity,
        shadow_color: (!scene.shadow_color.is_empty())
            .then(|| parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: (!scene.border_color.is_empty())
            .then(|| parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scale,
        numerals: scene.numerals,
    }
}

pub struct GradientFrame {
    value: (String, ResolvedTextStyle),
    unit: Option<ResolvedTextStyle>,
    triangle: Option<(Path, Paint)>,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
}

impl GradientFrame {
    pub fn draw(&self, canvas: &Canvas, fonts: &dyn FontResolver) -> WidgetResult<()> {
        let (text, style) = &self.value;
        draw_text_with_vertical_metrics_text(
            canvas,
            text,
            metric_vertical_metrics_text(text),
            style,
            fonts,
        )?;
        if let Some(style) = &self.unit {
            draw_text_with_vertical_metrics_text(
                canvas,
                "%",
                metric_vertical_metrics_text("%"),
                style,
                fonts,
            )?;
        }
        if let Some((path, paint)) = &self.triangle {
            canvas.draw_path(path, paint);
        }
        Ok(())
    }
}

pub fn prepare_gradient_frame(
    widget: &ValidatedGradientWidget,
    base_style: &ResolvedTextStyle,
    raw: Option<f64>,
    scale: f32,
    scene_opacity: f32,
    fonts: &dyn FontResolver,
) -> WidgetResult<GradientFrame> {
    let formatted = format_validated_gradient(widget, raw);
    let (value_text, has_unit) = match formatted.strip_suffix('%') {
        Some(prefix) => (prefix.to_owned(), true),
        None => (formatted, false),
    };
    let line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let mut value_style = base_style.clone();
    value_style.line_height = line_height;
    let value_width = measure_text(&value_text, &value_style, fonts)?.width;
    // The unit is sized like metric widget units, but sits flush against the digits.
    let mut unit = has_unit.then(|| {
        let mut style = base_style.clone();
        style.font_size =
            (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        style.line_height = style.font_size * METRIC_WIDGET_LINE_HEIGHT;
        style.color = rgba_color(widget.unit_color, scene_opacity);
        style
    });
    let unit_width = match &unit {
        Some(style) => measure_text("%", style, fonts)?.width,
        None => 0.0,
    };
    let text_width = value_width + unit_width;
    let triangle_width = widget.triangle_width * scale;
    let show_triangle = widget.show_triangle && triangle_width > 0.0;
    let content_width = text_width.max(if show_triangle { triangle_width } else { 0.0 });
    // Indicator width changes its shape around this fixed baseline.
    let zero_baseline = base_style.y + line_height + TRIANGLE_GAP * scale;
    value_style.x = base_style.x + (content_width - text_width) * 0.5;
    // value_offset retains its existing scene-pixel meaning in native export.
    value_style.y = base_style.y - widget.value_offset;
    if let Some(style) = &mut unit {
        // Align the unit's reference ink bottom with the value's, as metric widgets do.
        let value_reference =
            measure_text(metric_vertical_metrics_text(&value_text), &value_style, fonts)?;
        let bottom = value_style.y
            + (line_height + value_reference.bounds_bottom - value_reference.bounds_top) * 0.5;
        let unit_reference = measure_text(metric_vertical_metrics_text("%"), style, fonts)?;
        style.x = value_style.x + value_width;
        style.y = bottom
            - (style.line_height + unit_reference.bounds_bottom - unit_reference.bounds_top) * 0.5;
    }
    let (mut bounds, mut content_bounds) = text_bounds(&value_text, &value_style, fonts)?;
    if let Some(style) = &unit {
        let (unit_bounds, unit_content) = text_bounds("%", style, fonts)?;
        bounds.join(unit_bounds);
        content_bounds.join(unit_content);
    }
    if base_style.border_color.is_some() {
        bounds.outset((
            base_style.border_thickness * 0.5,
            base_style.border_thickness * 0.5,
        ));
        content_bounds.outset((
            base_style.border_thickness * 0.5,
            base_style.border_thickness * 0.5,
        ));
    }
    let triangle = if show_triangle {
        let left = base_style.x + (content_width - triangle_width) * 0.5;
        let grade = raw.unwrap_or(0.0);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(rgba_color(
            if grade < 0.0 {
                widget.triangle_negative_color
            } else {
                widget.triangle_positive_color
            },
            scene_opacity,
        ));
        let mut path = PathBuilder::new();
        path.move_to(Point::new(left, zero_baseline));
        path.line_to(Point::new(left + triangle_width, zero_baseline));
        if grade.abs() <= ZERO_EPSILON {
            let stroke_width = scale.max(1.0);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(stroke_width);
            paint.set_stroke_cap(PaintCap::Round);
            let triangle_bounds = Rect::new(
                left - stroke_width * 0.5,
                zero_baseline - stroke_width * 0.5,
                left + triangle_width + stroke_width * 0.5,
                zero_baseline + stroke_width * 0.5,
            );
            bounds.join(triangle_bounds);
            content_bounds.join(triangle_bounds);
        } else {
            let height = gradient_triangle_height(raw, triangle_width);
            let tip = zero_baseline + if grade < 0.0 { height } else { -height };
            path.line_to(Point::new(left + triangle_width, tip));
            path.close();
            paint.set_style(Style::Fill);
            let triangle_bounds = Rect::new(
                left,
                zero_baseline.min(tip),
                left + triangle_width,
                zero_baseline.max(tip),
            );
            bounds.join(triangle_bounds);
            content_bounds.join(triangle_bounds);
        }
        Some((path.detach(), paint))
    } else {
        None
    };
    let padding = 2.0
        + if base_style.shadow_color.is_some() {
            base_style.shadow_strength * 3.0 + base_style.shadow_distance.abs()
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
    Ok(GradientFrame {
        value: (value_text, value_style),
        unit,
        triangle,
        bounds,
        content_bounds,
        content_width,
    })
}

fn text_bounds(
    text: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<(Rect, Rect)> {
    // Baseline comes from the same stable reference the draw uses, so digits don't jump.
    let ink = measure_text(text, style, fonts)?;
    let reference = measure_text(metric_vertical_metrics_text(text), style, fonts)?;
    let baseline = style.y + (style.line_height - (reference.bounds_bottom - reference.bounds_top)) * 0.5
        - reference.bounds_top;
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

/// Draws the percentage as an angle in degrees, exaggerating the slope for
/// readability, capped at 30. The text value is not capped.
pub fn gradient_triangle_height(raw: Option<f64>, width: f32) -> f32 {
    let magnitude = raw.unwrap_or(0.0).abs().min(MAX_GRADIENT);
    if width <= 0.0 || magnitude <= ZERO_EPSILON {
        return 0.0;
    }
    width * (magnitude as f32).to_radians().tan()
}
