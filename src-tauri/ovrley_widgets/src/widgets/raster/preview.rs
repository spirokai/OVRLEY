use super::{config::RasterConfig, frame::RasterFrame, validation::validate_raster_geometry};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    widgets::value::preview::WidgetGeometry,
};
use skia_safe::{AlphaType, ColorType, Data, Image};
use std::{collections::BTreeMap, sync::Arc};

struct PreviewImage {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

#[derive(Default)]
pub struct PreviewImages(BTreeMap<String, PreviewImage>);

impl PreviewImages {
    /// Oriented PNG bytes from the native immutable resource, decoded once per session.
    pub fn register(&mut self, id: String, bytes: &[u8]) -> WidgetResult<()> {
        if id.is_empty() {
            return Err(WidgetError::Config("empty raster resource identity".into()));
        }
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
        self.0.insert(
            id,
            PreviewImage {
                width,
                height,
                pixels: Arc::new(pixels),
            },
        );
        Ok(())
    }
}

pub struct PreviewRaster {
    pub pixels: Arc<Vec<u8>>,
    pub geometry: Vec<u8>,
    pub revision: u32,
    source_id: Option<String>,
    pixels_revision: u32,
}

impl PreviewRaster {
    pub fn new(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &PreviewImages,
    ) -> WidgetResult<Self> {
        Self::prepare(widget, scene, images, None)
    }

    pub fn update(
        &mut self,
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &PreviewImages,
    ) -> WidgetResult<()> {
        *self = Self::prepare(widget, scene, images, Some(self))?;
        Ok(())
    }

    fn prepare(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &PreviewImages,
        previous: Option<&Self>,
    ) -> WidgetResult<Self> {
        let config = validate_raster_geometry(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        if widget.resource_id.is_some() && widget.resource_error_code.is_some() {
            return Err(WidgetError::Config(
                "Raster has conflicting resource state".into(),
            ));
        }
        // Keep source pixels immutable; presentation edits only update the frame.
        let (source_id, width, height, pixels) =
            if widget.path.is_none() || widget.resource_error_code.is_some() {
                (None, 1, 1, Arc::new(vec![255; 4]))
            } else {
                let id = widget.resource_id.as_ref().ok_or_else(|| {
                    WidgetError::Config("Raster has no loaded image resource".into())
                })?;
                let image = images.0.get(id).ok_or_else(|| {
                    WidgetError::Render(format!("raster resource not registered: {id}"))
                })?;
                (
                    Some(id.clone()),
                    image.width,
                    image.height,
                    Arc::clone(&image.pixels),
                )
            };
        let frame = RasterFrame::new(&config, scene.scale);
        let content = frame.content_bounds();
        let pixels_revision = previous.map_or(1, |previous| {
            if previous.source_id == source_id {
                previous.pixels_revision
            } else {
                previous.pixels_revision.wrapping_add(1)
            }
        });
        #[derive(serde::Serialize)]
        struct Geometry {
            #[serde(flatten)]
            widget: WidgetGeometry,
            rotation: f32,
            rotation_origin: [f32; 2],
            opacity: f32,
            pixels_revision: u32,
        }
        let geometry = serde_json::to_vec(&Geometry {
            widget: WidgetGeometry {
                left: content.left,
                top: content.top,
                width,
                height,
                content_width: content.width,
                content,
            },
            rotation: frame.rotation,
            rotation_origin: content.rotation_origin(),
            opacity: frame.paint_opacity(source_id.is_some(), scene.opacity),
            pixels_revision,
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self {
            pixels,
            geometry,
            revision: previous.map_or(1, |previous| previous.revision.wrapping_add(1)),
            source_id,
            pixels_revision,
        })
    }
}
