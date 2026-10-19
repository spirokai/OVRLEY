use super::{
    layout::{prepare_lean_angle_cache, LeanAngleCache},
    validation::validate_lean_angle,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    render::{pixels::read_surface_rgba, preview::PreviewRasterScale},
    widgets::value::{
        config::ValueConfig,
        preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};

pub struct PreviewLeanAngle {
    cache: LeanAngleCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewLeanAngle {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_lean_angle(widget, 0)?;
        let preview_raster_scale = PreviewRasterScale::from_scene(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        let cache = prepare_lean_angle_cache(&config, &scene)?;
        Ok(Self {
            cache,
            preview_raster_scale,
            surface: None,
            last_sample: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &PreviewFonts,
    ) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        // Signed fill changes continuously, even when the magnitude label stays the same.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw, fonts)?;
        let bounds = frame.bounds;
        let (width, height) = self.preview_raster_scale.dimensions(bounds);
        if self
            .surface
            .as_ref()
            .map(|surface| (surface.width(), surface.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        self.preview_raster_scale.apply(canvas);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, &self.cache);
        canvas.restore();
        read_surface_rgba(surface, &mut self.pixels)?;
        self.geometry = serde_json::to_vec(&WidgetGeometry {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            preview_raster_scale: self.preview_raster_scale.value(),
            content_width: frame.content_width,
            content: frame.content_bounds.into(),
        })
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(raw);
        Ok(())
    }
}
