use super::{
    config::RollingChartConfig,
    layout::{prepare_rolling_chart_cache, RollingChartCache},
    validation::{validate_rolling_chart, ValidatedRollingChart},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::{
        preview::{render_tile, PreviewRasterScale},
        text::FontResolver,
    },
    widgets::value::preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
};
use skia_safe::{Canvas, Surface};

pub struct PreviewRollingChart {
    cache: RollingChartCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_second: Option<f64>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewRollingChart {
    pub fn new(
        widget: RollingChartConfig,
        scene: ScenePresentationConfig,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_rolling_chart(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedRollingChart,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_rolling_chart_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            surface: None,
            last_second: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    /// The window slides with the playhead, so the frame depends on the second itself.
    pub fn frame_key(&self, second: f64) -> f64 {
        second
    }

    /// Scene rectangle a render draws into; lines and markers stay inside fixed bounds.
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
            .prepare_frame(activity, second)
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        if self.last_second == Some(second) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(activity, second);
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.preview_raster_scale,
            self.cache.bounds,
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
        self.last_second = Some(second);
        Ok(())
    }

    /// A replaced activity redraws the current second.
    pub fn invalidate_activity(&mut self) {
        self.last_second = None;
    }
}
