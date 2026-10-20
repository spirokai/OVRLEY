//! Shared static friction circle and logical frame geometry.
use super::{format::GForceSample, validation::ValidatedGForceWidget};
use crate::{
    error::WidgetResult,
    model::scene::ValidatedScenePresentation,
    render::{
        shadow::shadow_paint,
        text::{parse_color, FontResolver},
    },
    widgets::value::preview::{surface, SHAPE_COLOR_TYPE},
};
use skia_safe::{
    paint::Style, BlendMode, ClipOp, Color, Image, Paint, PathBuilder, PathFillType, Point,
};

const GUIDE_DEFAULT_THICKNESS_PX: f32 = 2.0;
const GUIDE_REFERENCE_DIAMETER_PX: f32 = 400.0;
const GUIDE_OPACITY: f32 = 0.4;
const GUIDE_RING_RADIUS_RATIOS: [f32; 2] = [0.33, 0.66];

#[derive(Clone, Copy, Debug)]
pub struct GForceLayout {
    pub width: f32,
    pub height: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

pub fn g_force_layout(widget: &ValidatedGForceWidget, scale: f32) -> GForceLayout {
    let width = widget.width * scale;
    let height = widget.height * scale;
    GForceLayout {
        width,
        height,
        center_x: width * 0.5,
        center_y: height * 0.5,
        radius: widget.diameter * scale * 0.5,
    }
}

#[derive(Clone, Debug)]
pub struct GForceCache {
    config: ValidatedGForceWidget,
    scene: ValidatedScenePresentation,
    pub parent_circle_image: Image,
    pub parent_circle_image_x: f32,
    pub parent_circle_image_y: f32,
    pub layout: GForceLayout,
    pub x: f32,
    pub y: f32,
}

impl GForceCache {
    pub fn prepare_frame(
        &self,
        sample: GForceSample,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<super::frame::GForceFrame> {
        super::frame::prepare_g_force_frame(&self.config, &self.scene, sample, fonts)
    }
}

pub fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedScenePresentation,
) -> WidgetResult<GForceCache> {
    let scale = scene.scale;
    let layout = g_force_layout(widget, scale);
    let GForceLayout {
        center_x,
        center_y,
        radius,
        ..
    } = layout;
    let border_thickness = widget.border_thickness * scale;
    let guide_thickness =
        GUIDE_DEFAULT_THICKNESS_PX * (widget.diameter / GUIDE_REFERENCE_DIAMETER_PX) * scale;
    let shadow_extent = if scene.shadow_strength > 0.0 {
        (scene.shadow_distance + scene.shadow_strength * 3.0) * scale
    } else {
        0.0
    };
    let padding = (border_thickness.max(1.0) * 0.5 + shadow_extent).ceil() as u32;
    let mut surface = surface(
        (layout.width.ceil() as u32).saturating_add(padding.saturating_mul(2)),
        (layout.height.ceil() as u32).saturating_add(padding.saturating_mul(2)),
        SHAPE_COLOR_TYPE,
    )?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((padding as f32, padding as f32));
    if border_thickness > 0.0 {
        let mut source = Paint::default();
        source.set_anti_alias(true);
        if let Some(paint) = shadow_paint(
            &source,
            parse_color(&scene.shadow_color, widget.opacity),
            scene.shadow_strength * scale,
        ) {
            let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
            ring_path.add_circle(Point::new(center_x, center_y), radius, None);
            ring_path.add_circle(
                Point::new(center_x, center_y),
                radius - border_thickness,
                None,
            );
            canvas.save();
            canvas.translate((scene.shadow_distance * scale, scene.shadow_distance * scale));
            canvas.draw_path(&ring_path.detach(), &paint);
            canvas.restore();
        }
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(parse_color(
            &widget.border_color,
            widget.border_opacity * widget.opacity,
        ));
        canvas.draw_circle(Point::new(center_x, center_y), radius, &paint);
        paint.set_blend_mode(BlendMode::Clear);
        canvas.draw_circle(
            Point::new(center_x, center_y),
            radius - border_thickness,
            &paint,
        );
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(
        &widget.fill_color,
        widget.fill_opacity * widget.opacity,
    ));
    canvas.draw_circle(
        Point::new(center_x, center_y),
        radius - border_thickness,
        &paint,
    );
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(guide_thickness);
    paint.set_color(parse_color(&widget.marker_color, 1.0));
    let mut clip = PathBuilder::new();
    clip.add_circle(
        Point::new(center_x, center_y),
        radius - border_thickness,
        None,
    );
    canvas.save();
    canvas.clip_path(&clip.detach(), ClipOp::Intersect, true);
    canvas.save_layer_alpha_f(None, GUIDE_OPACITY * widget.opacity);
    let mut guides = PathBuilder::new();
    guides.move_to(Point::new(center_x - radius, center_y));
    guides.line_to(Point::new(center_x + radius, center_y));
    guides.move_to(Point::new(center_x, center_y - radius));
    guides.line_to(Point::new(center_x, center_y + radius));
    for ratio in GUIDE_RING_RADIUS_RATIOS {
        guides.add_circle(Point::new(center_x, center_y), radius * ratio, None);
    }
    canvas.draw_path(&guides.detach(), &paint);
    canvas.restore();
    canvas.restore();
    Ok(GForceCache {
        config: widget.clone(),
        scene: scene.clone(),
        layout,
        x: widget.x,
        y: widget.y,
        parent_circle_image: surface.image_snapshot(),
        parent_circle_image_x: widget.x - padding as f32,
        parent_circle_image_y: widget.y - padding as f32,
    })
}
