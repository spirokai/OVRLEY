//! Preview pixel density is independent of scene geometry and native export.
use crate::{
    error::WidgetResult,
    model::{scene::ScenePresentationConfig, validation::require_positive_f32},
    render::pixels::read_surface_rgba,
};
use skia_safe::{Canvas, Color, Rect, Surface};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PreviewRasterScale(f32);

impl PreviewRasterScale {
    pub(crate) const NATIVE: Self = Self(1.0);

    /// Hosts without a viewport density retain native raster resolution.
    pub(crate) fn from_scene(scene: &ScenePresentationConfig) -> WidgetResult<Self> {
        match scene.extra.get("preview_raster_scale") {
            None => Ok(Self::NATIVE),
            Some(value) => require_positive_f32(
                value.as_f64().map(|value| value as f32),
                "scene.preview_raster_scale",
            )
            .map(Self),
        }
    }

    pub(crate) fn dimensions(self, bounds: Rect) -> (u32, u32) {
        (
            (bounds.width() * self.0).ceil() as u32,
            (bounds.height() * self.0).ceil() as u32,
        )
    }

    pub(crate) fn apply(self, canvas: &Canvas) {
        canvas.scale((self.0, self.0));
    }

    pub(crate) fn value(self) -> f32 {
        self.0
    }
}

/// Draws a scene-space frame into a widget's reused tile surface and reads back straight RGBA.
pub(crate) fn render_tile(
    surface: &mut Option<Surface>,
    pixels: &mut Vec<u8>,
    raster_scale: PreviewRasterScale,
    bounds: Rect,
    draw: impl FnOnce(&Canvas) -> WidgetResult<()>,
) -> WidgetResult<(u32, u32)> {
    let (width, height) = raster_scale.dimensions(bounds);
    if surface.as_ref().map(|surface| (surface.width(), surface.height()))
        != Some((width as i32, height as i32))
    {
        *surface = Some(crate::widgets::value::preview::surface(width, height)?);
        pixels.resize(width as usize * height as usize * 4, 0);
    }
    let surface = surface.as_mut().expect("tile surface allocated");
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.save();
    raster_scale.apply(canvas);
    canvas.translate((-bounds.left, -bounds.top));
    let result = draw(canvas);
    canvas.restore();
    result?;
    read_surface_rgba(surface, pixels)?;
    Ok((width, height))
}
