//! Shared lean-angle track geometry and static cache.
use super::validation::ValidatedLeanAngleWidget;
use crate::{
    error::WidgetResult,
    model::scene::ValidatedScenePresentation,
    render::{shadow::shadow_paint, text::parse_color},
    widgets::value::preview::{surface, SHAPE_COLOR_TYPE},
};
use skia_safe::{
    paint::Style, BlendMode, ClipOp, Color, Image, Paint, Path, PathBuilder, PathFillType, Point,
};

const START_ANGLE: f32 = 210.0;
const SWEEP_ANGLE: f32 = 120.0;
pub const LEAN_ANGLE_MAX_FILL_SWEEP: f32 = SWEEP_ANGLE * 0.5;
/// Upright: the sector's top, where the fill starts in either lean direction.
pub const LEAN_ANGLE_CENTER_ANGLE: f32 = START_ANGLE + LEAN_ANGLE_MAX_FILL_SWEEP;
const LABEL_LINE_HEIGHT_RATIO: f32 = 0.92;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeanAngleLayout {
    pub width: f32,
    pub height: f32,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
}

/// Derives the logical lean-angle frame from the canonical diameter geometry.
pub fn lean_angle_layout(diameter: f32, track_thickness: f32, font_size: f32) -> LeanAngleLayout {
    let outer_radius = diameter * 0.5;
    let inner_radius = outer_radius - track_thickness;
    let half_sweep_radians = (SWEEP_ANGLE * 0.5).to_radians();
    let sector_min_x = -outer_radius * half_sweep_radians.sin();
    let sector_max_x = outer_radius * half_sweep_radians.sin();
    let sector_min_y = -outer_radius;
    let sector_max_y = -inner_radius * half_sweep_radians.cos();
    let label_line_height = font_size * LABEL_LINE_HEIGHT_RATIO;
    let min_x = sector_min_x;
    let max_x = sector_max_x;
    let min_y = sector_min_y.min(-label_line_height * 0.5);
    let max_y = sector_max_y.max(label_line_height * 0.5);

    LeanAngleLayout {
        width: max_x - min_x,
        height: max_y - min_y,
        min_x,
        min_y,
        max_x,
        max_y,
        center_x: -min_x,
        center_y: -min_y,
        outer_radius,
        inner_radius,
        start_angle: START_ANGLE,
        sweep_angle: SWEEP_ANGLE,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LeanAngleGeometry {
    pub center_x: f32,
    pub center_y: f32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
}

pub(crate) fn geometry_from_layout(layout: LeanAngleLayout) -> LeanAngleGeometry {
    LeanAngleGeometry {
        center_x: layout.center_x,
        center_y: layout.center_y,
        outer_radius: layout.outer_radius,
        inner_radius: layout.inner_radius,
        start_angle: layout.start_angle,
        sweep_angle: layout.sweep_angle,
    }
}

pub(crate) fn inset_geometry(
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) -> LeanAngleGeometry {
    let track_width = geometry.outer_radius - geometry.inner_radius - border_thickness * 2.0;
    let outer_radius = geometry.outer_radius - border_thickness;
    LeanAngleGeometry {
        outer_radius,
        inner_radius: outer_radius - track_width,
        ..geometry
    }
}

/// Static empty track, border and zero guide, reused by both hosts.
#[derive(Clone, Debug)]
pub struct LeanAngleCache {
    config: ValidatedLeanAngleWidget,
    scene: ValidatedScenePresentation,
    pub static_image: Image,
    pub static_image_x: f32,
    pub static_image_y: f32,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub layout: LeanAngleLayout,
    pub track_thickness: f32,
    pub track_border_thickness: f32,
}

impl LeanAngleCache {
    pub fn prepare_frame(
        &self,
        raw: Option<f64>,
        fonts: &dyn crate::render::text::FontResolver,
    ) -> WidgetResult<super::frame::LeanAngleFrame> {
        super::frame::prepare_lean_angle_frame(&self.config, &self.scene, raw, fonts)
    }
}

pub fn prepare_lean_angle_cache(
    widget: &ValidatedLeanAngleWidget,
    scene: &ValidatedScenePresentation,
) -> WidgetResult<LeanAngleCache> {
    let scale = scene.scale;
    let layout = lean_angle_layout(
        widget.diameter * scale,
        widget.track_thickness * scale,
        widget.font_size * scale,
    );
    let geometry = geometry_from_layout(layout);
    let border_thickness = widget.track_border_thickness * scale;
    let outer_path = annular_sector_path(geometry);
    let inner_path = annular_sector_inset_path(geometry, border_thickness);
    let border_path = annular_sector_border_path(geometry, border_thickness);
    let shadow_extent = if border_thickness > 0.0 && scene.shadow_strength > 0.0 {
        (scene.shadow_distance + scene.shadow_strength * 3.0) * scale
    } else {
        0.0
    };
    let padding = (border_thickness.max(1.0) * 0.5 + shadow_extent).ceil();
    let track_bounds = outer_path.compute_tight_bounds();
    let left = track_bounds.left.floor() - padding;
    let top = track_bounds.top.floor() - padding;
    let width = (track_bounds.right.ceil() + padding - left) as u32;
    let height = (track_bounds.bottom.ceil() + padding - top) as u32;
    let mut surface = surface(width, height, SHAPE_COLOR_TYPE)?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-left, -top));
    if border_thickness > 0.0 {
        let mut source = Paint::default();
        source.set_anti_alias(true);
        if let Some(paint) = shadow_paint(
            &source,
            parse_color(&scene.shadow_color, widget.opacity),
            scene.shadow_strength * scale,
        ) {
            canvas.save();
            canvas.translate((scene.shadow_distance * scale, scene.shadow_distance * scale));
            canvas.draw_path(&border_path, &paint);
            canvas.restore();
        }
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Fill);
        paint.set_color(parse_color(&widget.track_border_color, widget.opacity));
        canvas.draw_path(&outer_path, &paint);
        paint.set_blend_mode(BlendMode::Clear);
        canvas.draw_path(&inner_path, &paint);
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Fill);
    paint.set_color(parse_color(
        &widget.track_empty_color,
        widget.track_empty_opacity * widget.opacity,
    ));
    canvas.draw_path(&inner_path, &paint);
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(widget.diameter / 200.0 * scale);
    paint.set_color(parse_color(widget.track_filled_color.leading_color(), 1.0));
    canvas.save();
    canvas.clip_path(&inner_path, ClipOp::Intersect, true);
    canvas.save_layer_alpha_f(None, 0.4 * widget.opacity);
    canvas.draw_line(
        Point::new(geometry.center_x, geometry.center_y - geometry.outer_radius),
        Point::new(geometry.center_x, geometry.center_y - geometry.inner_radius),
        &paint,
    );
    canvas.restore();
    canvas.restore();
    Ok(LeanAngleCache {
        config: widget.clone(),
        scene: scene.clone(),
        static_image: surface.image_snapshot(),
        static_image_x: widget.x + left,
        static_image_y: widget.y + top,
        x: widget.x,
        y: widget.y,
        width: layout.width.ceil() as u32,
        height: layout.height.ceil() as u32,
        rotation: widget.rotation,
        layout,
        track_thickness: widget.track_thickness * scale,
        track_border_thickness: border_thickness,
    })
}

pub(crate) fn annular_sector_path(geometry: LeanAngleGeometry) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_contour(&mut path, geometry);
    path.detach()
}

fn annular_sector_border_path(outer_geometry: LeanAngleGeometry, border_thickness: f32) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_contour(&mut path, outer_geometry);
    append_annular_sector_inset_contour(&mut path, outer_geometry, border_thickness);
    path.detach()
}

pub(crate) fn annular_sector_inset_path(
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) -> Path {
    let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
    append_annular_sector_inset_contour(&mut path, geometry, border_thickness);
    path.detach()
}

fn append_annular_sector_inset_contour(
    builder: &mut PathBuilder,
    geometry: LeanAngleGeometry,
    border_thickness: f32,
) {
    let inner_geometry = inset_geometry(geometry, border_thickness);
    let direction = geometry.sweep_angle.signum();
    let max_angle_offset = geometry.sweep_angle.abs() * 0.5;
    let outer_angle_offset = parallel_side_angle_offset(
        border_thickness,
        inner_geometry.outer_radius,
        max_angle_offset,
    );
    let inner_angle_offset = parallel_side_angle_offset(
        border_thickness,
        inner_geometry.inner_radius,
        max_angle_offset,
    );
    let outer_start_angle = geometry.start_angle + direction * outer_angle_offset;
    let outer_sweep_angle = geometry.sweep_angle - direction * outer_angle_offset * 2.0;
    let inner_end_angle =
        geometry.start_angle + geometry.sweep_angle - direction * inner_angle_offset;
    let inner_sweep_angle = -(geometry.sweep_angle - direction * inner_angle_offset * 2.0);

    builder.move_to(polar_point(
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.outer_radius,
        outer_start_angle,
    ));
    append_circular_arc(
        builder,
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.outer_radius,
        outer_start_angle,
        outer_sweep_angle,
    );
    builder.line_to(polar_point(
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.inner_radius,
        inner_end_angle,
    ));
    append_circular_arc(
        builder,
        inner_geometry.center_x,
        inner_geometry.center_y,
        inner_geometry.inner_radius,
        inner_end_angle,
        inner_sweep_angle,
    );
    builder.close();
}

fn parallel_side_angle_offset(border_thickness: f32, radius: f32, max_angle_offset: f32) -> f32 {
    if border_thickness == 0.0 {
        return 0.0;
    }

    (border_thickness / radius)
        .asin()
        .to_degrees()
        .min(max_angle_offset)
}

fn append_annular_sector_contour(builder: &mut PathBuilder, geometry: LeanAngleGeometry) {
    let end_angle = geometry.start_angle + geometry.sweep_angle;
    builder.move_to(polar_point(
        geometry.center_x,
        geometry.center_y,
        geometry.outer_radius,
        geometry.start_angle,
    ));
    append_circular_arc(
        builder,
        geometry.center_x,
        geometry.center_y,
        geometry.outer_radius,
        geometry.start_angle,
        geometry.sweep_angle,
    );
    builder.line_to(polar_point(
        geometry.center_x,
        geometry.center_y,
        geometry.inner_radius,
        end_angle,
    ));
    append_circular_arc(
        builder,
        geometry.center_x,
        geometry.center_y,
        geometry.inner_radius,
        end_angle,
        -geometry.sweep_angle,
    );
    builder.close();
}

fn append_circular_arc(
    builder: &mut PathBuilder,
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
    sweep_angle: f32,
) {
    let segment_count = ((sweep_angle.abs() / 90.0).ceil() as u32).max(1);
    let segment_sweep = sweep_angle / segment_count as f32;
    for index in 0..segment_count {
        let angle0 = start_angle + segment_sweep * index as f32;
        let angle1 = angle0 + segment_sweep;
        let control_distance = radius * (4.0 / 3.0) * ((angle1 - angle0).to_radians() * 0.25).tan();
        let start = polar_point(center_x, center_y, radius, angle0);
        let end = polar_point(center_x, center_y, radius, angle1);
        let start_tangent = path_tangent(angle0);
        let end_tangent = path_tangent(angle1);
        builder.cubic_to(
            Point::new(
                start.x + start_tangent.x * control_distance,
                start.y + start_tangent.y * control_distance,
            ),
            Point::new(
                end.x - end_tangent.x * control_distance,
                end.y - end_tangent.y * control_distance,
            ),
            end,
        );
    }
}

pub(crate) fn polar_point(center_x: f32, center_y: f32, radius: f32, angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(
        center_x + radius * radians.cos(),
        center_y + radius * radians.sin(),
    )
}

fn path_tangent(angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(-radians.sin(), radians.cos())
}
