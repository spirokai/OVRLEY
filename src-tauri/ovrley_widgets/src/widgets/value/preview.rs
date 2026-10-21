//! Standalone telemetry value surfaces and geometry.
use crate::{
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::text::FontResolver,
    render::{
        preview::{render_tile, PreviewRasterScale},
        text::{validated_value_style, ResolvedTextStyle},
    },
    widgets::value::{
        config::ValueConfig,
        format::MetricDisplayParts,
        frame::{prepare_metric_frame, MetricFrame},
        layout::{
            draw_prepared_metric_frame, draw_static_metric_parts_for_value,
            static_metric_parts_for_value, StaticMetricParts,
        },
        presentation::ValuePresentation,
    },
};
use serde::{Deserialize, Serialize};
use skia_safe::{surfaces, AlphaType, Canvas, Color, ColorType, Image, ImageInfo, Rect, Surface};

#[derive(Clone, PartialEq, Deserialize)]
pub struct WidgetRequest {
    pub widget: ValueConfig,
    pub scene: ScenePresentationConfig,
    #[serde(default)]
    pub transient: bool,
}

#[derive(Deserialize, Serialize)]
pub struct WidgetGeometry {
    pub left: f32,
    pub top: f32,
    pub width: u32,
    pub height: u32,
    pub preview_raster_scale: f32,
    pub content_width: f32,
    pub content: ContentBounds,
}

impl WidgetGeometry {
    /// Serialized placement of a rendered tile; its origin is the tile's typed bounds.
    pub(crate) fn new(
        bounds: TileBounds,
        (width, height): (u32, u32),
        preview_raster_scale: f32,
        content_width: f32,
        content: ContentBounds,
    ) -> Self {
        Self {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            preview_raster_scale,
            content_width,
            content,
        }
    }
}

/// Scene-pixel rectangle covered by a widget's rendered tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl TileBounds {
    /// Placement moves the tile without re-rendering; match the serialized translation.
    pub fn translated(self, dx: f64, dy: f64) -> Self {
        Self {
            left: (self.left as f64 + dx) as f32,
            top: (self.top as f64 + dy) as f32,
            ..self
        }
    }
}

impl From<skia_safe::Rect> for TileBounds {
    fn from(rect: skia_safe::Rect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width(),
            height: rect.height(),
        }
    }
}

impl From<ContentBounds> for TileBounds {
    fn from(content: ContentBounds) -> Self {
        Self {
            left: content.left,
            top: content.top,
            width: content.width,
            height: content.height,
        }
    }
}

/// Interaction rectangle in scene pixels, independent of raster allocation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ContentBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl ContentBounds {
    /// Center-pivoted editor frame for a local rectangle drawn at the saved origin.
    pub fn rotated_frame(rect: skia_safe::Rect, rotation: f32, x: f32, y: f32) -> Self {
        let center = (
            rect.left + rect.width() * 0.5,
            rect.top + rect.height() * 0.5,
        );
        let (sin, cos) = rotation.to_radians().sin_cos();
        Self {
            left: x + center.0 * cos - center.1 * sin - rect.width() * 0.5,
            top: y + center.0 * sin + center.1 * cos - rect.height() * 0.5,
            width: rect.width(),
            height: rect.height(),
        }
    }

    pub fn rotation_origin(&self) -> [f32; 2] {
        [self.width * 0.5, self.height * 0.5]
    }
}

impl From<skia_safe::Rect> for ContentBounds {
    fn from(rect: skia_safe::Rect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width(),
            height: rect.height(),
        }
    }
}

struct StaticImage {
    image: Image,
    left: f32,
    top: f32,
}

/// Layout and drawing state, borrowed apart from the tile surface while rendering.
struct ValueLayout {
    config: ValuePresentation,
    style: ResolvedTextStyle,
    scale: f32,
    static_parts: StaticMetricParts,
    static_image: Option<StaticImage>,
}

impl ValueLayout {
    /// Layout step: the metric frame and the tile rectangle covering it and the static parts.
    fn frame(&self, parts: &MetricDisplayParts, fonts: &dyn FontResolver) -> WidgetResult<(MetricFrame, Rect)> {
        let frame = prepare_metric_frame(&self.style, parts, self.scale, fonts, self.config.base())?;
        let mut bounds = frame.bounds;
        if let Some(cache) = &self.static_image {
            bounds.join(Rect::from_xywh(
                cache.left,
                cache.top,
                cache.image.width() as f32,
                cache.image.height() as f32,
            ));
        }
        Ok((frame, bounds))
    }

    fn draw(
        &self,
        canvas: &Canvas,
        parts: &MetricDisplayParts,
        fonts: &dyn FontResolver,
        frame: &MetricFrame,
    ) -> WidgetResult<()> {
        if let Some(cache) = &self.static_image {
            canvas.draw_image(&cache.image, (cache.left, cache.top), None);
        }
        draw_prepared_metric_frame(
            canvas,
            &self.style,
            parts,
            fonts,
            self.static_parts,
            self.config.base(),
            frame,
        )
    }
}

pub struct PreviewWidget {
    layout: ValueLayout,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
    last_parts: Option<MetricDisplayParts>,
}

thread_local! {
    static ALLOCATED_SURFACES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Widget surfaces allocated on this thread. Layout-only work must leave it unchanged.
pub fn allocated_surfaces() -> u64 {
    ALLOCATED_SURFACES.with(std::cell::Cell::get)
}

pub(crate) fn surface(width: u32, height: u32) -> WidgetResult<Surface> {
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err(WidgetError::Render(format!(
            "widget surface too large: {width}x{height}"
        )));
    }
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    ALLOCATED_SURFACES.with(|count| count.set(count.get() + 1));
    surfaces::raster(&info, None, None)
        .ok_or_else(|| WidgetError::Render("widget surface allocation failed".into()))
}

impl PreviewWidget {
    pub fn new(request: WidgetRequest, fonts: &dyn FontResolver) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&request.scene)?;
        let config = ValuePresentation::new(request.widget, &host)?;
        Self::from_validated(config, &validate_scene_presentation(request.scene)?, &host, fonts)
    }

    pub fn from_validated(
        config: ValuePresentation,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let preview_raster_scale = host.raster_scale;
        let style = validated_value_style(config.base(), scene, scene.scale);
        let static_parts = static_metric_parts_for_value(config.base());
        let static_image = if static_parts.icon || static_parts.unit {
            let parts = config.parts(None, 0.0);
            let frame = prepare_metric_frame(&style, &parts, scene.scale, fonts, config.base())?;
            let bounds = frame.bounds;
            let mut surface = surface(bounds.width() as u32, bounds.height() as u32)?;
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((-bounds.left, -bounds.top));
            draw_static_metric_parts_for_value(canvas, config.base(), &style, scene.scale, fonts)?;
            Some(StaticImage {
                image: surface.image_snapshot(),
                left: bounds.left,
                top: bounds.top,
            })
        } else {
            None
        };
        Ok(Self {
            layout: ValueLayout {
                config,
                style,
                scale: scene.scale,
                static_parts,
                static_image,
            },
            preview_raster_scale,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
            last_parts: None,
        })
    }

    /// The parts the frame at `second` draws; equal parts draw identical pixels.
    pub fn frame_key(
        &self,
        activity: Option<&crate::activity::ActivitySampler>,
        second: f64,
    ) -> MetricDisplayParts {
        self.layout.config.parts(activity, second)
    }

    /// Scene rectangle a render at `second` draws into, from text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&crate::activity::ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let parts = self.layout.config.parts(activity, second);
        Ok(self.layout.frame(&parts, fonts)?.1.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&crate::activity::ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let parts = self.layout.config.parts(activity, second);
        let (frame, bounds) = self.layout.frame(&parts, fonts)?;
        self.layout.draw(canvas, &parts, fonts, &frame)?;
        Ok(bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&crate::activity::ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let parts = self.layout.config.parts(activity, second);
        if self.last_parts.as_ref() == Some(&parts) {
            return Ok(());
        }
        let (frame, bounds) = self.layout.frame(&parts, fonts)?;
        let layout = &self.layout;
        let (width, height) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            self.preview_raster_scale,
            bounds,
            |canvas| layout.draw(canvas, &parts, fonts, &frame),
        )?;
        let tile = TileBounds::from(bounds);
        self.geometry = serde_json::to_vec(&WidgetGeometry::new(
            tile,
            (width, height),
            self.preview_raster_scale.value(),
            frame.content_width,
            frame.content_bounds.into(),
        ))
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        self.bounds = Some(tile);
        self.revision = self.revision.wrapping_add(1);
        self.last_parts = Some(parts);
        Ok(())
    }
}
