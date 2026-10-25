//! Split-table and formatted-presentation caches for the running splits raster surface.
use super::{
    format::{splits_content, SplitsContent},
    layout::{prepare_splits_layout, SplitsLayout},
    table::SplitTable,
    validation::{validate_splits_run, ValidatedSplitsRun},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::preview::{render_tile, PreviewRasterScale},
    render::text::FontResolver,
    widgets::value::{
        config::ValueConfig,
        preview::{TileBounds, WidgetGeometry, TEXT_COLOR_TYPE},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewSplitsRun {
    config: ValidatedSplitsRun,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<SplitsLayout>,
    last_content: Option<SplitsContent>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewSplitsRun {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_splits_run(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Ok(Self::from_validated(config, validate_scene_presentation(scene)?, &host))
    }

    pub fn from_validated(config: ValidatedSplitsRun, scene: ValidatedScenePresentation, host: &WidgetHost) -> Self {
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

    /// The table core derived for this widget's split length, once the host has supplied it.
    fn table<'a>(&self, activity: Option<&'a ActivitySampler>) -> Option<&'a SplitTable> {
        activity.and_then(|activity| activity.split_table(self.config.length))
    }

    /// The content the frame at `second` draws; equal content draws identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> SplitsContent {
        splits_content(self.table(activity), second)
    }

    /// Column widths follow the split table, so a new activity or table rebuilds the layout.
    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_content = None;
    }

    fn layout(&mut self, table: Option<&SplitTable>, fonts: &dyn FontResolver) -> WidgetResult<&SplitsLayout> {
        if self.layout.is_none() {
            self.layout = Some(prepare_splits_layout(&self.config, &self.scene, table, fonts)?);
        }
        Ok(self.layout.as_ref().expect("layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        Ok(self.layout(table, fonts)?.frame_bounds(table, &content).1.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        let raster_scale = self.preview_raster_scale;
        let layout = self.layout(table, fonts)?;
        let frame = layout.prepare_frame(table, &content, raster_scale)?;
        frame.draw(canvas, layout);
        Ok(frame.bounds.into())
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64, fonts: &dyn FontResolver) -> WidgetResult<()> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        if self.last_content.as_ref() == Some(&content) {
            return Ok(());
        }
        self.layout(table, fonts)?;
        let layout = self.layout.as_ref().expect("layout prepared");
        let frame = layout.prepare_frame(table, &content, self.preview_raster_scale)?;
        let bounds = frame.bounds;
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            TEXT_COLOR_TYPE,
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
        self.last_content = Some(content);
        Ok(())
    }
}
