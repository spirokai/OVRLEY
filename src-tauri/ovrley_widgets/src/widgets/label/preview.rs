use super::{config::LabelConfig, frame::prepare_label_frame, validation::validate_label};
use crate::{error::{WidgetError, WidgetResult}, model::scene::{validate_scene_presentation, ScenePresentationConfig}, preview::PreviewFonts, widgets::value::preview::{surface, WidgetGeometry}};
use skia_safe::{AlphaType, Color, ColorType, ImageInfo};

/// Static pixels are produced only when the handle is created or its config changes.
pub struct PreviewLabel {
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewLabel {
    pub fn new(widget: LabelConfig, scene: ScenePresentationConfig, fonts: &PreviewFonts) -> WidgetResult<Self> {
        let label = validate_label(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        let frame = prepare_label_frame(&label, &scene, fonts)?;
        let bounds = frame.bounds;
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        let mut surface = surface(width, height)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas)?;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let info = ImageInfo::new((width as i32, height as i32), ColorType::RGBA8888, AlphaType::Unpremul, None);
        if !surface.read_pixels(&info, &mut pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render("unable to read label RGBA pixels".into()));
        }
        let geometry = serde_json::to_vec(&WidgetGeometry { left: bounds.left, top: bounds.top, width, height, content_width: frame.content_width, content: frame.content_bounds.into() }).map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(Self { pixels, geometry, revision: 1 })
    }
}
