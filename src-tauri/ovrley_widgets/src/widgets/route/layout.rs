//! Cached remaining route and vector drawing; one frame supplies paint and interaction bounds.
use super::{
    geometry::{RouteFrameState, RouteGeometry},
    validation::ValidatedRoutePlot,
};
use crate::{
    error::WidgetResult,
    model::scene::ValidatedScenePresentation,
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        text::parse_color,
    },
    widgets::{
        plot_marker::PlotMarker,
        value::preview::{ContentBounds, SHAPE_COLOR_TYPE},
    },
};

fn route_marker(config: &ValidatedRoutePlot, scene: &ValidatedScenePresentation) -> PlotMarker {
    PlotMarker::new(
        config.marker_variant,
        config.marker_size,
        config.marker_variant_diameter,
        &config.marker_color,
        config.marker_opacity * scene.opacity,
        scene.scale,
    )
}
use skia_safe::{BlurStyle, Canvas, MaskFilter, Paint, PaintCap, PaintJoin, Path, Point, Rect};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RouteLayout {
    pub config: ValidatedRoutePlot,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<RouteGeometry>,
    /// Remaining line at the final angle, relative to the widget origin.
    remaining: StaticRaster,
    bounds: Rect,
    content_bounds: ContentBounds,
    scale: [f32; 2],
    completed_paint: Paint,
    marker: PlotMarker,
}

struct RemainingStroke {
    scale: [f32; 2],
    width: f32,
    offset: [f32; 2],
    blur: f32,
    bounds: Rect,
}

pub struct RouteFrame {
    pub state: RouteFrameState,
    pub bounds: Rect,
    /// Unrotated selection rectangle in scene pixels; rotation uses its center.
    pub content_bounds: ContentBounds,
}

pub(crate) fn scaled_rect(rect: Rect, scale: [f32; 2]) -> Rect {
    Rect::new(
        rect.left * scale[0],
        rect.top * scale[1],
        rect.right * scale[0],
        rect.bottom * scale[1],
    )
}

pub(crate) fn rotated_rect(rect: Rect, rotation: f32, x: f32, y: f32) -> Rect {
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

pub(crate) fn outward(rect: Rect) -> Rect {
    Rect::new(
        rect.left.floor(),
        rect.top.floor(),
        rect.right.ceil().max(rect.left.floor() + 1.0),
        rect.bottom.ceil().max(rect.top.floor() + 1.0),
    )
}

pub(crate) fn stroke_paint(color: &str, width: f32, opacity: f32) -> Paint {
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
    /// Remaining-path stroke and its unrotated raster rectangle, before any drawing.
    fn remaining_stroke(
        config: &ValidatedRoutePlot,
        scene: &ValidatedScenePresentation,
        geometry: &RouteGeometry,
    ) -> RemainingStroke {
        let scale = geometry.scale_to(config).map(|value| value * scene.scale);
        let width = (config.remaining_line_width * scene.scale).max(1.0);
        let line_bounds =
            scaled_rect(geometry.point_bounds, scale).with_outset((width * 0.5, width * 0.5));
        let mut bounds = line_bounds;
        let (sin, cos) = config.rotation.to_radians().sin_cos();
        let distance = scene.shadow_distance * scene.scale;
        let offset = [(cos + sin) * distance, (cos - sin) * distance];
        let blur = scene.shadow_strength * scene.scale;
        if blur > 0.0 {
            bounds.join(
                line_bounds
                    .with_outset((blur * 3.0, blur * 3.0))
                    .with_offset((offset[0], offset[1])),
            );
        }
        RemainingStroke {
            scale,
            width,
            offset,
            blur,
            bounds: outward(bounds.with_outset((1.0, 1.0))),
        }
    }

    /// Frame bounds from layout alone; the cached remaining raster is not drawn.
    pub fn measure(
        config: &ValidatedRoutePlot,
        scene: &ValidatedScenePresentation,
        geometry: &RouteGeometry,
    ) -> Rect {
        let remaining = Self::remaining_stroke(config, scene, geometry);
        Self::measure_bounds(config, scene, geometry, remaining.bounds).0
    }

    pub fn new(
        config: ValidatedRoutePlot,
        scene: ValidatedScenePresentation,
        geometry: Arc<RouteGeometry>,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let RemainingStroke {
            scale,
            width,
            offset,
            blur,
            bounds: remaining_bounds,
        } = Self::remaining_stroke(&config, &scene, &geometry);
        // Cache at the final angle so playback can blit without resampling a rotated
        // bitmap every frame. Positions stay relative to the widget for cheap dragging.
        let remaining_image_bounds =
            outward(rotated_rect(remaining_bounds, config.rotation, 0.0, 0.0));
        let remaining = StaticRaster::new(
            raster_scale,
            Point::new(config.x, config.y),
            remaining_image_bounds,
            SHAPE_COLOR_TYPE,
            |canvas| {
                let path = geometry.path.lock().expect("route path lock");
                canvas.rotate(config.rotation, None);
                let opacity = config.remaining_line_opacity * scene.opacity;
                if blur > 0.0 {
                    canvas.save();
                    canvas.translate((offset[0], offset[1]));
                    canvas.scale((scale[0], scale[1]));
                    let mut paint = stroke_paint(&scene.shadow_color, width / scale[0], opacity);
                    paint.set_mask_filter(MaskFilter::blur(
                        BlurStyle::Normal,
                        blur / scale[0],
                        true,
                    ));
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
                Ok(())
            },
        )?;
        let (bounds, content_bounds) =
            Self::measure_bounds(&config, &scene, &geometry, remaining_bounds);
        let completed_paint = stroke_paint(
            &config.completed_line_color,
            (config.completed_line_width * scene.scale).max(1.0) / scale[0],
            config.completed_line_opacity * scene.opacity,
        );
        let marker = route_marker(&config, &scene);
        Ok(Self {
            config,
            scene,
            geometry,
            remaining,
            bounds,
            content_bounds,
            scale,
            completed_paint,
            marker,
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
        let radius = route_marker(config, scene).extent();
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
        self.remaining.draw(canvas, None);
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
        self.marker.draw(canvas, point);
        canvas.restore();
    }
}
