//! Render config contracts: the deserialized config, its validation into a renderer-ready
//! [`ValidatedRenderConfig`], and the activity columns a validated config needs.
//!
//! Deserialized types keep unknown fields (`#[serde(flatten)]`) so configs stay
//! forward-compatible; validation rejects every missing or invalid output-affecting field.

mod scene;
mod widgets;

use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use ovrley_widgets::model::telemetry::{mode_activity, widget_telemetry, ActivitySeries};
use ovrley_widgets::widgets::altitude_tape::TapeSource;
use ovrley_widgets::widgets::backdrop::ValidatedBackdrop;
use ovrley_widgets::widgets::elevation::ValidatedElevationPlot;
use ovrley_widgets::widgets::diving_profile::ValidatedDivingProfile;
use ovrley_widgets::widgets::rolling_chart::ValidatedRollingChart;
use ovrley_widgets::widgets::g_force::GForceAxis;
use ovrley_widgets::widgets::label::ValidatedLabel;
use ovrley_widgets::widgets::map::ValidatedMapWidget;
use ovrley_widgets::widgets::route::ValidatedRoutePlot;
use ovrley_widgets::widgets::value::ValidatedValue;
use widgets::validate_render_contents;
pub use scene::{
    validate_ffmpeg_config, validate_scene_config, ValidatedFfmpegConfig, ValidatedSceneConfig,
};
pub(crate) use scene::validate_disabled_laps;
pub use widgets::{
    validate_elevation_plots, validate_route_plots, validate_template_rasters, RasterSource,
    ValidatedRaster,
};
use ovrley_widgets::widgets::backdrop::BackdropConfig;
use ovrley_widgets::widgets::label::LabelConfig;
use ovrley_widgets::widgets::raster::RasterConfig;
use ovrley_widgets::widgets::value::ValueConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
                        mode_activity("time", widget.formatting.mode().as_str())
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
        if !self.maps.is_empty() {
            requirements.include_widget("map");
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
    pub maps: Vec<ValidatedMapWidget>,
    pub elevation_plots: Vec<ValidatedElevationPlot>,
    pub diving_profiles: Vec<ValidatedDivingProfile>,
    pub rolling_charts: Vec<ValidatedRollingChart>,
}

/// Validates a render config: the scene and every widget collection. Session images resolve
/// through `resources`; without them, rasters render from their file paths.
pub fn validate_render_config(
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
            maps: self.maps,
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





/// Global render settings shared by labels, metric values, plots, and ffmpeg.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    #[serde(flatten)]
    pub presentation: ScenePresentationConfig,
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    #[serde(default, alias = "updateRate")]
    pub update_rate: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_path: Option<String>,
    #[serde(default, rename = "qualityType", skip_serializing)]
    pub quality_type: Option<crate::encode::settings::QualityType>,
    #[serde(default, rename = "qualityValue", skip_serializing)]
    pub quality_value: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_sync_offset: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_video_fps_num: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_fps_den: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_duration: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_render_duration: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_video_trim_start: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_widget_update_rate: Option<u32>,
    #[serde(default)]
    pub ffmpeg: Value,
    #[serde(default)]
    pub custom_export_range_active: Option<bool>,
    /// Editor-disabled lap IDs of the rendered activity; absent when none is disabled.
    #[serde(default, skip_serializing)]
    pub disabled_laps: Vec<usize>,
}




#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig<S = SceneConfig> {
    pub scene: S,
    #[serde(default)]
    pub backdrops: Vec<BackdropConfig>,
    #[serde(default, deserialize_with = "deserialize_render_rasters")]
    pub rasters: Vec<RasterConfig>,
    #[serde(default)]
    pub labels: Vec<LabelConfig>,
    #[serde(default)]
    pub values: Vec<ValueConfig>,
    #[serde(default)]
    pub plots: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Materialized presentation requires explicit collections at JSON ingress.
/// Saved/single config optional collections retain their existing contract.
pub(crate) fn deserialize_render_presentation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RenderConfig<ScenePresentationConfig>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    for field in ["scene", "backdrops", "rasters", "labels", "values", "plots"] {
        if value.get(field).is_none() {
            return Err(serde::de::Error::missing_field(field));
        }
    }
    if !value["plots"].is_array() {
        return Err(serde::de::Error::custom(
            "batch template.plots must be an array",
        ));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

impl<S> RenderConfig<S> {
    pub(crate) fn with_scene<T>(self, scene: T) -> RenderConfig<T> {
        RenderConfig {
            scene,
            backdrops: self.backdrops,
            rasters: self.rasters,
            labels: self.labels,
            values: self.values,
            plots: self.plots,
            extra: self.extra,
        }
    }
}

fn deserialize_render_rasters<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RasterConfig>, D::Error> {
    use serde::de::Error;
    Vec::<RasterConfig>::deserialize(deserializer).map_err(|error| {
        D::Error::custom(format!(
            "rasters: {error} [raster_error:invalid_config:configuration]"
        ))
    })
}

/// Deserializes a render config from JSON text.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))?;
    parse_config_value(&value)
}

/// Deserializes a render config from a JSON value.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_value(value: &Value) -> CoreResult<RenderConfig> {
    serde_json::from_value(value.clone())
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))
}
