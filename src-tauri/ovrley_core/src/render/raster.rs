//! Prepares oriented raster images once per render job and draws their scene geometry.

use crate::error::{CoreError, CoreResult};
use crate::normalize::{RasterGeometry, RasterSource, ValidatedRaster};
use crate::raster::load_selected_raster;
use image::{imageops, RgbaImage};
use skia_safe::{images, AlphaType, Canvas, ColorType, Data, FilterMode, Image, Paint, Rect};
use std::sync::Arc;

pub struct PreparedRaster {
    pub(super) geometry: RasterGeometry,
    pub(super) content_hash: u64,
    image: Image,
}

pub(super) fn prepare_rasters(
    rasters: &[ValidatedRaster],
    global_scale: f32,
) -> CoreResult<Vec<PreparedRaster>> {
    rasters
        .iter()
        .map(|config| {
            let selected = match &config.source {
                RasterSource::Image(image) => Arc::clone(image),
                RasterSource::File(path) => Arc::new(
                    load_selected_raster(path).map_err(|error| error.for_widget(&config.id))?,
                ),
            };
            let preview = selected.preview_png();
            let image = Image::from_encoded(Data::new_copy(preview)).ok_or_else(|| {
                CoreError::Render(format!(
                    "Raster {} could not be decoded [raster_error:decode:{}]",
                    config.id, config.id
                ))
            })?;
            let image = downsample_raster(image, config, global_scale)?;
            Ok(PreparedRaster {
                geometry: config.geometry.clone(),
                content_hash: selected.content_hash(),
                image,
            })
        })
        .collect()
}

fn downsample_raster(
    image: Image,
    config: &ValidatedRaster,
    global_scale: f32,
) -> CoreResult<Image> {
    // Keep subpixel widgets at one pixel and avoid allocating enlarged source images.
    let width =
        ((config.geometry.width * global_scale).ceil() as u32).clamp(1, image.width() as u32);
    let height =
        ((config.geometry.height * global_scale).ceil() as u32).clamp(1, image.height() as u32);
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
        return Err(CoreError::Render(format!(
            "Raster {} pixels could not be read for resizing",
            config.id
        )));
    }
    // Filter premultiplied colors to prevent transparent pixels from bleeding into edges.
    let resized = imageops::resize(&pixels, width, height, imageops::FilterType::Triangle);
    images::raster_from_data(
        &info.with_dimensions((width as i32, height as i32)),
        Data::new_copy(resized.as_raw()),
        width as usize * 4,
    )
    .ok_or_else(|| {
        CoreError::Render(format!(
            "Raster {} resized image could not be created",
            config.id
        ))
    })
}

pub(super) fn draw_rasters(
    canvas: &Canvas,
    rasters: &[PreparedRaster],
    global_scale: f32,
    global_opacity: f32,
) {
    for raster in rasters {
        let config = &raster.geometry;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_alpha_f(config.opacity * global_opacity);
        canvas.save();
        canvas.translate((config.x, config.y));
        canvas.rotate(config.rotation, None);
        canvas.scale((global_scale, global_scale));
        canvas.draw_image_rect_with_sampling_options(
            &raster.image,
            None,
            Rect::from_xywh(0.0, 0.0, config.width, config.height),
            FilterMode::Linear,
            &paint,
        );
        canvas.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn prepared(
        id: &str,
        color: [u8; 4],
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rotation: f32,
        opacity: f32,
    ) -> PreparedRaster {
        let mut encoded = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba(color)))
            .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
            .unwrap();
        let config = ValidatedRaster {
            id: id.into(),
            geometry: crate::normalize::RasterGeometry {
                x,
                y,
                width,
                height,
                rotation,
                opacity,
            },
            source: RasterSource::Image(Arc::new(
                crate::raster::load_embedded_raster("png", encoded).unwrap(),
            )),
        };
        prepare_rasters(&[config], 1.0).unwrap().pop().unwrap()
    }

    #[test]
    fn stretches_rotates_and_composites_in_collection_order() {
        let mut surface = crate::render::surface::create_surface(12, 12).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let rasters = [
            prepared("red", [255, 0, 0, 255], 2.0, 2.0, 4.0, 2.0, 0.0, 1.0),
            prepared("blue", [0, 0, 255, 255], 5.0, 2.0, 4.0, 2.0, 90.0, 0.5),
        ];
        draw_rasters(surface.canvas(), &rasters, 1.0, 1.0);
        let png = surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
            .unwrap();
        let image = image::load_from_memory_with_format(png.as_bytes(), ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(image.get_pixel(2, 3).0, [255, 0, 0, 255]);
        // Rotating the second frame about its top-left corner places it above/left of its origin.
        let overlap = image.get_pixel(4, 3).0;
        assert!(overlap[0] > 100 && overlap[0] < 150);
        assert!(overlap[2] > 100 && overlap[2] < 150);
        let blue_only = image.get_pixel(4, 5).0;
        assert!(blue_only[0] > 100 && blue_only[0] < 150);
        assert_eq!(blue_only[0], blue_only[1]);
        assert_eq!(blue_only[2], 255);
    }

    #[test]
    fn applies_scene_scale_and_opacity_to_raster_frame() {
        let mut surface = crate::render::surface::create_surface(10, 10).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let rasters = [prepared(
            "red",
            [255, 0, 0, 255],
            2.0,
            2.0,
            2.0,
            1.0,
            0.0,
            0.5,
        )];
        draw_rasters(surface.canvas(), &rasters, 2.0, 0.5);
        let png = surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
            .unwrap();
        let image = image::load_from_memory_with_format(png.as_bytes(), ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        let inside = image.get_pixel(4, 3).0;
        assert_eq!(inside[0], 255);
        assert!((185..=200).contains(&inside[1]));
        assert_eq!(inside[1], inside[2]);
        assert_eq!(image.get_pixel(7, 3).0, [255, 255, 255, 255]);
    }

    #[test]
    fn selected_resource_renders_without_reopening_its_source_path() {
        let raster = prepared("raster-1", [255, 0, 0, 255], 0.0, 0.0, 2.0, 3.0, 0.0, 1.0);
        assert_eq!((raster.image.width(), raster.image.height()), (1, 1));
    }
}
