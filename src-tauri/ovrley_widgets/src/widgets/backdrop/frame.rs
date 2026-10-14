//! Backdrop painting and interaction share the same scaled frame.
use super::{layout::draw_backdrop, validation::ValidatedBackdrop};
use crate::model::types::BackdropType;
use skia_safe::{Canvas, Rect};

pub struct BackdropFrame<'a> {
    pub bounds: Rect,
    pub content_bounds: Rect,
    backdrop: &'a ValidatedBackdrop,
    scale: f32,
}

impl BackdropFrame<'_> {
    pub fn draw(&self, canvas: &Canvas, scene_opacity: f32) {
        draw_backdrop(
            canvas, self.backdrop, self.scale, self.backdrop.opacity * scene_opacity,
        );
    }
}

pub fn prepare_backdrop_frame(backdrop: &ValidatedBackdrop, scale: f32) -> BackdropFrame<'_> {
    let (width, height) = match backdrop.display_type {
        BackdropType::Circle => (backdrop.diameter, backdrop.diameter),
        BackdropType::Rectangle => (backdrop.width, backdrop.height),
    };
    let content_bounds = Rect::from_xywh(backdrop.x, backdrop.y, width * scale, height * scale);
    // One pixel of raster padding retains antialiasing at fractional edges.
    let bounds = Rect::new(
        content_bounds.left.floor() - 1.0,
        content_bounds.top.floor() - 1.0,
        content_bounds.right.ceil() + 1.0,
        content_bounds.bottom.ceil() + 1.0,
    );
    BackdropFrame { bounds, content_bounds, backdrop, scale }
}
