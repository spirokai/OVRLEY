use super::{
    config::BackdropConfig,
    frame::prepare_backdrop_frame,
    validation::{validate_backdrop, ValidatedBackdrop},
};
use crate::{
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::pixels::read_surface_rgba,
    widgets::value::preview::{surface, SHAPE_COLOR_TYPE, TileBounds, WidgetGeometry},
};
use skia_safe::{Canvas, Color};

/// Backdrops produce pixels at creation/update only, without fonts or activity.
pub struct PreviewBackdrop {
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub bounds: TileBounds,
    pub revision: u32,
    backdrop: ValidatedBackdrop,
    scale: f32,
    scene_opacity: f32,
}

impl PreviewBackdrop {
    pub fn new(widget: BackdropConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let backdrop = validate_backdrop(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(backdrop, &validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        backdrop: ValidatedBackdrop,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let preview_raster_scale = host.raster_scale;
        let frame = prepare_backdrop_frame(&backdrop, scene.scale);
        let bounds = frame.bounds;
        let (width, height) = preview_raster_scale.dimensions(bounds);
        let mut surface = surface(width, height, SHAPE_COLOR_TYPE)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        preview_raster_scale.apply(canvas);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, scene.opacity);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        read_surface_rgba(&mut surface, &mut pixels)?;
        let bounds = TileBounds::from(bounds);
        let geometry = serde_json::to_vec(&WidgetGeometry::new(
            bounds, (width, height), preview_raster_scale.value(),
            frame.content_bounds.width(), frame.content_bounds.into(),
        )).map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self {
            pixels,
            geometry,
            bounds,
            revision: 1,
            scale: scene.scale,
            scene_opacity: scene.opacity,
            backdrop,
        })
    }

    /// Fixed geometry: every sample draws into the same rectangle.
    pub fn layout_bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Draws the backdrop onto a host canvas in scene coordinates.
    pub fn draw(&self, canvas: &Canvas) -> TileBounds {
        prepare_backdrop_frame(&self.backdrop, self.scale).draw(canvas, self.scene_opacity);
        self.bounds
    }
}
