//! Preview pixel density is independent of scene geometry and native export.
use crate::{
    error::WidgetResult,
    model::{scene::ScenePresentationConfig, validation::require_positive_f32},
    render::pixels::read_surface_rgba,
};
use skia_safe::{Canvas, Color, ColorType, FilterMode, Image, Paint, Point, Rect, Surface};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewRasterScale(f32);

impl PreviewRasterScale {
    pub const NATIVE: Self = Self(1.0);

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

    /// Grows scene `bounds` outwards onto this density's pixel grid.
    pub(crate) fn align(self, bounds: Rect) -> Rect {
        let s = self.0;
        Rect::new(
            (bounds.left * s).floor() / s,
            (bounds.top * s).floor() / s,
            (bounds.right * s).ceil() / s,
            (bounds.bottom * s).ceil() / s,
        )
    }

    /// Pixel size of grid-aligned bounds; rounding absorbs float error in the aligned edges.
    fn aligned_dimensions(self, bounds: Rect) -> (u32, u32) {
        (
            (bounds.width() * self.0).round() as u32,
            (bounds.height() * self.0).round() as u32,
        )
    }

    pub(crate) fn apply(self, canvas: &Canvas) {
        canvas.scale((self.0, self.0));
    }

    pub(crate) fn value(self) -> f32 {
        self.0
    }
}

/// A static layer rasterized once at the host's density, on the same pixel grid as the tiles
/// it is stamped into, so drawing it copies pixels 1:1 instead of resampling them.
#[derive(Clone, Debug)]
pub struct StaticRaster {
    image: Image,
    /// Covered rectangle in the layer's local coordinates.
    bounds: Rect,
}

impl StaticRaster {
    /// `draw` paints in local coordinates; `origin` is where local (0, 0) lands in the scene.
    pub(crate) fn new(
        raster_scale: PreviewRasterScale,
        origin: Point,
        bounds: Rect,
        color_type: ColorType,
        draw: impl FnOnce(&Canvas) -> WidgetResult<()>,
    ) -> WidgetResult<Self> {
        let scene = raster_scale.align(bounds.with_offset(origin));
        let (width, height) = raster_scale.aligned_dimensions(scene);
        let mut surface = crate::widgets::value::preview::surface(width, height, color_type)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        raster_scale.apply(canvas);
        canvas.translate((origin.x - scene.left, origin.y - scene.top));
        draw(canvas)?;
        Ok(Self {
            image: surface.image_snapshot(),
            bounds: scene.with_offset(-origin),
        })
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Linear sampling is exact on the grid and stays smooth if the layer has since moved off it.
    pub(crate) fn draw(&self, canvas: &Canvas, paint: Option<&Paint>) {
        let default = Paint::default();
        canvas.draw_image_rect_with_sampling_options(
            &self.image,
            None,
            self.bounds,
            FilterMode::Linear,
            paint.unwrap_or(&default),
        );
    }
}

/// Draws a scene-space frame into a widget's reused tile surface and reads back straight RGBA.
/// The tile grows onto the density's pixel grid; the returned bounds are the tile's.
pub(crate) fn render_tile(
    surface: &mut Option<Surface>,
    pixels: &mut Vec<u8>,
    color_type: ColorType,
    raster_scale: PreviewRasterScale,
    bounds: Rect,
    draw: impl FnOnce(&Canvas) -> WidgetResult<()>,
) -> WidgetResult<(u32, u32, Rect)> {
    let bounds = raster_scale.align(bounds);
    let (width, height) = raster_scale.aligned_dimensions(bounds);
    if surface.as_ref().map(|surface| (surface.width(), surface.height()))
        != Some((width as i32, height as i32))
    {
        *surface = Some(crate::widgets::value::preview::surface(width, height, color_type)?);
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
    Ok((width, height, bounds))
}
