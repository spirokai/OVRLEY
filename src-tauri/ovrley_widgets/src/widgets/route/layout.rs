//! Cached route shadow and vector drawing; one frame supplies paint and interaction bounds.
use super::{
    geometry::{ProgressLine, RouteGeometry},
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
        config.path.marker_variant,
        config.path.marker_size,
        config.path.marker_variant_diameter,
        &config.path.marker_color,
        config.path.marker_opacity * scene.opacity,
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
    /// The line's shadow at the final angle, relative to the widget origin. Blurring is the costly
    /// part, so it is drawn once. None when the scene casts no shadow.
    shadow: Option<StaticRaster>,
    bounds: Rect,
    content_bounds: ContentBounds,
    scale: [f32; 2],
    stroke: SplitStroke,
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
    pub progress: f32,
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

/// How every route draws its line each frame: whole in `remaining`, then the stretch behind the split
/// in `completed` on top. The whole line never changes, so its path is built once and a frame builds
/// only the completed stretch. Segments wholly outside `within` are left out of both, so a line that
/// mostly runs off its frame strokes only what shows.
#[derive(Clone, Debug)]
pub(crate) struct SplitStroke {
    /// The whole line, which the remaining side and the route plot's shadow stroke.
    pub(crate) whole: Path,
    /// Reached out by half a stroke and a unit of antialiasing.
    within: Rect,
    remaining: Paint,
    completed: Paint,
}

impl SplitStroke {
    /// `within` and both paints' widths are in `line`'s units.
    pub(crate) fn new(
        line: &ProgressLine,
        within: Rect,
        remaining: Paint,
        completed: Paint,
    ) -> Self {
        // A segment just outside still reaches in by half its stroke, and antialiasing by a unit.
        let reach = remaining.stroke_width().max(completed.stroke_width()) / 2.0 + 1.0;
        let within = within.with_outset((reach, reach));
        Self {
            whole: line.path(within),
            within,
            remaining,
            completed,
        }
    }

    /// Draws `line`, the one this was built for, split at `progress`, with its units scaled by
    /// `scale`. Returns where the split lands in the canvas's units, with the line's heading there in
    /// degrees clockwise from up.
    pub(crate) fn draw(
        &self,
        canvas: &Canvas,
        line: &ProgressLine,
        progress: f32,
        scale: [f32; 2],
    ) -> (Point, f32) {
        let split = line.split(progress);
        canvas.save();
        canvas.scale((scale[0], scale[1]));
        canvas.draw_path(&self.whole, &self.remaining);
        canvas.draw_path(&line.completed_path(&split, self.within), &self.completed);
        canvas.restore();
        let ((x, y), (dx, dy)) = (split.point, split.direction);
        (
            Point::new(x * scale[0], y * scale[1]),
            (dx * scale[0]).atan2(-dy * scale[1]).to_degrees(),
        )
    }
}

impl RouteLayout {
    /// Remaining-path stroke and the unrotated rectangle it and its shadow cover, before any drawing.
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

    /// Frame bounds from layout alone; the cached shadow is not drawn.
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
        let opacity = config.path.remaining_line_opacity * scene.opacity;
        // Every segment lies inside the line's own bounds, so none is left out.
        let stroke = SplitStroke::new(
            &geometry.line,
            geometry.point_bounds,
            stroke_paint(&config.path.remaining_line_color, width / scale[0], opacity),
            stroke_paint(
                &config.path.completed_line_color,
                (config.completed_line_width * scene.scale).max(1.0) / scale[0],
                config.path.completed_line_opacity * scene.opacity,
            ),
        );
        // Cache at the final angle so playback can blit without resampling a rotated
        // bitmap every frame. Positions stay relative to the widget for cheap dragging.
        let shadow = (blur > 0.0)
            .then(|| {
                StaticRaster::new(
                    raster_scale,
                    Point::new(config.x, config.y),
                    outward(rotated_rect(remaining_bounds, config.rotation, 0.0, 0.0)),
                    SHAPE_COLOR_TYPE,
                    |canvas| {
                        canvas.rotate(config.rotation, None);
                        canvas.translate((offset[0], offset[1]));
                        canvas.scale((scale[0], scale[1]));
                        let mut paint =
                            stroke_paint(&scene.shadow_color, width / scale[0], opacity);
                        paint.set_mask_filter(MaskFilter::blur(
                            BlurStyle::Normal,
                            blur / scale[0],
                            true,
                        ));
                        canvas.draw_path(&stroke.whole, &paint);
                        Ok(())
                    },
                )
            })
            .transpose()?;
        let (bounds, content_bounds) =
            Self::measure_bounds(&config, &scene, &geometry, remaining_bounds);
        let marker = route_marker(&config, &scene);
        Ok(Self {
            config,
            scene,
            geometry,
            shadow,
            bounds,
            content_bounds,
            scale,
            stroke,
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
        if config.path.remaining_line_opacity > 0.0 {
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
            progress,
            bounds: self.bounds,
            content_bounds: self.content_bounds,
        }
    }

    pub fn draw(&self, canvas: &Canvas, frame: &RouteFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        if let Some(shadow) = &self.shadow {
            shadow.draw(canvas, None);
        }
        canvas.rotate(self.config.rotation, None);
        let (point, heading) =
            self.stroke
                .draw(canvas, &self.geometry.line, frame.progress, self.scale);
        self.marker.draw_heading(canvas, point, heading);
        canvas.restore();
    }
}
