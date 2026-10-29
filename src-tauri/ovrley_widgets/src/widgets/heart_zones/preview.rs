use super::{
    layout::{prepare_heart_zones_cache, HeartZonesCache},
    validation::{validate_heart_zones, ValidatedHeartZones},
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

pub struct PreviewHeartZones {
    cache: HeartZonesCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewHeartZones {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heart_zones(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeartZones,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heart_zones_cache(widget, scene, fonts, host.raster_scale)?,
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
        activity.and_then(|activity| activity.heartrate_at(second))
    }

    /// Scene rectangle a render draws into; the indicator travels inside fixed bounds.
    pub fn layout_bounds(&self) -> TileBounds {
        self.cache.bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> TileBounds {
        self.cache
            .prepare_frame(self.frame_key(activity, second))
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let raw = self.frame_key(activity, second);
        // The indicator moves continuously, even between whole-bpm samples.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw);
        let bounds = self.cache.bounds;
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
            self.cache.content_bounds.width(),
            self.cache.content_bounds.into(),
        ))
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.bounds = Some(tile);
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(raw);
        Ok(())
    }
}
