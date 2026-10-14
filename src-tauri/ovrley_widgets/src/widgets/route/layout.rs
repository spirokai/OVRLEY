//! Cached remaining route and vector drawing; one frame supplies paint and interaction bounds.
use super::{
    geometry::{RouteFrameState, RouteGeometry},
    validation::ValidatedRoutePlot,
};
use crate::{
    error::WidgetResult,
    model::scene::ValidatedScenePresentation,
    render::text::parse_color,
    widgets::value::preview::{surface, ContentBounds},
};
use skia_safe::{
    BlurStyle, Canvas, Color, Image, MaskFilter, Paint, PaintCap, PaintJoin, Path, Rect,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RouteLayout {
    pub config: ValidatedRoutePlot,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<RouteGeometry>,
    remaining: Image,
    remaining_bounds: Rect,
}

pub struct RouteFrame {
    pub state: RouteFrameState,
    pub bounds: Rect,
    /// Unrotated selection rectangle in scene pixels; rotation uses its center.
    pub content_bounds: ContentBounds,
}

fn scaled_rect(rect: Rect, scale: [f32; 2]) -> Rect {
    Rect::new(
        rect.left * scale[0],
        rect.top * scale[1],
        rect.right * scale[0],
        rect.bottom * scale[1],
    )
}

fn rotated_rect(rect: Rect, rotation: f32, x: f32, y: f32) -> Rect {
    let (sin, cos) = rotation.to_radians().sin_cos();
    let corners = [
        (rect.left, rect.top),
        (rect.right, rect.top),
        (rect.left, rect.bottom),
        (rect.right, rect.bottom),
    ];
    let mut bounds = Rect::new(
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for (px, py) in corners {
        let (rx, ry) = (x + px * cos - py * sin, y + px * sin + py * cos);
        bounds = Rect::new(
            bounds.left.min(rx),
            bounds.top.min(ry),
            bounds.right.max(rx),
            bounds.bottom.max(ry),
        );
    }
    bounds
}

fn outward(rect: Rect) -> Rect {
    Rect::new(
        rect.left.floor(),
        rect.top.floor(),
        rect.right.ceil().max(rect.left.floor() + 1.0),
        rect.bottom.ceil().max(rect.top.floor() + 1.0),
    )
}

fn stroke_paint(color: &str, width: f32, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(parse_color(color, opacity));
    paint
}

impl RouteLayout {
    pub fn new(
        config: ValidatedRoutePlot,
        scene: ValidatedScenePresentation,
        geometry: Arc<RouteGeometry>,
    ) -> WidgetResult<Self> {
        let scale = geometry.scale_to(&config).map(|value| value * scene.scale);
        let width = (config.remaining_line_width * scene.scale).max(1.0);
        let line_bounds =
            scaled_rect(geometry.point_bounds, scale).with_outset((width * 0.5, width * 0.5));
        let mut remaining_bounds = line_bounds;
        let (sin, cos) = config.rotation.to_radians().sin_cos();
        let distance = scene.shadow_distance * scene.scale;
        let offset = [(cos + sin) * distance, (cos - sin) * distance];
        let blur = scene.shadow_strength * scene.scale;
        if blur > 0.0 {
            remaining_bounds.join(
                line_bounds
                    .with_outset((blur * 3.0, blur * 3.0))
                    .with_offset((offset[0], offset[1])),
            );
        }
        // The cached image is local to the widget; dragging only changes its presentation.
        remaining_bounds = outward(remaining_bounds.with_outset((1.0, 1.0)));
        let mut layer = surface(
            remaining_bounds.width() as u32,
            remaining_bounds.height() as u32,
        )?;
        let canvas = layer.canvas();
        let path = geometry.path.lock().expect("route path lock");
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-remaining_bounds.left, -remaining_bounds.top));
        let opacity = config.remaining_line_opacity * scene.opacity;
        if blur > 0.0 {
            canvas.save();
            canvas.translate((offset[0], offset[1]));
            canvas.scale((scale[0], scale[1]));
            let mut paint = stroke_paint(&scene.shadow_color, width / scale[0], opacity);
            paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur / scale[0], true));
            canvas.draw_path(&path, &paint);
            canvas.restore();
        }
        canvas.save();
        canvas.scale((scale[0], scale[1]));
        canvas.draw_path(
            &path,
            &stroke_paint(&config.remaining_line_color, width / scale[0], opacity),
        );
        canvas.restore();
        drop(path);
        Ok(Self {
            config,
            scene,
            geometry,
            remaining: layer.image_snapshot(),
            remaining_bounds,
        })
    }

    /// Selection follows the entire route stroke and never the moving marker.
    pub fn measure_content(
        config: &ValidatedRoutePlot,
        scene: &ValidatedScenePresentation,
        geometry: &RouteGeometry,
    ) -> Rect {
        let scale = geometry.scale_to(config).map(|value| value * scene.scale);
        let half_stroke =
            (config.completed_line_width.max(config.remaining_line_width) * scene.scale).max(1.0)
                * 0.5;
        scaled_rect(geometry.point_bounds, scale).with_outset((half_stroke, half_stroke))
    }

    pub fn frame(&self, progress: f32) -> RouteFrame {
        let state = self.geometry.state(progress);
        let content = Self::measure_content(&self.config, &self.scene, &self.geometry);
        let mut raster = content;
        let scale = self
            .geometry
            .scale_to(&self.config)
            .map(|value| value * self.scene.scale);
        let marker_radius = (self.config.marker_size * self.scene.scale).max(2.0);
        let radius = match self.config.marker_variant.as_str() {
            "ring" => marker_radius
                .max(self.config.marker_variant_diameter * self.scene.scale * 0.5 + 0.75),
            "halo" => {
                marker_radius.max(self.config.marker_variant_diameter * self.scene.scale * 0.5)
            }
            _ => marker_radius,
        };
        // Allocate marker overflow for the entire path once; playback cannot resize the raster.
        raster.join(scaled_rect(self.geometry.point_bounds, scale).with_outset((radius, radius)));
        if self.config.remaining_line_opacity > 0.0 {
            raster.join(self.remaining_bounds);
        }
        raster.outset((1.0, 1.0));
        let center = (
            content.left + content.width() * 0.5,
            content.top + content.height() * 0.5,
        );
        let bounds = outward(rotated_rect(
            raster,
            self.config.rotation,
            self.config.x,
            self.config.y,
        ));
        let (sin, cos) = self.config.rotation.to_radians().sin_cos();
        // x/y retain the existing plot-origin contract. The DOM frame rotates around
        // its center while occupying exactly the same scene pixels as the saved route.
        let content_bounds = ContentBounds {
            left: self.config.x + center.0 * cos - center.1 * sin - content.width() * 0.5,
            top: self.config.y + center.0 * sin + center.1 * cos - content.height() * 0.5,
            width: content.width(),
            height: content.height(),
        };
        RouteFrame {
            state,
            bounds,
            content_bounds,
        }
    }

    pub fn draw(&self, canvas: &Canvas, frame: &RouteFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        canvas.rotate(self.config.rotation, None);
        canvas.draw_image(
            &self.remaining,
            (self.remaining_bounds.left, self.remaining_bounds.top),
            None,
        );
        let scale = self
            .geometry
            .scale_to(&self.config)
            .map(|value| value * self.scene.scale);
        let completed: Path = self.geometry.completed_path(&frame.state);
        canvas.save();
        canvas.scale((scale[0], scale[1]));
        canvas.draw_path(
            &completed,
            &stroke_paint(
                &self.config.completed_line_color,
                (self.config.completed_line_width * self.scene.scale).max(1.0) / scale[0],
                self.config.completed_line_opacity * self.scene.opacity,
            ),
        );
        canvas.restore();
        let point = (
            frame.state.marker_x * scale[0],
            frame.state.marker_y * scale[1],
        );
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let opacity = self.config.marker_opacity * self.scene.opacity;
        let variant_radius = self.config.marker_variant_diameter * self.scene.scale * 0.5;
        match self.config.marker_variant.as_str() {
            "ring" => {
                paint.set_color(parse_color(&self.config.marker_color, opacity));
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(1.5);
                canvas.draw_circle(point, variant_radius, &paint);
            }
            "halo" => {
                paint.set_color(parse_color(&self.config.marker_color, opacity * 0.35));
                canvas.draw_circle(point, variant_radius, &paint);
            }
            _ => {}
        }
        paint.set_style(skia_safe::paint::Style::Fill);
        paint.set_color(parse_color(&self.config.marker_color, opacity));
        canvas.draw_circle(
            point,
            (self.config.marker_size * self.scene.scale).max(2.0),
            &paint,
        );
        canvas.restore();
    }
}
