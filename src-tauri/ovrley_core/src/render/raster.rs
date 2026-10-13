//! Prepares oriented raster images once per render job and draws their scene geometry.

use crate::error::{CoreError, CoreResult};
use crate::normalize::{RasterSource, ValidatedRaster};
use ovrley_widgets::widgets::raster::{validation::RasterGeometry, image::downsample_raster, frame::RasterFrame};
use crate::raster::load_selected_raster;
use skia_safe::{Canvas, Data, Image};
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
            let image = downsample_raster(image, &config.geometry, &config.id, global_scale)?;
            Ok(PreparedRaster {
                geometry: config.geometry.clone(),
                content_hash: selected.content_hash(),
                image,
            })
        })
        .collect()
}

pub(super) fn draw_rasters(canvas: &Canvas, rasters: &[PreparedRaster], global_scale: f32, global_opacity: f32) {
    for raster in rasters {
        RasterFrame::new(&raster.geometry, global_scale).draw(canvas, Some(&raster.image), global_opacity);
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
            geometry: ovrley_widgets::widgets::raster::validation::RasterGeometry {
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
    fn wasm_pixels_match_native_rasters_with_rotation_downsampling_and_opacity() {
        use ovrley_widgets::preview::PreviewSession;
        use serde_json::json;
        use skia_safe::{AlphaType, ColorType, ImageInfo};
        let source = RgbaImage::from_fn(80, 40, |x, y| Rgba([x as u8 * 3, y as u8 * 5, 120, if x < 40 { 120 } else { 255 }]));
        let mut encoded = Vec::new();
        DynamicImage::ImageRgba8(source).write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png).unwrap();
        let selected = Arc::new(crate::raster::load_embedded_raster("png", encoded).unwrap());
        let mut preview = PreviewSession::default();
        preview.images.register("selected".into(), selected.preview_png()).unwrap();
        for rotation in [0.0, 37.0, 90.0, 230.0] {
            let config = ValidatedRaster {
                id: "raster".into(),
                geometry: RasterGeometry { x: 110.25, y: 70.5, width: 21.25, height: 10.625, rotation, opacity: 0.7 },
                source: RasterSource::Image(Arc::clone(&selected)),
            };
            let id = preview.create_widget(serde_json::from_value(json!({
                "type": "raster",
                "widget": { "id":"raster", "x":110.25, "y":70.5, "width":21.25, "height":10.625, "rotation":rotation, "opacity":0.7, "path":"/image.png", "resourceId":"selected" },
                "scene": { "width":1920, "height":1080, "scale":1.7, "opacity":0.6, "shadow_color":"#000000", "shadow_strength":0, "shadow_distance":0, "border_color":"#000000", "border_thickness":0 }
            })).unwrap()).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: serde_json::Value = serde_json::from_slice(widget.geometry()).unwrap();
            let width = geometry["width"].as_i64().unwrap() as i32;
            let height = geometry["height"].as_i64().unwrap() as i32;
            let mut surface = crate::render::surface::create_surface(width as u32, height as u32).unwrap();
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            surface.canvas().translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
            draw_rasters(surface.canvas(), &prepare_rasters(&[config], 1.7).unwrap(), 1.7, 0.6);
            let info = ImageInfo::new((width, height), ColorType::RGBA8888, AlphaType::Unpremul, None);
            let mut pixels = vec![0; widget.pixels().len()];
            assert!(surface.read_pixels(&info, &mut pixels, width as usize * 4, (0, 0)));
            assert_eq!(&pixels, widget.pixels(), "rotation={rotation}");
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
        }
    }

    #[test]
    fn selected_resource_renders_without_reopening_its_source_path() {
        let raster = prepared("raster-1", [255, 0, 0, 255], 0.0, 0.0, 2.0, 3.0, 0.0, 1.0);
        assert_eq!((raster.image.width(), raster.image.height()), (1, 1));
    }
}
