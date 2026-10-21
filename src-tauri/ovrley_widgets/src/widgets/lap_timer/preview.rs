//! Activity and formatted-presentation caches for the shared lap timer raster surface.
use super::{
    format::{lap_timer_content, LapHistory, LapSample, LapTimerContent},
    layout::{prepare_lap_timer_layout, LapTimerLayout},
    validation::{validate_lap_timer, ValidatedLapTimer},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::text::FontResolver,
    render::preview::{render_tile, PreviewRasterScale},
    widgets::value::{
        config::ValueConfig,
        preview::{TEXT_COLOR_TYPE, TileBounds, WidgetGeometry},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewLapTimer {
    config: ValidatedLapTimer,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<LapTimerLayout>,
    last_content: Option<LapTimerContent>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewLapTimer {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_lap_timer(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Ok(Self::from_validated(config, validate_scene_presentation(scene)?, &host))
    }

    pub fn from_validated(
        config: ValidatedLapTimer,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        Self {
            config,
            scene,
            preview_raster_scale: host.raster_scale,
            layout: None,
            last_content: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        }
    }

    /// The content the frame at `second` draws; equal content draws identical pixels.
    pub fn frame_key(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<LapTimerContent> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        lap_timer_content(&self.config, history, sample)
    }

    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_content = None;
    }

    /// Glyph layout for the activity's lap history; it allocates no surface.
    fn layout(
        &mut self,
        history: LapHistory<'_>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<&LapTimerLayout> {
        if self.layout.is_none() {
            self.layout = Some(prepare_lap_timer_layout(
                &self.config,
                &self.scene,
                history,
                fonts,
            )?);
        }
        Ok(self.layout.as_ref().expect("layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, from glyph layout alone.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        // Direct glyph runs bound the same rectangle as the cached static layer.
        Ok(self.layout(history, fonts)?.prepare_frame(history, &content)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        let frame = self.layout(history, fonts)?.prepare_cached_frame(history, &content)?;
        frame.draw(canvas);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        if self.last_content.as_ref() == Some(&content) {
            return Ok(());
        }
        let frame = self.layout(history, fonts)?.prepare_cached_frame(history, &content)?;
        let bounds = frame.bounds;
        let (width, height) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            TEXT_COLOR_TYPE,
            self.preview_raster_scale,
            bounds,
            |canvas| {
                frame.draw(canvas);
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
        self.last_content = Some(content);
        Ok(())
    }
}
