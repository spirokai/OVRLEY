use super::{
    layout::{prepare_heading_cache, HeadingCache},
    validation::validate_heading,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    widgets::value::{
        config::ValueConfig,
        preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{AlphaType, Color, ColorType, ImageInfo, Surface};

pub struct PreviewHeading {
    cache: HeadingCache,
    surface: Option<Surface>,
    last_heading: Option<f32>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewHeading {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        fonts: &PreviewFonts,
    ) -> WidgetResult<Self> {
        let widget = validate_heading(widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        Ok(Self {
            cache: prepare_heading_cache(&widget, &scene, fonts)?,
            surface: None,
            last_heading: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let heading = raw.unwrap_or(0.0) as f32;
        // The strip scrolls continuously, even between integer-degree headings.
        if self.last_heading == Some(heading) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw);
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
        let surface = self.surface.as_mut().expect("surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.translate((-bounds.left, -bounds.top));
        frame.draw(canvas, &self.cache);
        canvas.restore();
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        if !surface.read_pixels(&info, &mut self.pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render(
                "unable to read heading RGBA pixels".into(),
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
        self.last_heading = Some(heading);
        Ok(())
    }
}
