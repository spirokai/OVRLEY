//! Session-owned gauge surfaces; activity changes invalidate static range labels too.
use super::{
    frame::{GaugeLayout, GaugeMeasure},
    metric::{sample_gauge, GaugeFrameState},
    validation::ValidatedGauge,
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
        preview::{SHAPE_COLOR_TYPE, TileBounds, WidgetGeometry},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewGauge {
    widget: ValidatedGauge,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<GaugeLayout>,
    /// Layout-only measurement for bounds requested before the layout is drawn.
    measure: Option<GaugeMeasure>,
    last_state: Option<GaugeFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewGauge {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let widget = ValidatedGauge::new(widget)?;
        let host = WidgetHost::from_scene(&scene)?;
        Ok(Self::from_validated(widget, validate_scene_presentation(scene)?, &host))
    }

    pub fn from_validated(
        widget: ValidatedGauge,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        Self {
            widget,
            scene,
            preview_raster_scale: host.raster_scale,
            layout: None,
            measure: None,
            last_state: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        }
    }

    /// The state the frame at `second` draws; equal states draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> GaugeFrameState {
        sample_gauge(&self.widget, activity, second)
    }
    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.measure = None;
        self.last_state = None;
    }
    /// Range labels follow the activity's metric range and altitude correction.
    fn activity_range(&self, activity: Option<&ActivitySampler>) -> ((f64, f64), f64) {
        let range = activity.map_or((0.0, 100.0), |activity| {
            activity.gauge_range(self.widget.metric())
        });
        let offset = activity.map_or(0.0, |activity| {
            activity.gauge_altitude_offset(self.widget.starting_altitude_m())
        });
        (range, offset)
    }

    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<&GaugeLayout> {
        if self.layout.is_none() {
            let (range, offset) = self.activity_range(activity);
            self.layout = Some(GaugeLayout::new(
                &self.widget,
                &self.scene,
                range,
                offset,
                fonts,
                self.preview_raster_scale,
            )?);
        }
        Ok(self.layout.as_ref().expect("gauge layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, without drawing the static layer.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let state = sample_gauge(&self.widget, activity, second);
        if let Some(layout) = &self.layout {
            return Ok(layout.prepare_frame(state, fonts)?.bounds.into());
        }
        if self.measure.is_none() {
            let (range, offset) = self.activity_range(activity);
            self.measure = Some(GaugeMeasure::new(
                &self.widget,
                &self.scene,
                range,
                offset,
                fonts,
            )?);
        }
        let measure = self.measure.as_ref().expect("gauge measure prepared");
        Ok(measure.frame_bounds(state, fonts)?.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let state = sample_gauge(&self.widget, activity, second);
        let layout = self.layout(activity, fonts)?;
        let frame = layout.prepare_frame(state, fonts)?;
        frame.draw(canvas, layout);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        self.layout(activity, fonts)?;
        let state = sample_gauge(&self.widget, activity, second);
        // A continuous fill can change even while the formatted integer stays unchanged.
        if self.last_state.as_ref() == Some(&state) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("gauge layout prepared");
        let frame = layout.prepare_frame(state, fonts)?;
        let bounds = frame.bounds;
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.preview_raster_scale,
            bounds,
            |canvas| {
                frame.draw(canvas, layout);
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
        self.last_state = Some(frame.state);
        Ok(())
    }
}
