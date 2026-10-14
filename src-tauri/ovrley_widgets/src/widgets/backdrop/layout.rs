//! Static backdrop widget drawing.

use super::{frame::prepare_backdrop_frame, validation::ValidatedBackdrop};
use crate::model::types::BackdropType;
use crate::render::text::parse_color;
use skia_safe::{paint::Style, Canvas, Paint, Path, PathBuilder, Point};

#[derive(Clone, Copy)]
struct RectangleCorners {
    top_left: bool,
    top_right: bool,
    bottom_right: bool,
    bottom_left: bool,
}

pub fn draw_backdrops_static_layer(
    canvas: &Canvas,
    backdrops: &[ValidatedBackdrop],
    scale: f32,
    scene_opacity: f32,
) {
    for backdrop in backdrops {
        prepare_backdrop_frame(backdrop, scale).draw(canvas, scene_opacity);
    }
}

pub(crate) fn draw_backdrop(canvas: &Canvas, backdrop: &ValidatedBackdrop, scale: f32, opacity: f32) {
    match backdrop.display_type {
        BackdropType::Circle => draw_circle_backdrop(canvas, backdrop, scale, opacity),
        BackdropType::Rectangle => draw_rectangle_backdrop(canvas, backdrop, scale, opacity),
    }
}

fn backdrop_paint(color: &str, opacity: f32, style: Style) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(style);
    paint.set_color(parse_color(color, opacity));
    paint
}

fn draw_circle_backdrop(canvas: &Canvas, backdrop: &ValidatedBackdrop, scale: f32, opacity: f32) {
    let diameter = backdrop.diameter * scale;
    let border = (backdrop.border_thickness * scale).max(0.0);
    let center = Point::new(backdrop.x + diameter * 0.5, backdrop.y + diameter * 0.5);
    let fill_alpha = opacity * backdrop.fill_opacity;
    let fill_paint = backdrop_paint(&backdrop.fill_color, fill_alpha, Style::Fill);

    canvas.draw_circle(center, (diameter * 0.5 - border).max(0.0), &fill_paint);

    if backdrop.border_thickness <= 0.0 {
        return;
    }

    let border_alpha = opacity * backdrop.border_opacity;
    let mut border_paint = backdrop_paint(&backdrop.border_color, border_alpha, Style::Stroke);
    border_paint.set_stroke_width(border);
    canvas.draw_circle(center, (diameter - border).max(0.0) * 0.5, &border_paint);
}

fn draw_rectangle_backdrop(canvas: &Canvas, backdrop: &ValidatedBackdrop, scale: f32, opacity: f32) {
    let width = backdrop.width * scale;
    let height = backdrop.height * scale;
    let border = (backdrop.border_thickness * scale).max(0.0);
    let radius = (backdrop.corner_radius * scale).max(0.0);
    let corners = RectangleCorners {
        top_left: backdrop.round_top_left,
        top_right: backdrop.round_top_right,
        bottom_right: backdrop.round_bottom_right,
        bottom_left: backdrop.round_bottom_left,
    };
    let fill_alpha = opacity * backdrop.fill_opacity;
    let fill_paint = backdrop_paint(&backdrop.fill_color, fill_alpha, Style::Fill);
    let fill_path = rounded_rect_path(
        backdrop.x + border,
        backdrop.y + border,
        (width - border * 2.0).max(0.0),
        (height - border * 2.0).max(0.0),
        (radius - border).max(0.0),
        corners,
    );

    canvas.draw_path(&fill_path, &fill_paint);

    if backdrop.border_thickness <= 0.0 {
        return;
    }

    let border_alpha = opacity * backdrop.border_opacity;
    let mut border_paint = backdrop_paint(&backdrop.border_color, border_alpha, Style::Stroke);
    border_paint.set_stroke_width(border);
    let stroke_inset = border * 0.5;
    let stroke_path = rounded_rect_path(
        backdrop.x + stroke_inset,
        backdrop.y + stroke_inset,
        (width - border).max(0.0),
        (height - border).max(0.0),
        radius,
        corners,
    );

    canvas.draw_path(&stroke_path, &border_paint);
}

fn rounded_rect_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    corners: RectangleCorners,
) -> Path {
    let radius = radius.max(0.0).min(width * 0.5).min(height * 0.5);
    let top_left = if corners.top_left { radius } else { 0.0 };
    let top_right = if corners.top_right { radius } else { 0.0 };
    let bottom_right = if corners.bottom_right { radius } else { 0.0 };
    let bottom_left = if corners.bottom_left { radius } else { 0.0 };
    let right = x + width;
    let bottom = y + height;
    let mut path = PathBuilder::new();

    path.move_to(Point::new(x + top_left, y));
    path.line_to(Point::new(right - top_right, y));
    if top_right > 0.0 {
        path.quad_to(Point::new(right, y), Point::new(right, y + top_right));
    } else {
        path.line_to(Point::new(right, y));
    }

    path.line_to(Point::new(right, bottom - bottom_right));
    if bottom_right > 0.0 {
        path.quad_to(
            Point::new(right, bottom),
            Point::new(right - bottom_right, bottom),
        );
    } else {
        path.line_to(Point::new(right, bottom));
    }

    path.line_to(Point::new(x + bottom_left, bottom));
    if bottom_left > 0.0 {
        path.quad_to(Point::new(x, bottom), Point::new(x, bottom - bottom_left));
    } else {
        path.line_to(Point::new(x, bottom));
    }

    path.line_to(Point::new(x, y + top_left));
    if top_left > 0.0 {
        path.quad_to(Point::new(x, y), Point::new(x + top_left, y));
    } else {
        path.line_to(Point::new(x, y));
    }
    path.close();
    path.detach()
}
