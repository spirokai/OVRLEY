use super::{
    layout::{prepare_lean_angle_cache, LeanAngleCache},
    validation::{validate_lean_angle, ValidatedLeanAngleWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::text::FontResolver,
    render::preview::{render_tile, PreviewRasterScale},
    widgets::value::{
        config::ValueConfig,
        preview::{SHAPE_COLOR_TYPE, TileBounds, WidgetGeometry},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewLeanAngle {
    cache: LeanAngleCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewLeanAngle {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_lean_angle(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&config, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: &ValidatedLeanAngleWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_lean_angle_cache(config, scene)?,
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
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.lean_angle_at(second))
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        Ok(self.cache.prepare_frame(raw, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        let frame = self.cache.prepare_frame(raw, fonts)?;
        frame.draw(canvas, &self.cache);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.lean_angle_at(second));
        // Signed fill changes continuously, even when the magnitude label stays the same.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw, fonts)?;
        let bounds = frame.bounds;
        let (width, height) = render_tile(
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
        self.last_sample = Some(raw);
        Ok(())
    }
}
