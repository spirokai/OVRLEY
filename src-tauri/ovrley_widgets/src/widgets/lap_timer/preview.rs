//! Activity and formatted-presentation caches for the shared lap timer raster surface.
use super::{
    format::{lap_timer_content, LapHistory, LapSample, LapTimerContent},
    layout::{prepare_lap_timer_layout, LapTimerLayout},
    validation::{validate_lap_timer, ValidatedLapTimer},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    preview::PreviewFonts,
    render::{pixels::read_surface_rgba, preview::PreviewRasterScale},
    widgets::value::{
        config::ValueConfig,
        preview::{surface, TEXT_COLOR_TYPE, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};

pub struct PreviewLapTimer {
    config: ValidatedLapTimer,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<LapTimerLayout>,
    last_content: Option<LapTimerContent>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewLapTimer {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let preview_raster_scale = PreviewRasterScale::from_scene(&scene)?;
        Ok(Self {
            config: validate_lap_timer(widget, 0)?,
            scene: validate_scene_presentation(scene)?,
            preview_raster_scale,
            layout: None,
            last_content: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }

    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_content = None;
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &PreviewFonts,
    ) -> WidgetResult<()> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        if self.last_content.as_ref() == Some(&content) {
            return Ok(());
        }
        if self.layout.is_none() {
            self.layout = Some(prepare_lap_timer_layout(
                &self.config,
                &self.scene,
                history,
                fonts,
            )?);
        }
        let frame = self
            .layout
            .as_ref()
            .expect("layout prepared")
            .prepare_cached_frame(history, &content)?;
        let bounds = frame.bounds;
        let (width, height) = self.preview_raster_scale.dimensions(bounds);
        if self
            .surface
            .as_ref()
            .map(|surface| (surface.width(), surface.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height, TEXT_COLOR_TYPE)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        self.preview_raster_scale.apply(canvas);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas);
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
        self.last_content = Some(content);
        Ok(())
    }
}
