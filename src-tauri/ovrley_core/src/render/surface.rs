use skia_safe::{surfaces, Borrows, EncodedImageFormat, ImageInfo, Surface};
use std::path::Path;

pub fn create_surface(width: u32, height: u32) -> Result<Surface, String> {
    surfaces::raster_n32_premul((width as i32, height as i32))
        .ok_or_else(|| format!("Failed to create raster surface {width}x{height}"))
}

pub fn native_n32_image_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(
        (width as i32, height as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    )
}

pub fn wrap_native_surface<'pixels>(
    width: u32,
    height: u32,
    pixels: &'pixels mut [u8],
) -> Result<Borrows<'pixels, Surface>, String> {
    let info = native_n32_image_info(width, height);
    let row_bytes = (width as usize) * 4;
    surfaces::wrap_pixels(&info, pixels, row_bytes, None)
        .ok_or_else(|| format!("Failed to wrap native n32 surface {width}x{height}"))
}

pub fn write_surface_png(surface: &mut Surface, path: &Path) -> Result<(), String> {
    let image = surface.image_snapshot();
    let data = image
        .encode(None, EncodedImageFormat::PNG, 100)
        .ok_or_else(|| "Failed to encode preview PNG".to_string())?;
    std::fs::write(path, data.as_bytes())
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}
