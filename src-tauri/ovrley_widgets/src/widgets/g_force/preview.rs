use super::{
    format::GForceSample,
    layout::{prepare_g_force_cache, GForceCache},
    validation::validate_g_force,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::preview::{render_tile, PreviewRasterScale},
    render::text::FontResolver,
    widgets::value::{
        config::ValueConfig,
        preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewGForce {
    config: super::validation::ValidatedGForceWidget,
    cache: GForceCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<GForceSample>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewGForce {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_g_force(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(config, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: super::validation::ValidatedGForceWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let cache = prepare_g_force_cache(&config, scene, host.raster_scale)?;
        Ok(Self {
            config,
            cache,
            preview_raster_scale: host.raster_scale,
            surface: None,
            last_sample: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> GForceSample {
        activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        })
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        Ok(self.cache.prepare_frame(sample, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let sample = activity.map_or_else(GForceSample::default, |activity| {
            activity.g_force_at(&self.config, second)
        });
        let frame = self.cache.prepare_frame(sample, fonts)?;
        frame.draw(canvas, &self.cache);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
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
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.preview_raster_scale,
            bounds,
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        let tile = TileBounds::from(bounds);
        self.geometry = serde_json::to_vec(&WidgetGeometry::new(
            tile,
            (width, height),
            self.preview_raster_scale.value(),
            frame.content_width,
            frame.content_bounds.into(),
        ))
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.bounds = Some(tile);
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(sample);
        Ok(())
    }
}
