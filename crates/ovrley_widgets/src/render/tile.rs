//! Widget raster tiles: the editor's pixel density, surface allocation, premultiplied readback
//! and the scene geometry a rendered tile presents.
use crate::model::validation::require_positive_f32;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use skia_safe::{
    surfaces, AlphaType, Canvas, Color, ColorType, FilterMode, Image, ImageInfo, Paint, Point,
    Rect, Surface,
};
use std::cell::Cell;

/// Text-dominated widgets draw about 3x faster into BGRA, path and image widgets into RGBA.
pub const TEXT_COLOR_TYPE: ColorType = ColorType::BGRA8888;
pub const SHAPE_COLOR_TYPE: ColorType = ColorType::RGBA8888;

/// Largest widget surface edge, in pixels.
const MAX_SURFACE_EDGE: u32 = 16_384;
/// Largest widget surface area, in pixels.
const MAX_SURFACE_PIXELS: u64 = 16_777_216;

thread_local! {
    static ALLOCATED_SURFACES: Cell<u64> = const { Cell::new(0) };
}

/// Widget surfaces allocated on this thread. Layout-only work must leave it unchanged.
pub fn allocated_surfaces() -> u64 {
    ALLOCATED_SURFACES.with(Cell::get)
}

/// A raster surface for one widget; readback converts either colour type to straight RGBA.
pub(crate) fn surface(width: u32, height: u32, color_type: ColorType) -> WidgetResult<Surface> {
    if width == 0
        || height == 0
        || width > MAX_SURFACE_EDGE
        || height > MAX_SURFACE_EDGE
        || u64::from(width) * u64::from(height) > MAX_SURFACE_PIXELS
    {
        return Err(WidgetError::Render(format!(
            "widget surface too large: {width}x{height}"
        )));
    }
    let info = ImageInfo::new(
        (width as i32, height as i32),
        color_type,
        AlphaType::Premul,
        None,
    );
    ALLOCATED_SURFACES.with(|count| count.set(count.get() + 1));
    surfaces::raster(&info, None, None)
        .ok_or_else(|| WidgetError::Render("widget surface allocation failed".into()))
}

/// The editor's display density, set on the preview session; it is not widget or scene configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewRasterScale(f32);

/// Hosts without a viewport density retain native raster resolution.
impl Default for PreviewRasterScale {
    fn default() -> Self {
        Self::NATIVE
    }
}

impl PreviewRasterScale {
    pub const NATIVE: Self = Self(1.0);

    pub fn new(scale: f32) -> WidgetResult<Self> {
        require_positive_f32(Some(scale), "preview raster scale").map(Self)
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
        let mut surface = surface(width, height, color_type)?;
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

/// Scene-pixel rectangle covered by a widget's rendered tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl TileBounds {
    /// The same tile moved by a placement edit, which does not re-render it.
    pub fn translated(self, dx: f64, dy: f64) -> Self {
        Self {
            left: (f64::from(self.left) + dx) as f32,
            top: (f64::from(self.top) + dy) as f32,
            ..self
        }
    }
}

impl From<Rect> for TileBounds {
    fn from(rect: Rect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width(),
            height: rect.height(),
        }
    }
}

impl From<ContentBounds> for TileBounds {
    fn from(content: ContentBounds) -> Self {
        Self {
            left: content.left,
            top: content.top,
            width: content.width,
            height: content.height,
        }
    }
}

/// Interaction rectangle in scene pixels, independent of raster allocation.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct ContentBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl ContentBounds {
    /// Center-pivoted editor frame for a local rectangle drawn at the saved origin.
    pub fn rotated_frame(rect: Rect, rotation: f32, x: f32, y: f32) -> Self {
        let center = (
            rect.left + rect.width() * 0.5,
            rect.top + rect.height() * 0.5,
        );
        let (sin, cos) = rotation.to_radians().sin_cos();
        Self {
            left: x + center.0 * cos - center.1 * sin - rect.width() * 0.5,
            top: y + center.0 * sin + center.1 * cos - rect.height() * 0.5,
            width: rect.width(),
            height: rect.height(),
        }
    }

    pub fn rotation_origin(&self) -> [f32; 2] {
        [self.width * 0.5, self.height * 0.5]
    }

    fn translated(self, dx: f64, dy: f64) -> Self {
        Self {
            left: (f64::from(self.left) + dx) as f32,
            top: (f64::from(self.top) + dy) as f32,
            ..self
        }
    }
}

impl From<Rect> for ContentBounds {
    fn from(rect: Rect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width(),
            height: rect.height(),
        }
    }
}

/// What a widget presents inside its tile: its interaction rectangle, the width its content
/// aligns by, and the rotation a framed widget applies about the rectangle's center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Presented {
    pub content: ContentBounds,
    pub content_width: f32,
    pub rotation: Option<f32>,
}

impl Presented {
    pub fn new(content: impl Into<ContentBounds>, content_width: f32) -> Self {
        Self {
            content: content.into(),
            content_width,
            rotation: None,
        }
    }

    /// Content whose alignment width is its own width.
    pub fn content(content: impl Into<ContentBounds>) -> Self {
        let content = content.into();
        Self::new(content, content.width)
    }

    pub fn rotated(self, rotation: f32) -> Self {
        Self {
            rotation: Some(rotation),
            ..self
        }
    }
}

/// A rendered tile's placement in scene pixels, its pixel size and what it presents.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct WidgetGeometry {
    pub left: f32,
    pub top: f32,
    pub width: u32,
    pub height: u32,
    pub preview_raster_scale: f32,
    pub content_width: f32,
    pub content: ContentBounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
}

impl WidgetGeometry {
    /// The pivot of a rotated widget, relative to its content's top-left corner.
    pub fn rotation_origin(&self) -> Option<[f32; 2]> {
        self.rotation.map(|_| self.content.rotation_origin())
    }

    /// The same tile moved by a placement edit, which does not re-render it.
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self {
            left: (f64::from(self.left) + dx) as f32,
            top: (f64::from(self.top) + dy) as f32,
            content: self.content.translated(dx, dy),
            ..self.clone()
        }
    }
}

/// The raster a preview widget last drew and where it sits in the scene. The revision changes
/// with every redraw.
#[derive(Default)]
pub struct Tile {
    surface: Option<Surface>,
    /// Premultiplied RGBA, as the host composites it.
    pub pixels: Vec<u8>,
    /// Absent until the first render.
    pub geometry: Option<WidgetGeometry>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl Tile {
    /// Draws a scene-space frame covering `bounds` into the reused surface and reads back
    /// premultiplied RGBA. The tile grows onto the density's pixel grid.
    pub(crate) fn render(
        &mut self,
        raster_scale: PreviewRasterScale,
        color_type: ColorType,
        bounds: Rect,
        presented: Presented,
        draw: impl FnOnce(&Canvas) -> WidgetResult<()>,
    ) -> WidgetResult<()> {
        let bounds = raster_scale.align(bounds);
        let (width, height) = raster_scale.aligned_dimensions(bounds);
        if self
            .surface
            .as_ref()
            .map(|surface| (surface.width(), surface.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height, color_type)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("tile surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        raster_scale.apply(canvas);
        canvas.translate((-bounds.left, -bounds.top));
        let result = draw(canvas);
        canvas.restore();
        result?;
        read_premultiplied_rgba(surface, &mut self.pixels)?;
        self.geometry = Some(WidgetGeometry {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            preview_raster_scale: raster_scale.value(),
            content_width: presented.content_width,
            content: presented.content,
            rotation: presented.rotation,
        });
        self.bounds = Some(bounds.into());
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

/// Reads the surface as premultiplied RGBA, the form the host composites: a BGRA surface is
/// swizzled, an RGBA one copied.
pub fn read_premultiplied_rgba(surface: &mut Surface, pixels: &mut [u8]) -> WidgetResult<()> {
    let info = ImageInfo::new(
        (surface.width(), surface.height()),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    if !surface.read_pixels(&info, pixels, surface.width() as usize * 4, (0, 0)) {
        return Err(WidgetError::Render(
            "unable to read widget RGBA pixels".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::read_premultiplied_rgba;
    use skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};

    #[test]
    fn readback_returns_the_premultiplied_pixels_of_either_colour_type() {
        for color_type in [ColorType::RGBA8888, ColorType::BGRA8888] {
            let info = ImageInfo::new((2, 1), color_type, AlphaType::Premul, None);
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            surface.canvas().clear(Color::from_argb(128, 255, 0, 0));
            let mut pixels = vec![0; 8];
            read_premultiplied_rgba(&mut surface, &mut pixels).unwrap();
            assert_eq!(pixels, [128, 0, 0, 128, 128, 0, 0, 128], "{color_type:?}");
        }
    }

    #[test]
    fn reused_output_replaces_previous_colors_and_alpha() {
        let info = ImageInfo::new((2, 1), ColorType::RGBA8888, AlphaType::Premul, None);
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let mut pixels = vec![0; 8];
        surface.canvas().clear(Color::from_argb(255, 0, 255, 0));
        read_premultiplied_rgba(&mut surface, &mut pixels).unwrap();
        assert_eq!(pixels, [0, 255, 0, 255, 0, 255, 0, 255]);
        surface.canvas().clear(Color::TRANSPARENT);
        read_premultiplied_rgba(&mut surface, &mut pixels).unwrap();
        assert_eq!(pixels, [0; 8]);
    }
}
