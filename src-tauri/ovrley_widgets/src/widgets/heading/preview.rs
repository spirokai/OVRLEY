use super::{
    layout::{prepare_heading_cache, HeadingCache},
    validation::{validate_heading, ValidatedHeading},
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

pub struct PreviewHeading {
    cache: HeadingCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_heading: Option<f32>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewHeading {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heading(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeading,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heading_cache(widget, scene, fonts)?,
            preview_raster_scale: host.raster_scale,
            surface: None,
            last_heading: None,
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

    /// Scene rectangle a render at `second` draws into; the tape's frame is fixed.
    pub fn layout_bounds(&self, activity: Option<&ActivitySampler>, second: f64) -> TileBounds {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        self.cache.prepare_frame(raw).bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(&self, canvas: &Canvas, activity: Option<&ActivitySampler>, second: f64) -> TileBounds {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let frame = self.cache.prepare_frame(raw);
        frame.draw(canvas, &self.cache);
        frame.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let heading = raw.unwrap_or(0.0) as f32;
        // The strip scrolls continuously, even between integer-degree headings.
        if self.last_heading == Some(heading) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw);
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
        self.last_heading = Some(heading);
        Ok(())
    }
}
