use super::{
    frame::{prepare_gradient_frame, validated_gradient_style, GradientFrame},
    validation::{validate_gradient_widget, ValidatedGradientWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
    render::text::FontResolver,
    render::{
        preview::{render_tile, PreviewRasterScale},
        text::ResolvedTextStyle,
    },
    widgets::value::{
        config::ValueConfig,
        preview::{TEXT_COLOR_TYPE, TileBounds, WidgetGeometry},
    },
};
use skia_safe::{Canvas, Surface};

pub struct PreviewGradient {
    config: ValidatedGradientWidget,
    style: ResolvedTextStyle,
    scale: f32,
    preview_raster_scale: PreviewRasterScale,
    scene_opacity: f32,
    surface: Option<Surface>,
    last_sample: Option<Option<f64>>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewGradient {
    pub fn new(widget: ValueConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_gradient_widget(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Ok(Self::from_validated(config, &validate_scene_presentation(scene)?, &host))
    }

    pub fn from_validated(
        config: ValidatedGradientWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        let preview_raster_scale = host.raster_scale;
        let style = validated_gradient_style(&config, scene, scene.scale);
        Self {
            config,
            style,
            scale: scene.scale,
            preview_raster_scale,
            scene_opacity: scene.opacity,
            surface: None,
            last_sample: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        }
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.gradient_at(second))
    }

    fn frame(&self, raw: Option<f64>, fonts: &dyn FontResolver) -> WidgetResult<GradientFrame> {
        prepare_gradient_frame(
            &self.config, &self.style, raw, self.scale, self.scene_opacity, fonts,
        )
    }

    /// Scene rectangle a render at `second` draws into, from text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.gradient_at(second));
        Ok(self.frame(raw, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.gradient_at(second));
        let frame = self.frame(raw, fonts)?;
        frame.draw(canvas, fonts)?;
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let raw = self.frame_key(activity, second);
        // The triangle moves continuously even while the formatted percentage stays unchanged.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.frame(raw, fonts)?;
        let bounds = frame.bounds;
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            TEXT_COLOR_TYPE,
            self.preview_raster_scale,
            bounds,
            |canvas| frame.draw(canvas, fonts),
        )?;
        let tile = TileBounds::from(bounds);
        self.geometry = serde_json::to_vec(&WidgetGeometry::new(
            tile,
            (width, height),
            self.preview_raster_scale.value(),
            frame.content_width,
            frame.content_bounds.into(),
        ))
        .map_err(|e| WidgetError::Render(e.to_string()))?;
        self.bounds = Some(tile);
        self.revision = self.revision.wrapping_add(1);
        self.last_sample = Some(raw);
        Ok(())
    }
}
