use super::{config::LabelConfig, frame::{prepare_label_frame, LabelFrame}, validation::{validate_label, ValidatedLabel}};
use crate::{
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::text::FontResolver,
    render::pixels::read_surface_rgba,
    widgets::value::preview::{surface, TileBounds, WidgetGeometry},
};
use skia_safe::{Canvas, Color};

/// Static pixels are produced only when the handle is created or its config changes.
pub struct PreviewLabel {
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub bounds: TileBounds,
    pub revision: u32,
    frame: LabelFrame,
}

impl PreviewLabel {
    pub fn new(widget: LabelConfig, scene: ScenePresentationConfig, fonts: &dyn FontResolver) -> WidgetResult<Self> {
        let label = validate_label(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&label, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(label: &ValidatedLabel, scene: &ValidatedScenePresentation, host: &WidgetHost, fonts: &dyn FontResolver) -> WidgetResult<Self> {
        let preview_raster_scale = host.raster_scale;
        let frame = prepare_label_frame(label, scene, fonts)?;
        let bounds = frame.bounds;
        let (width, height) = preview_raster_scale.dimensions(bounds);
        let mut surface = surface(width, height)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        preview_raster_scale.apply(canvas);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas)?;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        read_surface_rgba(&mut surface, &mut pixels)?;
        let bounds = TileBounds::from(bounds);
        let geometry = serde_json::to_vec(&WidgetGeometry::new(bounds, (width, height), preview_raster_scale.value(), frame.content_width, frame.content_bounds.into())).map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self { pixels, geometry, bounds, revision: 1, frame })
    }

    /// Static text: every sample draws into the same rectangle.
    pub fn layout_bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Draws the label onto a host canvas in scene coordinates.
    pub fn draw(&self, canvas: &Canvas) -> WidgetResult<TileBounds> {
        self.frame.draw(canvas)?;
        Ok(self.bounds)
    }
}
