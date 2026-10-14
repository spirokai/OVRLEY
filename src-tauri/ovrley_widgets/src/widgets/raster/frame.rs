use super::validation::RasterGeometry;
use crate::widgets::value::preview::ContentBounds;
use skia_safe::{Canvas, Color, FilterMode, Image, Paint, Rect};

/// One oriented frame for export pixels and editor interaction geometry.
pub struct RasterFrame {
    pub content: Rect,
    pub bounds: Rect,
    pub rotation: f32,
    opacity: f32,
}

impl RasterFrame {
    pub fn new(config: &RasterGeometry, scale: f32) -> Self {
        let content = Rect::from_xywh(
            config.x,
            config.y,
            config.width * scale,
            config.height * scale,
        );
        let (sin, cos) = config.rotation.to_radians().sin_cos();
        let points = [
            (0.0, 0.0),
            (content.width(), 0.0),
            (0.0, content.height()),
            (content.width(), content.height()),
        ]
        .map(|(x, y)| (config.x + x * cos - y * sin, config.y + x * sin + y * cos));
        let bounds = Rect::new(
            points
                .iter()
                .map(|p| p.0)
                .fold(f32::INFINITY, f32::min)
                .floor(),
            points
                .iter()
                .map(|p| p.1)
                .fold(f32::INFINITY, f32::min)
                .floor(),
            points
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil(),
            points
                .iter()
                .map(|p| p.1)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil(),
        );
        Self {
            content,
            bounds,
            rotation: config.rotation,
            opacity: config.opacity,
        }
    }

    pub fn content_bounds(&self) -> ContentBounds {
        ContentBounds::rotated_frame(
            Rect::from_wh(self.content.width(), self.content.height()),
            self.rotation,
            self.content.left,
            self.content.top,
        )
    }

    pub fn paint_opacity(&self, has_image: bool, scene_opacity: f32) -> f32 {
        self.opacity * scene_opacity * if has_image { 1.0 } else { 0.5 }
    }

    pub fn draw(&self, canvas: &Canvas, image: Option<&Image>, scene_opacity: f32) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::WHITE);
        paint.set_alpha_f(self.paint_opacity(image.is_some(), scene_opacity));
        canvas.save();
        canvas.translate((self.content.left, self.content.top));
        canvas.rotate(self.rotation, None);
        let rect = Rect::from_wh(self.content.width(), self.content.height());
        if let Some(image) = image {
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                rect,
                FilterMode::Linear,
                &paint,
            );
        } else {
            // Empty selections and failed persisted assets have an editable placeholder.
            canvas.draw_rect(rect, &paint);
        }
        canvas.restore();
    }
}
