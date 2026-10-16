use super::{
    frame::{prepare_gradient_frame, validated_gradient_style},
    validation::{validate_gradient_widget, ValidatedGradientWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    render::{pixels::read_surface_rgba, text::ResolvedTextStyle},
    widgets::value::{
        config::ValueConfig,
        preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};

pub struct PreviewGradient {
    config: ValidatedGradientWidget,
    style: ResolvedTextStyle,
    scale: f32,
    scene_opacity: f32,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewGradient {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_gradient_widget(widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        let style = validated_gradient_style(&config, &scene, scene.scale);
        Ok(Self {
            config,
            style,
            scale: scene.scale,
            scene_opacity: scene.opacity,
            surface: None,
            last_sample: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &PreviewFonts,
    ) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.gradient_at(second));
        // The triangle moves continuously even while the formatted percentage stays unchanged.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = prepare_gradient_frame(
            &self.config, &self.style, raw, self.scale, self.scene_opacity, fonts,
        )?;
        let bounds = frame.bounds;
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        if self.surface.as_ref().map(|s| (s.width(), s.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.translate((-bounds.left, -bounds.top));
        let result = frame.draw(canvas, fonts);
        canvas.restore();
        result?;
        read_surface_rgba(surface, &mut self.pixels)?;
        self.geometry = serde_json::to_vec(&WidgetGeometry {
            left: bounds.left,
            top: bounds.top,
            width,
            height,
            content_width: frame.content_width,
            content: frame.content_bounds.into(),
        })
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(raw);
        Ok(())
    }
}
