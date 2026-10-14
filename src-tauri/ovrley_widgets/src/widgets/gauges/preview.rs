//! Session-owned gauge surfaces; activity changes invalidate static range labels too.
use super::{
    frame::GaugeLayout,
    metric::{sample_gauge, GaugeFrameState},
    validation::ValidatedGauge,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    preview::PreviewFonts,
    widgets::value::{
        config::ValueConfig,
        preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{AlphaType, Color, ColorType, ImageInfo, Surface};

pub struct PreviewGauge {
    widget: ValidatedGauge,
    scene: ValidatedScenePresentation,
    layout: Option<GaugeLayout>,
    last_state: Option<GaugeFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewGauge {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        Ok(Self {
            widget: ValidatedGauge::new(widget)?,
            scene: validate_scene_presentation(scene)?,
            layout: None,
            last_state: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }
    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_state = None;
    }
    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &PreviewFonts,
    ) -> WidgetResult<()> {
        if self.layout.is_none() {
            let range = activity.map_or((0.0, 100.0), |activity| {
                activity.gauge_range(self.widget.metric())
            });
            let offset = activity.map_or(0.0, |activity| {
                activity.gauge_altitude_offset(self.widget.starting_altitude_m())
            });
            self.layout = Some(GaugeLayout::new(
                &self.widget,
                &self.scene,
                range,
                offset,
                fonts,
            )?);
        }
        let state = sample_gauge(&self.widget, activity, second);
        // A continuous fill can change even while the formatted integer stays unchanged.
        if self.last_state.as_ref() == Some(&state) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("gauge layout prepared");
        let frame = layout.prepare_frame(state, fonts)?;
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
        let surface = self.surface.as_mut().expect("gauge surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, layout);
        canvas.restore();
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        if !surface.read_pixels(&info, &mut self.pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render(
                "unable to read gauge RGBA pixels".into(),
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
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        self.last_state = Some(frame.state);
        Ok(())
    }
}
