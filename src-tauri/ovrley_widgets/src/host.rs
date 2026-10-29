//! Host-owned inputs that sit outside a widget's config and the scene presentation.
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ScenePresentationConfig,
    render::preview::PreviewRasterScale,
    widgets::route::sampling::RouteWindow,
};

/// The editor preview reads these from scene extras once; export supplies them typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetHost {
    pub(crate) raster_scale: PreviewRasterScale,
    /// Custom export range followed by plots that do not show the full activity.
    export_window: Option<RouteWindow>,
    /// Placeholder playback length for plots drawn without activity.
    placeholder_duration: Option<f64>,
    /// Clock origin for elapsed export time.
    export_start_seconds: Option<f64>,
}

impl WidgetHost {
    /// Export draws at native raster resolution and always knows its range and clock origin.
    pub fn export(
        export_window: Option<RouteWindow>,
        placeholder_duration: f64,
        export_start_seconds: f64,
    ) -> Self {
        Self {
            raster_scale: PreviewRasterScale::NATIVE,
            export_window,
            placeholder_duration: Some(placeholder_duration),
            export_start_seconds: Some(export_start_seconds),
        }
    }

    /// Preview extras are optional per family; a present extra must be well formed.
    pub(crate) fn from_scene(scene: &ScenePresentationConfig) -> WidgetResult<Self> {
        let finite = |field: &str| {
            scene
                .extra
                .get(field)
                .map(|value| {
                    value
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| {
                            WidgetError::Config(format!("{field}: required finite second"))
                        })
                })
                .transpose()
        };
        let active = scene
            .extra
            .get("custom_export_range_active")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    WidgetError::Config("custom_export_range_active must be boolean".into())
                })
            })
            .transpose()?
            .unwrap_or(false);
        let export_window = if active {
            let required = |field| {
                finite(field)?
                    .ok_or_else(|| WidgetError::Config(format!("{field}: required finite second")))
            };
            let (start, end) = (required("start")?, required("end")?);
            if start < 0.0 || end <= start {
                return Err(WidgetError::Config("invalid route export window".into()));
            }
            Some(RouteWindow { start, end })
        } else {
            None
        };
        let placeholder_duration = finite("preview_duration_seconds")?;
        if placeholder_duration.is_some_and(|duration| duration < 0.0) {
            return Err(WidgetError::Config(
                "preview_duration_seconds: required non-negative finite second".into(),
            ));
        }
        Ok(Self {
            raster_scale: PreviewRasterScale::from_scene(scene)?,
            export_window,
            placeholder_duration,
            export_start_seconds: finite("export_start_seconds")?,
        })
    }

    pub(crate) fn plot_window(&self, show_full_activity: bool) -> Option<RouteWindow> {
        self.export_window.filter(|_| !show_full_activity)
    }

    pub(crate) fn placeholder_duration(&self) -> WidgetResult<f64> {
        self.placeholder_duration.ok_or_else(|| {
            WidgetError::Config(
                "preview_duration_seconds: required non-negative finite second".into(),
            )
        })
    }

    pub(crate) fn export_start_seconds(&self) -> WidgetResult<f64> {
        self.export_start_seconds.ok_or_else(|| {
            WidgetError::Config("scene.export_start_seconds must be a finite number".into())
        })
    }
}
