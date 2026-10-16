use super::{config::BackdropConfig, frame::prepare_backdrop_frame, validation::validate_backdrop};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    render::pixels::read_surface_rgba,
    widgets::value::preview::{surface, WidgetGeometry},
};
use skia_safe::Color;

/// Backdrops produce pixels at creation/update only, without fonts or activity.
pub struct PreviewBackdrop {
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewBackdrop {
    pub fn new(widget: BackdropConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let backdrop = validate_backdrop(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        let frame = prepare_backdrop_frame(&backdrop, scene.scale);
        let bounds = frame.bounds;
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        let mut surface = surface(width, height)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, scene.opacity);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        read_surface_rgba(&mut surface, &mut pixels)?;
        let geometry = serde_json::to_vec(&WidgetGeometry {
            left: bounds.left, top: bounds.top, width, height,
            content_width: frame.content_bounds.width(), content: frame.content_bounds.into(),
        }).map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self { pixels, geometry, revision: 1 })
    }
}
