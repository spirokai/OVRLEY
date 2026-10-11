//! Standalone telemetry value surfaces and geometry.
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    render::text::{validated_value_style, ResolvedTextStyle},
    widgets::value::{
        config::ValueConfig,
        format::{format_speed_parts, MetricDisplayParts},
        frame::prepare_metric_frame,
        layout::{
            draw_prepared_metric_frame, draw_static_metric_parts_for_value,
            static_metric_parts_for_value, StaticMetricParts,
        },
        validation::{validate_value_widget, ValidatedValueWidget},
    },
};
use serde::{Deserialize, Serialize};
use skia_safe::{surfaces, AlphaType, Color, ColorType, Image, ImageInfo, Surface};

#[derive(Deserialize)]
pub struct WidgetRequest {
    pub widget: ValueConfig,
    pub scene: ScenePresentationConfig,
}

#[derive(Serialize)]
pub struct WidgetGeometry {
    pub left: f32,
    pub top: f32,
    pub width: u32,
    pub height: u32,
    pub content_width: f32,
    pub content: ContentBounds,
}

/// Tight interaction rectangle in scene pixels, independent of raster allocation.
#[derive(Serialize)]
pub struct ContentBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
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

pub struct PreviewWidget {
    config: ValidatedValueWidget,
    style: ResolvedTextStyle,
    scale: f32,
    static_parts: StaticMetricParts,
    static_image: Option<StaticImage>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
    last_parts: Option<MetricDisplayParts>,
}

fn surface(width: u32, height: u32) -> WidgetResult<Surface> {
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
    surfaces::raster(&info, None, None)
        .ok_or_else(|| WidgetError::Render("widget surface allocation failed".into()))
}

impl PreviewWidget {
    pub fn new(request: WidgetRequest, fonts: &PreviewFonts) -> WidgetResult<Self> {
        if request.widget.value != crate::model::types::MetricKind::Speed
            || request.widget.display_type != crate::model::types::DisplayType::Text
        {
            return Err(WidgetError::Config(
                "WASM preview currently supports speed/text".into(),
            ));
        }
        let config = validate_value_widget(request.widget, 0)?;
        let scene = validate_scene_presentation(request.scene)?;
        let style = validated_value_style(&config, &scene, scene.scale);
        let static_parts = static_metric_parts_for_value(&config);
        let static_image = if static_parts.icon || static_parts.unit {
            let parts = format_speed_parts(&config, Some(0.0));
            let frame = prepare_metric_frame(&style, &parts, scene.scale, fonts, &config)?;
            let bounds = frame.bounds;
            let mut surface = surface(bounds.width() as u32, bounds.height() as u32)?;
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((-bounds.left, -bounds.top));
            draw_static_metric_parts_for_value(canvas, &config, &style, scene.scale, fonts)?;
            Some(StaticImage {
                image: surface.image_snapshot(),
                left: bounds.left,
                top: bounds.top,
            })
        } else {
            None
        };
        Ok(Self {
            config,
            style,
            scale: scene.scale,
            static_parts,
            static_image,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
            last_parts: None,
        })
    }

    pub fn render(&mut self, speed: Option<f64>, fonts: &PreviewFonts) -> WidgetResult<()> {
        let parts = format_speed_parts(&self.config, speed);
        if self.last_parts.as_ref() == Some(&parts) {
            return Ok(());
        }
        let frame = prepare_metric_frame(&self.style, &parts, self.scale, fonts, &self.config)?;
        let mut bounds = frame.bounds;
        if let Some(cache) = &self.static_image {
            bounds.join(skia_safe::Rect::from_xywh(
                cache.left,
                cache.top,
                cache.image.width() as f32,
                cache.image.height() as f32,
            ));
        }
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        if self.surface.as_ref().map(|s| (s.width(), s.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.translate((-bounds.left, -bounds.top));
        if let Some(cache) = &self.static_image {
            canvas.draw_image(&cache.image, (cache.left, cache.top), None);
        }
        let result = draw_prepared_metric_frame(
            canvas,
            &self.style,
            &parts,
            fonts,
            self.static_parts,
            &self.config,
            &frame,
        );
        canvas.restore();
        result?;
        // Browser ImageData consumes straight alpha; Skia draws into premultiplied pixels.
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        if !surface.read_pixels(&info, &mut self.pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render(
                "unable to read widget RGBA pixels".into(),
            ));
        }
        self.geometry = serde_json::to_vec(&WidgetGeometry {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            content_width: frame.content_width,
            content: frame.content_bounds.into(),
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        self.last_parts = Some(parts);
        Ok(())
    }
}
