use super::{
    layout::{prepare_altitude_tape_cache, AltitudeTapeCache},
    validation::{validate_altitude_tape, TapeSource, ValidatedAltitudeTape},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::preview::{render_tile, PreviewRasterScale},
    render::text::FontResolver,
    widgets::value::{
        config::ValueConfig,
        preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{Canvas, Surface};

/// Offset altitude or depth in metres, and ascent-positive vertical speed in m/s, drawn by one frame.
pub type AltitudeTapeSample = (Option<f64>, Option<f64>);

pub struct PreviewAltitudeTape {
    cache: AltitudeTapeCache,
    preview_raster_scale: PreviewRasterScale,
    surface: Option<Surface>,
    last_sample: Option<AltitudeTapeSample>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewAltitudeTape {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_altitude_tape(widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedAltitudeTape,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_altitude_tape_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            surface: None,
            last_sample: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    /// The samples the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> AltitudeTapeSample {
        activity.map_or((None, None), |activity| {
            let show_rate = self.cache.show_vertical_speed;
            match self.cache.source {
                TapeSource::Altitude => (
                    activity.altitude_at(second, self.cache.starting_altitude_m),
                    show_rate.then(|| activity.vertical_speed_at(second)).flatten(),
                ),
                TapeSource::Depth => (
                    activity.depth_at(second),
                    show_rate.then(|| activity.depth_vertical_speed_at(second)).flatten(),
                ),
            }
        })
    }

    /// Scene rectangle a render draws into; the tape scrolls inside fixed bounds.
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
        let (altitude, speed) = self.frame_key(activity, second);
        self.cache
            .prepare_frame(altitude, speed)
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let sample = self.frame_key(activity, second);
        // The tape scrolls continuously, even between whole-unit readouts.
        if self.last_sample == Some(sample) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(sample.0, sample.1);
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
        self.last_sample = Some(sample);
        Ok(())
    }
}
