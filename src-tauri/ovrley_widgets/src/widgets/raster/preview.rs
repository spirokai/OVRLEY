use super::{
    config::RasterConfig, frame::RasterFrame, image::downsample_raster,
    validation::validate_raster_geometry,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    widgets::value::preview::{surface, WidgetGeometry},
};
use skia_safe::{AlphaType, Color, ColorType, Data, Image, ImageInfo};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PreviewImages(BTreeMap<String, Image>);

impl PreviewImages {
    /// Oriented PNG bytes from the native immutable resource, decoded once per session.
    pub fn register(&mut self, id: String, bytes: &[u8]) -> WidgetResult<()> {
        if id.is_empty() {
            return Err(WidgetError::Config("empty raster resource identity".into()));
        }
        let image = Image::from_encoded(Data::new_copy(bytes))
            .ok_or_else(|| WidgetError::Render("unable to decode raster resource".into()))?;
        self.0.insert(id, image);
        Ok(())
    }
}

pub struct PreviewRaster {
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewRaster {
    pub fn new(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &PreviewImages,
    ) -> WidgetResult<Self> {
        let config = validate_raster_geometry(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        if widget.resource_id.is_some() && widget.resource_error_code.is_some() {
            return Err(WidgetError::Config(
                "Raster has conflicting resource state".into(),
            ));
        }
        let image = if widget.path.is_none() || widget.resource_error_code.is_some() {
            None
        } else {
            let id = widget
                .resource_id
                .as_ref()
                .ok_or_else(|| WidgetError::Config("Raster has no loaded image resource".into()))?;
            let image = images.0.get(id).ok_or_else(|| {
                WidgetError::Render(format!("raster resource not registered: {id}"))
            })?;
            Some(downsample_raster(
                image.clone(),
                &config,
                &widget.id,
                scene.scale,
            )?)
        };
        let frame = RasterFrame::new(&config, scene.scale);
        let bounds = frame.bounds;
        let (width, height) = (bounds.width() as u32, bounds.height() as u32);
        let mut surface = surface(width, height)?;
        surface.canvas().clear(Color::TRANSPARENT);
        surface.canvas().translate((-bounds.left, -bounds.top));
        frame.draw(surface.canvas(), image.as_ref(), scene.opacity);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        if !surface.read_pixels(&info, &mut pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render(
                "unable to read raster RGBA pixels".into(),
            ));
        }
        #[derive(serde::Serialize)]
        struct Geometry {
            #[serde(flatten)]
            widget: WidgetGeometry,
            rotation: f32,
        }
        let geometry = serde_json::to_vec(&Geometry {
            widget: WidgetGeometry {
                left: bounds.left,
                top: bounds.top,
                width,
                height,
                content_width: frame.content.width(),
                content: frame.content.into(),
            },
            rotation: frame.rotation,
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self {
            pixels,
            geometry,
            revision: 1,
        })
    }
}
