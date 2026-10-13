//! One gradient layout for native drawing, preview pixels and interaction bounds.
use super::{format::format_validated_gradient, validation::ValidatedGradientWidget};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{draw_text, measure_text, parse_color, FontResolver, ResolvedTextStyle},
};
use skia_safe::{paint::Style, Canvas, Color, Paint, PaintCap, Path, PathBuilder, Point, Rect};

const TRIANGLE_GAP: f32 = 8.0;
const ZERO_EPSILON: f64 = 0.05;
const MAX_GRADIENT: f64 = 25.0;

pub fn validated_gradient_style(
    widget: &ValidatedGradientWidget,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: widget.x,
        y: widget.y,
        font_name: Some(widget.font_name.clone()),
        font_size: widget.font_size * scale,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: widget.font_size * scale * 0.92,
        color: rgba_color(widget.color),
        opacity: widget.opacity,
        shadow_color: (!scene.shadow_color.is_empty())
            .then(|| parse_color(&scene.shadow_color, widget.opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: (!scene.border_color.is_empty())
            .then(|| parse_color(&scene.border_color, widget.opacity)),
        border_thickness: scene.border_thickness * scale,
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
        draw_text(canvas, &self.value.0, &self.value.1, fonts)?;
        if let Some(style) = &self.unit {
            draw_text(canvas, "%", style, fonts)?;
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
    fonts: &dyn FontResolver,
) -> WidgetResult<GradientFrame> {
    let formatted = format_validated_gradient(widget, raw);
    let value_measure = measure_text(&formatted, base_style, fonts)?;
    let line_height = base_style.font_size * 0.92;
    let triangle_width = widget.triangle_width * scale;
    let show_triangle = widget.show_triangle && triangle_width > 0.0;
    let content_width = value_measure
        .width
        .max(if show_triangle { triangle_width } else { 0.0 });
    // Indicator width changes its shape around this fixed baseline.
    let zero_baseline = base_style.y + line_height + TRIANGLE_GAP * scale;
    let mut value_style = base_style.clone();
    value_style.x = base_style.x + (content_width - value_measure.width) * 0.5;
    // value_offset retains its existing scene-pixel meaning in native export.
    value_style.y = base_style.y - widget.value_offset;
    value_style.line_height = line_height;
    let (value_text, unit) = if let Some(prefix) = formatted.strip_suffix('%') {
        let mut unit_style = value_style.clone();
        unit_style.x += measure_text(prefix, &value_style, fonts)?.width;
        unit_style.color = rgba_color(widget.unit_color);
        (prefix.to_owned(), Some(unit_style))
    } else {
        (formatted, None)
    };
    let mut content_bounds = text_bounds(&value_text, &value_style, fonts)?;
    if let Some(style) = &unit {
        content_bounds.join(text_bounds("%", style, fonts)?);
    }
    if base_style.border_color.is_some() {
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
        paint.set_color(rgba_color(if grade < 0.0 {
            widget.triangle_negative_color
        } else {
            widget.triangle_positive_color
        }));
        let mut path = PathBuilder::new();
        path.move_to(Point::new(left, zero_baseline));
        path.line_to(Point::new(left + triangle_width, zero_baseline));
        if grade.abs() <= ZERO_EPSILON {
            let stroke_width = scale.max(1.0);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(stroke_width);
            paint.set_stroke_cap(PaintCap::Round);
            content_bounds.join(Rect::new(
                left - stroke_width * 0.5,
                zero_baseline - stroke_width * 0.5,
                left + triangle_width + stroke_width * 0.5,
                zero_baseline + stroke_width * 0.5,
            ));
        } else {
            let height = gradient_triangle_height(raw, triangle_width);
            let tip = zero_baseline + if grade < 0.0 { height } else { -height };
            path.line_to(Point::new(left + triangle_width, tip));
            path.close();
            paint.set_style(Style::Fill);
            content_bounds.join(Rect::new(
                left,
                zero_baseline.min(tip),
                left + triangle_width,
                zero_baseline.max(tip),
            ));
        }
        Some((path.detach(), paint))
    } else {
        None
    };
    let mut bounds = content_bounds;
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
) -> WidgetResult<Rect> {
    let ink = measure_text(text, style, fonts)?;
    let baseline =
        style.y + (style.line_height - (ink.bounds_bottom - ink.bounds_top)) * 0.5 - ink.bounds_top;
    Ok(Rect::new(
        style.x + ink.bounds_left,
        baseline + ink.bounds_top,
        style.x + ink.bounds_right,
        baseline + ink.bounds_bottom,
    ))
}

fn rgba_color(color: [u8; 4]) -> Color {
    Color::from_argb(color[3], color[0], color[1], color[2])
}

/// Preserves the existing full-angle triangle mapping, capped at 25 percent.
pub fn gradient_triangle_height(raw: Option<f64>, width: f32) -> f32 {
    let magnitude = raw.unwrap_or(0.0).abs().min(MAX_GRADIENT);
    if width <= 0.0 || magnitude <= ZERO_EPSILON {
        return 0.0;
    }
    width * (magnitude as f32).to_radians().tan()
}
