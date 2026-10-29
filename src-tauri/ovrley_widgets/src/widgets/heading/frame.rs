//! One frame owns the scroll positions, painting and tight interaction bounds.
use super::{
    geometry::heading_offset,
    layout::{HeadingCache, IndicatorShape},
};
use skia_safe::{canvas::SaveLayerRec, Canvas, ClipOp, FilterMode, Paint, Path, PathBuilder, Rect};

pub struct HeadingFrame {
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    first_copy: f32,
    indicators: Vec<(Path, Paint)>,
}

impl HeadingCache {
    pub fn prepare_frame(&self, raw: Option<f64>) -> HeadingFrame {
        // Missing external heading deliberately displays the north-facing tape.
        let heading = raw.unwrap_or(0.0) as f32;
        let offset = heading_offset(heading, self.pixels_per_degree, self.frame.width())
            .rem_euclid(self.tape_width);
        let mut x = self.frame.left - offset;
        let mut content = Rect::default();
        while x < self.frame.right {
            for ink in &self.tape_ink {
                let mut bounds = *ink;
                bounds.offset((x, self.frame.top + self.tape_content_y));
                if bounds.intersect(self.frame) {
                    content.join(bounds);
                }
            }
            x += self.tape_width;
        }
        let indicators: Vec<_> = self
            .indicators
            .iter()
            .map(|(shape, paint)| {
                let path = match shape {
                    IndicatorShape::HighlightBar(rect) => Path::rect(*rect, None),
                    IndicatorShape::Chevron(vertices) => {
                        let mut path = PathBuilder::new();
                        path.move_to(vertices[0]);
                        path.line_to(vertices[1]);
                        path.line_to(vertices[2]);
                        path.close();
                        path.detach()
                    }
                };
                (path, paint.clone())
            })
            .collect();
        for (path, _) in &indicators {
            let mut bounds = path.compute_tight_bounds();
            if bounds.intersect(self.frame) {
                content.join(bounds);
            }
        }
        HeadingFrame {
            bounds: Rect::new(
                self.frame.left.floor(),
                self.frame.top.floor(),
                self.frame.right.ceil(),
                self.frame.bottom.ceil(),
            ),
            // The tape is a fixed viewport; gaps at either edge do not move its horizontal bounds.
            content_bounds: Rect::new(
                self.frame.left,
                content.top,
                self.frame.right,
                content.bottom,
            ),
            content_width: self.frame.width(),
            first_copy: self.frame.left - offset,
            indicators,
        }
    }
}

impl HeadingFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeadingCache) {
        canvas.save();
        canvas.clip_rect(cache.frame, ClipOp::Intersect, false);
        let mut opacity = Paint::default();
        opacity.set_alpha_f(cache.opacity);
        canvas.save_layer(&SaveLayerRec::default().bounds(&cache.frame).paint(&opacity));
        let mut x = self.first_copy;
        while x < cache.frame.right {
            // Adjacent hard clips assign each destination pixel to exactly one copy.
            // Sampling gutters keep north continuous without blending two image edges.
            canvas.save();
            canvas.clip_rect(
                Rect::new(x, cache.frame.top, x + cache.tape_width, cache.frame.bottom),
                ClipOp::Intersect,
                false,
            );
            canvas.draw_image_with_sampling_options(
                &cache.tape_image,
                (x - 1.0, cache.frame.top + cache.tape_content_y),
                FilterMode::Linear,
                None,
            );
            canvas.restore();
            x += cache.tape_width;
        }
        // The opaque centre of the mask is a no-op; only shade the fading edges.
        for bounds in &cache.edge_fade_bounds {
            canvas.draw_rect(bounds, &cache.edge_fade);
        }
        if let Some(shadow) = &cache.indicator_shadow {
            canvas.save();
            canvas.translate((cache.shadow_distance, cache.shadow_distance));
            for (path, _) in &self.indicators {
                canvas.draw_path(path, shadow);
            }
            canvas.restore();
        }
        for (path, paint) in &self.indicators {
            canvas.draw_path(path, paint);
        }
        canvas.restore();
        canvas.restore();
    }
}
