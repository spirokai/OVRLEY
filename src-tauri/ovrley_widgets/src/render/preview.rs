//! Preview pixel density is independent of scene geometry and native export.
use crate::{
    error::WidgetResult,
    model::{scene::ScenePresentationConfig, validation::require_positive_f32},
};
use skia_safe::{Canvas, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PreviewRasterScale(f32);

impl PreviewRasterScale {
    /// Hosts without a viewport density retain native raster resolution.
    pub(crate) fn from_scene(scene: &ScenePresentationConfig) -> WidgetResult<Self> {
        match scene.extra.get("preview_raster_scale") {
            None => Ok(Self(1.0)),
            Some(value) => require_positive_f32(
                value.as_f64().map(|value| value as f32),
                "scene.preview_raster_scale",
            )
            .map(Self),
        }
    }

    pub(crate) fn dimensions(self, bounds: Rect) -> (u32, u32) {
        (
            (bounds.width() * self.0).ceil() as u32,
            (bounds.height() * self.0).ceil() as u32,
        )
    }

    pub(crate) fn apply(self, canvas: &Canvas) {
        canvas.scale((self.0, self.0));
    }

    pub(crate) fn value(self) -> f32 {
        self.0
    }
}
