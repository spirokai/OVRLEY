//! Scene config validation.
//!
//! `validate_scene_config` verifies that every output-affecting scene field
//! is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::config::SceneConfig;
use crate::encode::ffmpeg::catalog::{CodecSelection, TransparentCodecId};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::validation::{require_finite_f64, require_positive_f64, require_u32};
use serde_json::{Map, Value};
use std::num::NonZeroU32;

/// Canonical FFmpeg configuration validated from the external scene payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedFfmpegConfig {
    pub codec: CodecSelection,
    pub loglevel: String,
    pub container: Option<String>,
    pub pix_fmt: Option<String>,
    pub output_args: Vec<String>,
    pub qsv_full_init_args: Vec<String>,
}

impl Default for ValidatedFfmpegConfig {
    fn default() -> Self {
        Self {
            codec: CodecSelection::Transparent(TransparentCodecId::ProresKs),
            loglevel: "info".to_string(),
            container: None,
            pix_fmt: None,
            output_args: Vec::new(),
            qsv_full_init_args: Vec::new(),
        }
    }
}

/// Validates the optional external `scene.ffmpeg` object exactly once using
/// the default codec selected from the owning scene's render mode.
pub fn validate_ffmpeg_config(
    value: Value,
    default_codec: CodecSelection,
) -> CoreResult<ValidatedFfmpegConfig> {
    if value.is_null() {
        return Ok(ValidatedFfmpegConfig {
            codec: default_codec,
            ..ValidatedFfmpegConfig::default()
        });
    }
    let object = value
        .as_object()
        .ok_or_else(|| CoreError::Config("scene.ffmpeg must be an object".into()))?;
    let codec = optional_string(object, "codec")?
        .map(|name| {
            CodecSelection::from_external_name(name).ok_or_else(|| {
                CoreError::Config(format!("scene.ffmpeg.codec is unsupported: {name}"))
            })
        })
        .transpose()?
        .unwrap_or(default_codec);
    let loglevel = optional_string(object, "loglevel")?
        .unwrap_or("info")
        .to_string();

    Ok(ValidatedFfmpegConfig {
        codec,
        loglevel,
        container: optional_string(object, "container")?.map(str::to_string),
        pix_fmt: optional_string(object, "pix_fmt")?.map(str::to_string),
        output_args: optional_string_array(object, "output_args")?,
        qsv_full_init_args: optional_string_array(object, "qsv_full_init_args")?,
    })
}

fn optional_string<'a>(object: &'a Map<String, Value>, field: &str) -> CoreResult<Option<&'a str>> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| CoreError::Config(format!("scene.ffmpeg.{field} must be a string")))?;
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "scene.ffmpeg.{field} must not be empty"
        )));
    }
    Ok(Some(value))
}

fn optional_string_array(object: &Map<String, Value>, field: &str) -> CoreResult<Vec<String>> {
    let Some(value) = object.get(field) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        CoreError::Config(format!("scene.ffmpeg.{field} must be an array of strings"))
    })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                CoreError::Config(format!("scene.ffmpeg.{field}[{index}] must be a string"))
            })
        })
        .collect()
}

/// All output-affecting scene fields — no `Option`, no defaults at render time.
///
/// The frontend must materialize every value before sending the config.
/// Missing or invalid fields are rejected by `validate_scene_config`.
#[derive(Clone, Debug)]
pub struct ValidatedSceneConfig {
    pub presentation: ValidatedScenePresentation,
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    pub custom_export_range_active: Option<bool>,
    pub update_rate: NonZeroU32,
    /// Clock origin for elapsed-time widgets, including video lead-in.
    pub export_start_seconds: f64,
    /// Strictly increasing lap IDs the activity sampler excludes from lap widgets.
    pub disabled_laps: Vec<usize>,
}

use ovrley_widgets::model::scene::{validate_scene_presentation, ValidatedScenePresentation};

/// Validates scene config, rejecting missing or out-of-range fields.
pub fn validate_scene_config(raw: SceneConfig) -> CoreResult<ValidatedSceneConfig> {
    let fps = require_positive_f64(raw.fps, "scene.fps")?;
    if fps.fract().abs() > f64::EPSILON || fps > u32::MAX as f64 {
        return Err(CoreError::Config(
            "scene.fps must be a supported positive integer".into(),
        ));
    }
    let start = require_finite_f64(raw.start, "scene.start")?;
    let end = require_finite_f64(raw.end, "scene.end")?;
    if start >= end {
        return Err(CoreError::Config(format!(
            "scene.start ({start}) must be less than scene.end ({end})"
        )));
    }

    let presentation = validate_scene_presentation(raw.presentation)?;

    let update_rate = NonZeroU32::new(require_u32(raw.update_rate, "scene.update_rate")?)
        .ok_or_else(|| CoreError::Config("scene.update_rate: must be > 0".into()))?;
    if !(fps as u32).is_multiple_of(update_rate.get()) {
        return Err(CoreError::Config(format!(
            "scene.update_rate ({update_rate}) must cleanly divide scene.fps ({fps})"
        )));
    }
    let export_start_seconds = if raw.composite_video_path.is_some() {
        require_finite_f64(
            raw.composite_sync_offset.ok_or_else(|| {
                CoreError::Config(
                    "scene.composite_sync_offset required for composite render".into(),
                )
            })?,
            "scene.composite_sync_offset",
        )?
    } else {
        start
    };
    let custom_export_range_active = raw.custom_export_range_active;
    let disabled_laps = validate_disabled_laps(raw.disabled_laps, "scene.disabled_laps")?;

    Ok(ValidatedSceneConfig {
        presentation,
        fps,
        start,
        end,
        custom_export_range_active,
        update_rate,
        export_start_seconds,
        disabled_laps,
    })
}

/// Lap selections are strictly increasing canonical lap IDs; the activity bounds them.
pub(crate) fn validate_disabled_laps(laps: Vec<usize>, field: &str) -> CoreResult<Vec<usize>> {
    if laps.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CoreError::Config(format!(
            "{field} must be strictly increasing lap IDs"
        )));
    }
    Ok(laps)
}

impl ValidatedSceneConfig {
    pub(crate) fn plot_window(
        &self,
        show_full_activity: bool,
    ) -> Option<ovrley_widgets::widgets::route::sampling::RouteWindow> {
        (self.custom_export_range_active.unwrap_or(false) && !show_full_activity).then_some(
            ovrley_widgets::widgets::route::sampling::RouteWindow {
                start: self.start,
                end: self.end,
            },
        )
    }
}
