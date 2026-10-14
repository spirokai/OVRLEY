//! Straight-alpha preview transport without Skia's floating-point conversion path.
use crate::error::{WidgetError, WidgetResult};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

pub fn read_surface_rgba(surface: &mut Surface, pixels: &mut [u8]) -> WidgetResult<()> {
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
    // ImageData needs straight alpha; transparent and opaque pixels already match.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha) as u8;
        }
    }
    Ok(())
}
