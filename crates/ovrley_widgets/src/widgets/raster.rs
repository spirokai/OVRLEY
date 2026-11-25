//! Raster images placed, rotated and faded in the overlay. Hosts resolve and decode the image.
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{ContentBounds, TileBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{Presented, Tile};
use crate::{WidgetError, WidgetResult};
use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};
use skia_safe::{
    images, AlphaType, Canvas, Color, ColorType, Data, FilterMode, Image, ImageInfo, Paint, Rect,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Complete template render configuration.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RasterConfig {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub opacity: f64,
    #[serde(deserialize_with = "deserialize_raster_path")]
    pub path: Option<String>,
    #[serde(
        default,
        rename = "resourceId",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_id: Option<String>,
    #[serde(
        default,
        rename = "resourceErrorCode",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_error_code: Option<RasterError>,
}

fn deserialize_present_resource<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn deserialize_raster_path<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum RasterError {
    #[error("Select an absolute image path.")]
    RelativePath,
    #[error("The image file is missing.")]
    Missing,
    #[error("The image file cannot be read.")]
    Unreadable,
    #[error("The image must be a PNG, JPEG, BMP, or TIFF file with a supported extension.")]
    UnsupportedType,
    #[error("The image exceeds the 5 MiB encoded file limit.")]
    EncodedSize,
    #[error("The image exceeds the 25 MP decoded resolution limit.")]
    Resolution,
    #[error("The image could not be decoded.")]
    Decode,
    #[error("The embedded image asset is missing. Select a replacement image.")]
    MissingAsset,
    #[error("The embedded image asset is corrupt. Select a replacement image.")]
    CorruptAsset,
}

impl RasterError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RelativePath => "relative_path",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::UnsupportedType => "unsupported_type",
            Self::EncodedSize => "encoded_size",
            Self::Resolution => "resolution",
            Self::Decode => "decode",
            Self::MissingAsset => "missing_asset",
            Self::CorruptAsset => "corrupt_asset",
        }
    }

    pub fn for_widget(self, id: &str) -> crate::WidgetError {
        crate::WidgetError::Config(format!(
            "Raster {id}: {self} [raster_error:{}:{id}]",
            self.code()
        ))
    }
}

pub fn valid_raster_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[derive(Clone, Debug)]
pub struct RasterGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
}

fn invalid_raster(id: &str, detail: String) -> WidgetError {
    WidgetError::Config(format!(
        "{detail} [raster_error:invalid_config:{}]",
        if id.is_empty() { "configuration" } else { id }
    ))
}

fn raster_number(id: &str, index: usize, name: &str, value: f64) -> WidgetResult<f32> {
    if !value.is_finite() || value.abs() > f32::MAX as f64 {
        return Err(invalid_raster(
            id,
            format!("rasters[{index}].{name} must be a finite number"),
        ));
    }
    Ok(value as f32)
}

pub fn validate_raster_geometry(raw: &RasterConfig, index: usize) -> WidgetResult<RasterGeometry> {
    if !valid_raster_id(&raw.id) {
        return Err(invalid_raster(
            &raw.id,
            format!(
                "rasters[{index}].id must contain only letters, digits, hyphens or underscores"
            ),
        ));
    }
    let width = raster_number(&raw.id, index, "width", raw.width)?;
    let height = raster_number(&raw.id, index, "height", raw.height)?;
    let opacity = raster_number(&raw.id, index, "opacity", raw.opacity)?;
    if width <= 0.0 || height <= 0.0 {
        return Err(invalid_raster(
            &raw.id,
            format!("Raster {} must have a positive frame size", raw.id),
        ));
    }
    if !(0.0..=1.0).contains(&raw.opacity) {
        return Err(invalid_raster(
            &raw.id,
            format!("Raster {} opacity must be between 0 and 1", raw.id),
        ));
    }
    Ok(RasterGeometry {
        x: raster_number(&raw.id, index, "x", raw.x)?,
        y: raster_number(&raw.id, index, "y", raw.y)?,
        width,
        height,
        rotation: raster_number(&raw.id, index, "rotation", raw.rotation)?,
        opacity,
    })
}

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

/// One oriented frame for export pixels and editor interaction geometry.
pub struct RasterFrame {
    pub content: Rect,
    pub bounds: Rect,
    pub rotation: f32,
    opacity: f32,
}

impl RasterFrame {
    pub fn new(config: &RasterGeometry, scale: f32) -> Self {
        let content = Rect::from_xywh(
            config.x,
            config.y,
            config.width * scale,
            config.height * scale,
        );
        let (sin, cos) = config.rotation.to_radians().sin_cos();
        let points = [
            (0.0, 0.0),
            (content.width(), 0.0),
            (0.0, content.height()),
            (content.width(), content.height()),
        ]
        .map(|(x, y)| (config.x + x * cos - y * sin, config.y + x * sin + y * cos));
        let bounds = Rect::new(
            points
                .iter()
                .map(|p| p.0)
                .fold(f32::INFINITY, f32::min)
                .floor(),
            points
                .iter()
                .map(|p| p.1)
                .fold(f32::INFINITY, f32::min)
                .floor(),
            points
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil(),
            points
                .iter()
                .map(|p| p.1)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil(),
        );
        Self {
            content,
            bounds,
            rotation: config.rotation,
            opacity: config.opacity,
        }
    }

    pub fn content_bounds(&self) -> ContentBounds {
        ContentBounds::rotated_frame(
            Rect::from_wh(self.content.width(), self.content.height()),
            self.rotation,
            self.content.left,
            self.content.top,
        )
    }

    pub fn paint_opacity(&self, has_image: bool, scene_opacity: f32) -> f32 {
        self.opacity * scene_opacity * if has_image { 1.0 } else { 0.5 }
    }

    pub fn draw(&self, canvas: &Canvas, image: Option<&Image>, scene_opacity: f32) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::WHITE);
        paint.set_alpha_f(self.paint_opacity(image.is_some(), scene_opacity));
        canvas.save();
        canvas.translate((self.content.left, self.content.top));
        canvas.rotate(self.rotation, None);
        let rect = Rect::from_wh(self.content.width(), self.content.height());
        if let Some(image) = image {
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                rect,
                FilterMode::Linear,
                &paint,
            );
        } else {
            // Empty selections and failed persisted assets have an editable placeholder.
            canvas.draw_rect(rect, &paint);
        }
        canvas.restore();
    }
}

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn resize_raster(
    widget: RasterConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(WidgetError::Config("invalid raster resize size".into()));
    }
    let config = validate_raster_geometry(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let mut width = size[0] / scene.scale;
    let mut height = size[1] / scene.scale;
    if fixed[0] != 0.5 && fixed[1] != 0.5 {
        let x_factor = width / config.width;
        let y_factor = height / config.height;
        let factor = if (x_factor - 1.0).abs() >= (y_factor - 1.0).abs() {
            x_factor
        } else {
            y_factor
        };
        width = config.width * factor;
        height = config.height * factor;
    }
    let width = width.max(8.0);
    let height = height.max(8.0);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let dx = width * scene.scale * fixed[0];
    let dy = height * scene.scale * fixed[1];
    Ok(ResizeUpdate {
        x: point[0] - dx * cos + dy * sin,
        y: point[1] - dx * sin - dy * cos,
        width,
        height,
    })
}

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
    /// The oriented frame's tile, drawn with Skia like native export.
    pub tile: Tile,
    source_id: Option<String>,
    frame: RasterFrame,
    scene_opacity: f32,
    image: Option<Image>,
}

impl PreviewRaster {
    pub fn new(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        images: &(impl ImageSource + ?Sized),
    ) -> WidgetResult<Self> {
        Self::prepare(widget, scene, raster_scale, images, None)
    }

    pub fn update(
        &mut self,
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        images: &(impl ImageSource + ?Sized),
    ) -> WidgetResult<()> {
        *self = Self::prepare(widget, scene, raster_scale, images, Some(self))?;
        Ok(())
    }

    fn prepare(
        widget: RasterConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        images: &(impl ImageSource + ?Sized),
        previous: Option<&mut Self>,
    ) -> WidgetResult<Self> {
        let config = validate_raster_geometry(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
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
        let (previous_image, mut tile) = match previous {
            None => (None, Tile::default()),
            Some(previous) => (
                previous
                    .image
                    .take()
                    .map(|image| (previous.source_id.take(), image)),
                std::mem::take(&mut previous.tile),
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
        tile.render(
            host.raster_scale,
            SHAPE_COLOR_TYPE,
            frame.bounds,
            Presented::content(content).rotated(frame.rotation),
            |canvas| {
                frame.draw(canvas, image.as_ref(), scene.opacity);
                Ok(())
            },
        )?;
        Ok(Self {
            tile,
            source_id,
            frame,
            scene_opacity: scene.opacity,
            image,
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

/// Oriented source pixels in straight-alpha RGBA, shared without copying.
#[derive(Clone)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}

impl RasterImage {
    /// Decodes encoded image bytes directly to straight-alpha RGBA.
    pub fn decode(bytes: &[u8]) -> WidgetResult<Self> {
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
        Ok(Self {
            width,
            height,
            pixels: Arc::new(pixels),
        })
    }
}

pub trait ImageSource {
    fn resolve_image(&self, resource_id: &str) -> WidgetResult<RasterImage>;
}
