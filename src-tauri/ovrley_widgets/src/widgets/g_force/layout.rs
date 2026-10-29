//! Shared static friction circle and logical frame geometry.
use super::{format::GForceSample, validation::ValidatedGForceWidget};
use crate::{
    error::WidgetResult,
    model::scene::ValidatedScenePresentation,
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        shadow::shadow_paint,
        text::{parse_color, FontResolver},
    },
    widgets::value::preview::SHAPE_COLOR_TYPE,
};
use skia_safe::{paint::Style, BlendMode, ClipOp, Paint, PathBuilder, PathFillType, Point, Rect};

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
    /// Static dial relative to the widget origin (x, y).
    pub parent_circle: StaticRaster,
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

/// Rasterizes the static dial at the host's density.
///
/// Matching the host's density keeps thin guides intact; resampling a scene-density
/// dial onto a reduced preview raster drops whole pixel rows and columns.
pub fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedScenePresentation,
    raster_scale: PreviewRasterScale,
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
    let padding = (border_thickness.max(1.0) * 0.5 + shadow_extent).ceil();
    let parent_circle = StaticRaster::new(
        raster_scale,
        Point::new(widget.x, widget.y),
        Rect::new(
            -padding,
            -padding,
            layout.width.ceil() + padding,
            layout.height.ceil() + padding,
        ),
        SHAPE_COLOR_TYPE,
        |canvas| {
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
                    canvas
                        .translate((scene.shadow_distance * scale, scene.shadow_distance * scale));
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
            Ok(())
        },
    )?;
    Ok(GForceCache {
        config: widget.clone(),
        scene: scene.clone(),
        layout,
        x: widget.x,
        y: widget.y,
        parent_circle,
    })
}
