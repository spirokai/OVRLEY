use super::{config::RasterConfig, frame::RasterFrame, validation::{validate_raster_geometry, RasterGeometry}};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::image::{ImageSource, RasterImage},
    widgets::value::preview::{TileBounds, WidgetGeometry},
};
use skia_safe::{images, AlphaType, Canvas, ColorType, Data, Image, ImageInfo};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub struct PreviewImages(BTreeMap<String, RasterImage>);

impl PreviewImages {
    /// Oriented PNG bytes from the native immutable resource, decoded once per session.
    pub fn register(&mut self, id: String, bytes: &[u8]) -> WidgetResult<()> {
        if id.is_empty() {
            return Err(WidgetError::Config("empty raster resource identity".into()));
        }
        self.0.insert(id, RasterImage::decode(bytes)?);
        Ok(())
    }
}

impl ImageSource for PreviewImages {
    fn resolve_image(&self, resource_id: &str) -> WidgetResult<RasterImage> {
        self.0.get(resource_id).cloned().ok_or_else(|| {
            WidgetError::Render(format!("raster resource not registered: {resource_id}"))
        })
    }
}

pub struct PreviewRaster {
    pub pixels: Arc<Vec<u8>>,
    pub geometry: Vec<u8>,
    /// The unrotated content frame the preview stretches its source pixels over.
    pub bounds: TileBounds,
    pub revision: u32,
    source_id: Option<String>,
    pub pixels_revision: u32,
    frame: RasterFrame,
    dimensions: (u32, u32),
    scene_opacity: f32,
    /// Skia copy of the source pixels, made on the first host draw only.
    image: Option<Image>,
}

impl PreviewRaster {
    pub fn new(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &(impl ImageSource + ?Sized),
    ) -> WidgetResult<Self> {
        Self::prepare(widget, scene, images, None)
    }

    pub fn update(
        &mut self,
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &(impl ImageSource + ?Sized),
    ) -> WidgetResult<()> {
        *self = Self::prepare(widget, scene, images, Some(self))?;
        Ok(())
    }

    fn prepare(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        images: &(impl ImageSource + ?Sized),
        previous: Option<&Self>,
    ) -> WidgetResult<Self> {
        let config = validate_raster_geometry(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        if widget.resource_id.is_some() && widget.resource_error_code.is_some() {
            return Err(WidgetError::Config(
                "Raster has conflicting resource state".into(),
            ));
        }
        let source = if widget.path.is_none() || widget.resource_error_code.is_some() {
            None
        } else {
            let id = widget.resource_id.as_ref().ok_or_else(|| {
                WidgetError::Config("Raster has no loaded image resource".into())
            })?;
            Some((id.clone(), images.resolve_image(id)?))
        };
        Self::build(&config, &scene, source, previous)
    }

    /// Export hosts resolve and prepare the source image themselves.
    pub fn from_validated(
        config: &RasterGeometry,
        scene: &ValidatedScenePresentation,
        source_id: String,
        image: RasterImage,
    ) -> WidgetResult<Self> {
        Self::build(config, scene, Some((source_id, image)), None)
    }

    fn build(
        config: &RasterGeometry,
        scene: &ValidatedScenePresentation,
        source: Option<(String, RasterImage)>,
        previous: Option<&Self>,
    ) -> WidgetResult<Self> {
        // Keep source pixels immutable; presentation edits only update the frame.
        let (source_id, width, height, pixels) = match source {
            None => (None, 1, 1, Arc::new(vec![255; 4])),
            Some((id, image)) => (Some(id), image.width, image.height, image.pixels),
        };
        let frame = RasterFrame::new(config, scene.scale);
        let content = frame.content_bounds();
        // Source pixels stretch over the unrotated content frame.
        let bounds = TileBounds::from(content);
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
        }
        let geometry = serde_json::to_vec(&Geometry {
            widget: WidgetGeometry::new(bounds, (width, height), 1.0, content.width, content),
            rotation: frame.rotation,
            rotation_origin: content.rotation_origin(),
            opacity: frame.paint_opacity(source_id.is_some(), scene.opacity),
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self {
            pixels,
            geometry,
            bounds,
            revision: previous.map_or(1, |previous| previous.revision.wrapping_add(1)),
            source_id,
            pixels_revision,
            frame,
            dimensions: (width, height),
            scene_opacity: scene.opacity,
            image: None,
        })
    }

    /// The rotated scene extent the frame draws into, unlike the preview's content-frame tile.
    pub fn layout_bounds(&self) -> TileBounds {
        self.frame.bounds.into()
    }

    /// Draws the oriented frame onto a host canvas in scene coordinates.
    pub fn draw(&mut self, canvas: &Canvas) -> WidgetResult<TileBounds> {
        // Empty selections and failed persisted assets draw the editable placeholder.
        if self.source_id.is_some() && self.image.is_none() {
            let (width, height) = self.dimensions;
            let info = ImageInfo::new(
                (width as i32, height as i32),
                ColorType::RGBA8888,
                AlphaType::Unpremul,
                None,
            );
            self.image = Some(
                images::raster_from_data(&info, Data::new_copy(&self.pixels), width as usize * 4)
                    .ok_or_else(|| WidgetError::Render("unable to wrap raster RGBA pixels".into()))?,
            );
        }
        self.frame.draw(canvas, self.image.as_ref(), self.scene_opacity);
        Ok(self.layout_bounds())
    }
}
