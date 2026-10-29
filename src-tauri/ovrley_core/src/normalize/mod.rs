//! Template validation, validated render contracts, and telemetry requirements.

mod config;
mod raster;
mod scene;
mod widgets;

use crate::error::CoreResult;
use ovrley_widgets::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use ovrley_widgets::model::telemetry::{mode_activity, widget_telemetry, ActivitySeries};
use ovrley_widgets::widgets::{
    altitude_tape::validation::TapeSource, backdrop::validation::ValidatedBackdrop, elevation::validation::ValidatedElevationPlot,
    diving_profile::validation::ValidatedDivingProfile,
    rolling_chart::validation::ValidatedRollingChart,
    g_force::validation::GForceAxis, label::validation::ValidatedLabel,
    route::validation::ValidatedRoutePlot, value::validation::ValidatedValue,
};
use widgets::validate_render_contents;

pub(crate) use config::deserialize_render_presentation;
pub use config::{
    migrate_saved_font_input, parse_config_json, parse_config_value, parse_template_json,
    parse_template_value, validate_saved_label_typography, RenderConfig, SceneConfig,
    TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION,
};
pub use raster::{validate_template_rasters, RasterSource, ValidatedRaster};
pub use scene::{
    validate_ffmpeg_config, validate_scene_config, ValidatedFfmpegConfig, ValidatedSceneConfig,
};
pub(crate) use scene::validate_disabled_laps;
pub use widgets::{validate_elevation_plots, validate_route_plots};

macro_rules! define_requirements {
    (numeric { $($numeric:ident: $number:ident),* } other { $($other:ident: $variant:ident),* }) => {
        /// Activity columns needed by the selected template. Unused columns skip export preparation.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct RenderDataRequirements {
            $(pub $numeric: bool,)*
            $(pub $other: bool,)*
        }

        impl RenderDataRequirements {
            pub(crate) fn enabled(&self, series: ActivitySeries) -> bool {
                match series {
                    $(ActivitySeries::$number => self.$numeric,)*
                    $(ActivitySeries::$variant => self.$other,)*
                }
            }
        }

        impl Extend<ActivitySeries> for RenderDataRequirements {
            fn extend<I: IntoIterator<Item = ActivitySeries>>(&mut self, series: I) {
                for series in series {
                    match series {
                        $(ActivitySeries::$number => self.$numeric = true,)*
                        $(ActivitySeries::$variant => self.$other = true,)*
                    }
                }
            }
        }
    };
}

ovrley_widgets::activity_series!(define_requirements);

impl RenderDataRequirements {
    fn include_widget(&mut self, key: &str) {
        let activity = widget_telemetry(key);
        self.extend(activity.sources.iter().chain(&activity.requires).copied());
    }
}

impl ValidatedRenderConfig {
    /// Resolves source columns and mode dependencies before trimming/densifying.
    pub fn render_data_requirements(&self) -> RenderDataRequirements {
        let mut requirements = RenderDataRequirements::default();
        for value in &self.values {
            match value {
                ValidatedValue::TimeText(widget) => {
                    requirements.extend(
                        mode_activity("time", widget.formatting.mode())
                            .iter()
                            .copied(),
                    );
                }
                ValidatedValue::LapTimer(widget) => {
                    requirements.extend(
                        mode_activity("lap_timer", widget.mode.as_str())
                            .iter()
                            .copied(),
                    );
                }
                ValidatedValue::GForce(widget) => {
                    // The selected physical axes, rather than the scalar magnitude, drive this display.
                    for axis in [widget.axis_horizontal, widget.axis_vertical] {
                        requirements.extend([match axis {
                            GForceAxis::X => ActivitySeries::GForceX,
                            GForceAxis::Y => ActivitySeries::GForceY,
                            GForceAxis::Z => ActivitySeries::GForceZ,
                        }]);
                    }
                }
                ValidatedValue::AltitudeTape(widget) => {
                    requirements.include_widget(widget.source.metric().as_str());
                    // The trend and its readout sample the source's own rate.
                    if widget.show_vertical_speed {
                        match widget.source {
                            TapeSource::Altitude => requirements.include_widget("vertical_speed"),
                            TapeSource::Depth => {
                                requirements.extend([ActivitySeries::DepthVerticalSpeed])
                            }
                        }
                    }
                }
                _ => requirements.include_widget(value.metric_kind().as_str()),
            }
        }
        if !self.course_plots.is_empty() {
            requirements.include_widget("course");
        }
        if !self.elevation_plots.is_empty() {
            requirements.include_widget("elevation");
        }
        if !self.diving_profiles.is_empty() {
            requirements.include_widget("diving_profile");
        }
        for chart in &self.rolling_charts {
            for series in &chart.series {
                requirements.include_widget(series.metric.as_str());
            }
        }
        requirements
    }
}

/// Validated render config where all output-affecting fields are explicit.
#[derive(Clone)]
pub struct ValidatedRenderConfig<S = ValidatedSceneConfig> {
    pub scene: S,
    pub backdrops: Vec<ValidatedBackdrop>,
    pub rasters: Vec<ValidatedRaster>,
    pub labels: Vec<ValidatedLabel>,
    pub values: Vec<ValidatedValue>,
    pub course_plots: Vec<ValidatedRoutePlot>,
    pub elevation_plots: Vec<ValidatedElevationPlot>,
    pub diving_profiles: Vec<ValidatedDivingProfile>,
    pub rolling_charts: Vec<ValidatedRollingChart>,
}

/// Validates every value widget and label in the config. Returns the first
/// missing or invalid field as an error. Plots are pre-parsed and validated.
pub fn validate_render_config(raw: RenderConfig) -> CoreResult<ValidatedRenderConfig> {
    validate_render_config_with_resources(raw, None)
}

/// Resolves session images while validating submission, before any render job starts.
pub fn validate_render_config_with_resources(
    raw: RenderConfig,
    resources: Option<&dyn crate::raster::RasterResourceResolver>,
) -> CoreResult<ValidatedRenderConfig> {
    let scene = validate_scene_config(raw.scene.clone())?;
    Ok(
        validate_render_contents(raw.with_scene(scene.presentation.clone()), resources)?
            .with_scene(scene),
    )
}

/// Validates shared presentation directly, without inventing a render window.
pub(crate) fn validate_render_presentation(
    raw: RenderConfig<ScenePresentationConfig>,
    resources: Option<&dyn crate::raster::RasterResourceResolver>,
) -> CoreResult<ValidatedRenderConfig<ValidatedScenePresentation>> {
    let scene = validate_scene_presentation(raw.scene.clone())?;
    validate_render_contents(raw.with_scene(scene), resources)
}

impl<S> ValidatedRenderConfig<S> {
    pub(crate) fn with_scene<T>(self, scene: T) -> ValidatedRenderConfig<T> {
        ValidatedRenderConfig {
            scene,
            backdrops: self.backdrops,
            rasters: self.rasters,
            labels: self.labels,
            values: self.values,
            course_plots: self.course_plots,
            elevation_plots: self.elevation_plots,
            diving_profiles: self.diving_profiles,
            rolling_charts: self.rolling_charts,
        }
    }
}

impl ValidatedRenderConfig {
    /// Returns the frame decimation factor used for the encoded video stream.
    pub fn widget_update_rate(&self) -> std::num::NonZeroU32 {
        self.scene.update_rate
    }

    /// Returns the ffmpeg container FPS after applying update_rate.
    pub fn container_fps(&self) -> f64 {
        self.scene.fps / f64::from(self.widget_update_rate().get())
    }
}
