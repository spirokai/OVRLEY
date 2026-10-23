use super::{
    layout::{prepare_heading_compass_cache, HeadingCompassCache},
    validation::{validate_heading_compass, ValidatedHeadingCompass},
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
        preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewHeadingCompass {
    cache: HeadingCompassCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewHeadingCompass {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heading_compass(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeadingCompass,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heading_compass_cache(widget, scene, fonts, host.raster_scale)?,
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
        activity.and_then(|activity| activity.heading_at(second))
    }

    /// Scene rectangle a render at `second` draws into, from readout text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
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
        let raw = activity.and_then(|activity| activity.heading_at(second));
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
        let raw = activity.and_then(|activity| activity.heading_at(second));
        // The dial turns continuously, even between integer-degree readouts.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw, fonts)?;
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
        self.last_sample = Some(raw);
        Ok(())
    }
}
