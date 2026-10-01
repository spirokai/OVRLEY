//! Scene config validation.
//!
//! `validate_scene_config` verifies that every output-affecting scene field
//! is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{
    require_f32, require_finite_f64, require_non_negative_f32, require_positive_f32,
    require_positive_f64, require_positive_u32, require_u32,
};
use super::raw::SceneConfig;
use crate::encode::ffmpeg::catalog::{CodecSelection, CompositeCodecId, TransparentCodecId};
use crate::encode::quality::{validate_quality, EncodingQuality};
use crate::error::{CoreError, CoreResult};
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
fn validate_ffmpeg_config(
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
    // ── Core timing ───────────────────────────────────────────────────
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    // ── Dimensions ────────────────────────────────────────────────────
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    // ── Render defaults ───────────────────────────────────────────────
    pub font: Option<String>,
    pub font_size: Option<f32>,
    pub opacity: f32,
    pub decimal_rounding: Option<i32>,
    pub time_format: Option<String>,
    pub custom_export_range_active: Option<bool>,
    // ── Shadow/border ─────────────────────────────────────────────────
    pub shadow_color: String,
    pub shadow_strength: f32,
    pub shadow_distance: f32,
    pub border_color: String,
    pub border_thickness: f32,
    // ── Encoding ──────────────────────────────────────────────────────
    pub update_rate: NonZeroU32,
    pub overlay_filename: Option<String>,
    pub ffmpeg: ValidatedFfmpegConfig,
    // ── Composite encoding ────────────────────────────────────────────
    pub composite_video_path: Option<String>,
    pub quality: Option<EncodingQuality>,
    pub composite_sync_offset: Option<f64>,
    pub composite_video_fps_num: Option<u32>,
    pub composite_video_fps_den: Option<u32>,
    pub composite_video_duration: Option<f64>,
    pub composite_render_duration: Option<f64>,
    pub composite_video_trim_start: Option<f64>,
    pub composite_widget_update_rate: Option<NonZeroU32>,
}

/// Validates scene config, rejecting missing or out-of-range fields.
pub fn validate_scene_config(raw: SceneConfig) -> CoreResult<ValidatedSceneConfig> {
    let fps = require_positive_f64(raw.fps, "scene.fps")?;
    let start = require_finite_f64(raw.start, "scene.start")?;
    let end = require_finite_f64(raw.end, "scene.end")?;
    if start >= end {
        return Err(CoreError::Config(format!(
            "scene.start ({start}) must be less than scene.end ({end})"
        )));
    }

    let width = require_positive_u32(raw.width, "scene.width")?;
    let height = require_positive_u32(raw.height, "scene.height")?;
    let scale = require_positive_f32(raw.scale, "scene.scale")?;
    let opacity = require_f32(raw.opacity, "scene.opacity")?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config("scene.opacity must be between 0 and 1".into()));
    }

    let shadow_strength = require_f32(raw.shadow_strength, "scene.shadow_strength")?;
    require_non_negative_f32(shadow_strength, "scene.shadow_strength")?;
    let shadow_distance = require_f32(raw.shadow_distance, "scene.shadow_distance")?;
    require_non_negative_f32(shadow_distance, "scene.shadow_distance")?;
    let shadow_color = raw
        .shadow_color
        .ok_or_else(|| CoreError::Config("scene.shadow_color: required".into()))?;
    let border_thickness = require_f32(raw.border_thickness, "scene.border_thickness")?;
    require_non_negative_f32(border_thickness, "scene.border_thickness")?;
    let border_color = raw
        .border_color
        .ok_or_else(|| CoreError::Config("scene.border_color: required".into()))?;

    let update_rate = NonZeroU32::new(require_u32(raw.update_rate, "scene.update_rate")?)
        .ok_or_else(|| CoreError::Config("scene.update_rate: must be > 0".into()))?;
    let composite_sync_offset = raw.composite_sync_offset;
    let composite_video_trim_start = raw.composite_video_trim_start;
    let composite_widget_update_rate = raw
        .composite_widget_update_rate
        .map(|value| {
            NonZeroU32::new(value).ok_or_else(|| {
                CoreError::Config("scene.composite_widget_update_rate must be at least 1".into())
            })
        })
        .transpose()?;
    let default_codec = if raw.composite_video_path.is_some() {
        CodecSelection::Composite(CompositeCodecId::SoftwareH264)
    } else {
        CodecSelection::Transparent(TransparentCodecId::ProresKs)
    };
    let ffmpeg = validate_ffmpeg_config(raw.ffmpeg, default_codec)?;
    // Rate control is optional for transparent exports and required for composites.
    let quality = match (raw.quality_type, raw.quality_value) {
        (None, None) if raw.composite_video_path.is_none() => None,
        (None, _) => {
            return Err(CoreError::Config(
                "scene.qualityType required for composite render".into(),
            ))
        }
        (_, None) => {
            return Err(CoreError::Config(
                "scene.qualityValue required for composite render".into(),
            ))
        }
        (Some(quality_type), Some(value)) => Some(validate_quality(quality_type, value)?),
    };
    let custom_export_range_active = raw.custom_export_range_active;

    Ok(ValidatedSceneConfig {
        fps,
        start,
        end,
        width,
        height,
        scale,
        font: raw.font,
        font_size: raw.font_size,
        opacity,
        decimal_rounding: raw.decimal_rounding,
        time_format: raw.time_format,
        custom_export_range_active,
        shadow_color,
        shadow_strength,
        shadow_distance,
        border_color,
        border_thickness,
        update_rate,
        overlay_filename: raw.overlay_filename,
        ffmpeg,
        composite_video_path: raw.composite_video_path,
        quality,
        composite_sync_offset,
        composite_video_fps_num: raw.composite_video_fps_num,
        composite_video_fps_den: raw.composite_video_fps_den,
        composite_video_duration: raw.composite_video_duration,
        composite_render_duration: raw.composite_render_duration,
        composite_video_trim_start,
        composite_widget_update_rate,
    })
}
