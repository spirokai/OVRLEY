//! One frame owns the scroll positions, painting and tight interaction bounds.
use super::{
    geometry::heading_offset,
    layout::{HeadingCache, IndicatorShape},
};
use skia_safe::{canvas::SaveLayerRec, Canvas, ClipOp, Paint, Path, PathBuilder, Rect};

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
        // One repeated texture lets linear sampling cross north without compositing two edges.
        canvas.save();
        canvas.translate((self.first_copy, cache.frame.top + cache.tape_content_y));
        canvas.draw_rect(
            Rect::from_xywh(
                cache.frame.left - self.first_copy,
                0.0,
                cache.frame.width(),
                cache.tape_image.height() as f32,
            ),
            &cache.tape_paint,
        );
        canvas.restore();
        // Mask only the scrolling strip inside its isolated layer, then draw the indicator.
        canvas.draw_rect(cache.frame, &cache.edge_fade);
        if let Some(shadow) = &cache.indicator_shadow {
            for (path, _) in &self.indicators {
                canvas.draw_path(path, shadow);
            }
        }
        for (path, paint) in &self.indicators {
            canvas.draw_path(path, paint);
        }
        canvas.restore();
        canvas.restore();
    }
}
