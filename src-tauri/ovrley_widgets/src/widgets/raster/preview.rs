use super::{
    config::RasterConfig,
    frame::RasterFrame,
    validation::{validate_raster_geometry, RasterGeometry},
};
use crate::{
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::{
        image::{ImageSource, RasterImage},
        preview::render_tile,
    },
    widgets::value::preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
};
use skia_safe::{images, AlphaType, Canvas, ColorType, Data, Image, ImageInfo, Surface};
use std::collections::BTreeMap;

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
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// The oriented frame's tile, drawn with Skia like native export.
    pub bounds: TileBounds,
    pub revision: u32,
    source_id: Option<String>,
    frame: RasterFrame,
    scene_opacity: f32,
    image: Option<Image>,
    surface: Option<Surface>,
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
        previous: Option<&mut Self>,
    ) -> WidgetResult<Self> {
        let config = validate_raster_geometry(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        if widget.resource_id.is_some() && widget.resource_error_code.is_some() {
            return Err(WidgetError::Config(
                "Raster has conflicting resource state".into(),
            ));
        }
        let source = if widget.path.is_none() || widget.resource_error_code.is_some() {
            None
        } else {
            let id = widget
                .resource_id
                .as_ref()
                .ok_or_else(|| WidgetError::Config("Raster has no loaded image resource".into()))?;
            Some((id.clone(), images.resolve_image(id)?))
        };
        Self::build(&config, &scene, &host, source, previous)
    }

    /// Export hosts resolve and prepare the source image themselves.
    pub fn from_validated(
        config: &RasterGeometry,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        source_id: String,
        image: RasterImage,
    ) -> WidgetResult<Self> {
        Self::build(config, scene, host, Some((source_id, image)), None)
    }

    fn build(
        config: &RasterGeometry,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        source: Option<(String, RasterImage)>,
        previous: Option<&mut Self>,
    ) -> WidgetResult<Self> {
        let (previous_image, mut surface, mut pixels, revision) = match previous {
            None => (None, None, Vec::new(), 1),
            Some(previous) => (
                previous
                    .image
                    .take()
                    .map(|image| (previous.source_id.take(), image)),
                previous.surface.take(),
                std::mem::take(&mut previous.pixels),
                previous.revision.wrapping_add(1),
            ),
        };
        // Empty selections and failed persisted assets draw the editable placeholder.
        let (source_id, image) = match source {
            None => (None, None),
            Some((id, image)) => match previous_image {
                // Wrap the immutable source once per resource, not per presentation edit.
                Some((Some(previous_id), previous)) if previous_id == id => {
                    (Some(id), Some(previous))
                }
                _ => (Some(id), Some(skia_image(&image)?)),
            },
        };
        let frame = RasterFrame::new(config, scene.scale);
        let content = frame.content_bounds();
        let raster_scale = host.raster_scale;
        let (width, height, tile) = render_tile(
            &mut surface,
            &mut pixels,
            SHAPE_COLOR_TYPE,
            raster_scale,
            frame.bounds,
            |canvas| {
                frame.draw(canvas, image.as_ref(), scene.opacity);
                Ok(())
            },
        )?;
        let bounds = TileBounds::from(tile);
        #[derive(serde::Serialize)]
        struct Geometry {
            #[serde(flatten)]
            widget: WidgetGeometry,
            rotation: f32,
            rotation_origin: [f32; 2],
        }
        let geometry = serde_json::to_vec(&Geometry {
            widget: WidgetGeometry::new(
                bounds,
                (width, height),
                raster_scale.value(),
                content.width,
                content,
            ),
            rotation: frame.rotation,
            rotation_origin: content.rotation_origin(),
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self {
            pixels,
            geometry,
            bounds,
            revision,
            source_id,
            frame,
            scene_opacity: scene.opacity,
            image,
            surface,
        })
    }

    /// The rotated scene extent the frame draws into.
    pub fn layout_bounds(&self) -> TileBounds {
        self.frame.bounds.into()
    }

    /// Draws the oriented frame onto a host canvas in scene coordinates.
    pub fn draw(&mut self, canvas: &Canvas) -> WidgetResult<TileBounds> {
        self.frame
            .draw(canvas, self.image.as_ref(), self.scene_opacity);
        Ok(self.layout_bounds())
    }
}

fn skia_image(image: &RasterImage) -> WidgetResult<Image> {
    let info = ImageInfo::new(
        (image.width as i32, image.height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    images::raster_from_data(
        &info,
        Data::new_copy(&image.pixels),
        image.width as usize * 4,
    )
    .ok_or_else(|| WidgetError::Render("unable to wrap raster RGBA pixels".into()))
}
