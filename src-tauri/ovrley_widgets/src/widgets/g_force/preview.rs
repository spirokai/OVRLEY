use super::{
    format::GForceSample,
    layout::{prepare_g_force_cache, GForceCache},
    validation::validate_g_force,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    render::pixels::read_surface_rgba,
    widgets::value::{
        config::ValueConfig,
        preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};

pub struct PreviewGForce {
    config: super::validation::ValidatedGForceWidget,
    cache: GForceCache,
    surface: Option<Surface>,
    last_sample: Option<GForceSample>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewGForce {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_g_force(widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        let cache = prepare_g_force_cache(&config, &scene)?;
        Ok(Self {
            config,
            cache,
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
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        // Marker direction and clipping scale can change while the formatted magnitude stays equal.
        if self.last_sample == Some(sample) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(sample, fonts)?;
        let bounds = frame.bounds;
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
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
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, &self.cache);
        canvas.restore();
        read_surface_rgba(surface, &mut self.pixels)?;
        self.geometry = serde_json::to_vec(&WidgetGeometry {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            content_width: frame.content_width,
            content: frame.content_bounds.into(),
        })
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(sample);
        Ok(())
    }
}
