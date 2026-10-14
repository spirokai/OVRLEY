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
    remaining_image_bounds: Rect,
    bounds: Rect,
    content_bounds: ContentBounds,
    scale: [f32; 2],
    completed_paint: Paint,
    marker_paint: Paint,
    variant_paint: Paint,
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
        // Cache at the final angle so playback can blit without resampling a rotated
        // bitmap every frame. Positions stay relative to the widget for cheap dragging.
        remaining_bounds = outward(remaining_bounds.with_outset((1.0, 1.0)));
        let remaining_image_bounds =
            outward(rotated_rect(remaining_bounds, config.rotation, 0.0, 0.0));
        let mut layer = surface(
            remaining_image_bounds.width() as u32,
            remaining_image_bounds.height() as u32,
        )?;
        let canvas = layer.canvas();
        let path = geometry.path.lock().expect("route path lock");
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-remaining_image_bounds.left, -remaining_image_bounds.top));
        canvas.rotate(config.rotation, None);
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
        let (bounds, content_bounds) =
            Self::measure_bounds(&config, &scene, &geometry, remaining_bounds);
        let completed_paint = stroke_paint(
            &config.completed_line_color,
            (config.completed_line_width * scene.scale).max(1.0) / scale[0],
            config.completed_line_opacity * scene.opacity,
        );
        let mut marker_paint = Paint::default();
        marker_paint.set_anti_alias(true);
        let marker_opacity = config.marker_opacity * scene.opacity;
        marker_paint.set_color(parse_color(&config.marker_color, marker_opacity));
        let mut variant_paint = marker_paint.clone();
        match config.marker_variant.as_str() {
            "ring" => {
                variant_paint.set_style(skia_safe::paint::Style::Stroke);
                variant_paint.set_stroke_width(1.5);
            }
            "halo" => {
                variant_paint.set_color(parse_color(&config.marker_color, marker_opacity * 0.35));
            }
            _ => {}
        }
        Ok(Self {
            config,
            scene,
            geometry,
            remaining: layer.image_snapshot(),
            remaining_bounds,
            remaining_image_bounds,
            bounds,
            content_bounds,
            scale,
            completed_paint,
            marker_paint,
            variant_paint,
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

    fn measure_bounds(
        config: &ValidatedRoutePlot,
        scene: &ValidatedScenePresentation,
        geometry: &RouteGeometry,
        remaining_bounds: Rect,
    ) -> (Rect, ContentBounds) {
        let content = Self::measure_content(config, scene, geometry);
        let mut raster = content;
        let scale = geometry.scale_to(config).map(|value| value * scene.scale);
        let marker_radius = (config.marker_size * scene.scale).max(2.0);
        let radius = match config.marker_variant.as_str() {
            "ring" => marker_radius.max(config.marker_variant_diameter * scene.scale * 0.5 + 0.75),
            "halo" => marker_radius.max(config.marker_variant_diameter * scene.scale * 0.5),
            _ => marker_radius,
        };
        // Allocate marker overflow for the entire path once; playback cannot resize the raster.
        raster.join(scaled_rect(geometry.point_bounds, scale).with_outset((radius, radius)));
        if config.remaining_line_opacity > 0.0 {
            raster.join(remaining_bounds);
        }
        raster.outset((1.0, 1.0));
        let bounds = outward(rotated_rect(raster, config.rotation, config.x, config.y));
        let content_bounds =
            ContentBounds::rotated_frame(content, config.rotation, config.x, config.y);
        (bounds, content_bounds)
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        if self.config.x == x && self.config.y == y {
            return;
        }
        self.config.x = x;
        self.config.y = y;
        (self.bounds, self.content_bounds) = Self::measure_bounds(
            &self.config,
            &self.scene,
            &self.geometry,
            self.remaining_bounds,
        );
    }

    pub fn frame(&self, progress: f32) -> RouteFrame {
        RouteFrame {
            state: self.geometry.state(progress),
            bounds: self.bounds,
            content_bounds: self.content_bounds,
        }
    }

    pub fn draw(&self, canvas: &Canvas, frame: &RouteFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        canvas.draw_image(
            &self.remaining,
            (
                self.remaining_image_bounds.left,
                self.remaining_image_bounds.top,
            ),
            None,
        );
        canvas.rotate(self.config.rotation, None);
        let scale = self.scale;
        let completed: Path = self.geometry.completed_path(&frame.state);
        canvas.save();
        canvas.scale((scale[0], scale[1]));
        canvas.draw_path(&completed, &self.completed_paint);
        canvas.restore();
        let point = (
            frame.state.marker_x * scale[0],
            frame.state.marker_y * scale[1],
        );
        let variant_radius = self.config.marker_variant_diameter * self.scene.scale * 0.5;
        if self.config.marker_variant != "single" {
            canvas.draw_circle(point, variant_radius, &self.variant_paint);
        }
        canvas.draw_circle(
            point,
            (self.config.marker_size * self.scene.scale).max(2.0),
            &self.marker_paint,
        );
        canvas.restore();
    }
}
