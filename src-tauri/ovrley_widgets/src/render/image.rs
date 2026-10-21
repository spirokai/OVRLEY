//! Hosts acquire raster resources; widget construction only borrows decoded pixels.
use crate::error::{WidgetError, WidgetResult};
use skia_safe::{AlphaType, ColorType, Data, Image};
use std::sync::Arc;

/// Oriented source pixels in straight-alpha RGBA, shared without copying.
#[derive(Clone)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}

impl RasterImage {
    /// Decodes encoded image bytes directly to straight-alpha RGBA.
    pub fn decode(bytes: &[u8]) -> WidgetResult<Self> {
        let image = Image::from_encoded(Data::new_copy(bytes))
            .ok_or_else(|| WidgetError::Render("unable to decode raster resource".into()))?;
        let (width, height) = (image.width() as u32, image.height() as u32);
        // Decode source colors directly; a premultiplied round trip loses low-alpha precision.
        let info = image
            .image_info()
            .with_color_type(ColorType::RGBA8888)
            .with_alpha_type(AlphaType::Unpremul);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        if !image.read_pixels(
            &info,
            &mut pixels,
            width as usize * 4,
            (0, 0),
            skia_safe::image::CachingHint::Disallow,
        ) {
            return Err(WidgetError::Render(
                "unable to read raster RGBA pixels".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            pixels: Arc::new(pixels),
        })
    }
}

pub trait ImageSource {
    fn resolve_image(&self, resource_id: &str) -> WidgetResult<RasterImage>;
}
