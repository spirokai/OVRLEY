use skia_safe::{surfaces, AlphaType, ColorType, EncodedImageFormat, ImageInfo, Surface};
use std::path::Path;

pub fn create_surface(width: u32, height: u32) -> Result<Surface, String> {
    surfaces::raster_n32_premul((width as i32, height as i32))
        .ok_or_else(|| format!("Failed to create raster surface {width}x{height}"))
}

pub fn write_surface_png(surface: &mut Surface, path: &Path) -> Result<(), String> {
    let image = surface.image_snapshot();
    let data = image
        .encode(None, EncodedImageFormat::PNG, 100)
        .ok_or_else(|| "Failed to encode preview PNG".to_string())?;
    std::fs::write(path, data.as_bytes())
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

pub fn read_surface_rgba(surface: &mut Surface, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = (width as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * (height as usize)];
    if surface.read_pixels(&info, pixels.as_mut_slice(), row_bytes, (0, 0)) {
        Ok(pixels)
    } else {
        Err("Failed to read RGBA pixels from render surface".to_string())
    }
}
