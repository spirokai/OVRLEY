use super::validation::RasterGeometry;
use crate::error::{WidgetError, WidgetResult};
use image::{imageops, RgbaImage};
use skia_safe::{images, AlphaType, ColorType, Data, Image};

pub fn downsample_raster(
    image: Image,
    config: &RasterGeometry,
    id: &str,
    global_scale: f32,
) -> WidgetResult<Image> {
    // Keep subpixel widgets at one pixel and avoid allocating enlarged source images.
    let width = ((config.width * global_scale).ceil() as u32).clamp(1, image.width() as u32);
    let height = ((config.height * global_scale).ceil() as u32).clamp(1, image.height() as u32);
    if (width, height) == (image.width() as u32, image.height() as u32) {
        return Ok(image);
    }
    let info = image
        .image_info()
        .with_color_type(ColorType::RGBA8888)
        .with_alpha_type(AlphaType::Premul);
    let mut pixels = RgbaImage::new(image.width() as u32, image.height() as u32);
    if !image.read_pixels(
        &info,
        pixels.as_mut(),
        image.width() as usize * 4,
        (0, 0),
        skia_safe::image::CachingHint::Allow,
    ) {
        return Err(WidgetError::Render(format!(
            "Raster {} pixels could not be read for resizing",
            id
        )));
    }
    // Filter premultiplied colors to prevent transparent pixels from bleeding into edges.
    let resized = imageops::resize(&pixels, width, height, imageops::FilterType::Triangle);
    images::raster_from_data(
        &info.with_dimensions((width as i32, height as i32)),
        Data::new_copy(resized.as_raw()),
        width as usize * 4,
    )
    .ok_or_else(|| WidgetError::Render(format!("Raster {} resized image could not be created", id)))
}
