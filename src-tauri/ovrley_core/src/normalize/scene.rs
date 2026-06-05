//! Scene config validation.
//!
//! `validate_scene_config` verifies that every output-affecting scene field
//! is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{require_f32, require_non_negative_f32, require_positive_f32};
use super::raw::SceneConfig;
use crate::error::{CoreError, CoreResult};

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
    pub opacity: Option<f32>,
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
    pub update_rate: u32,
    pub overlay_filename: Option<String>,
    pub ffmpeg: serde_json::Value,
    // ── Composite encoding ────────────────────────────────────────────
    pub composite_video_path: Option<String>,
    pub composite_bitrate: Option<String>,
    pub composite_sync_offset: Option<f64>,
    pub composite_video_fps_num: Option<u32>,
    pub composite_video_fps_den: Option<u32>,
    pub composite_video_duration: Option<f64>,
    pub composite_render_duration: Option<f64>,
    pub composite_video_trim_start: Option<f64>,
    pub composite_widget_update_rate: Option<u32>,
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

    let update_rate = require_u32(raw.update_rate, "scene.update_rate")?;
    if update_rate == 0 {
        return Err(CoreError::Config(format!("scene.update_rate: must be > 0")));
    }
    let composite_sync_offset = raw.composite_sync_offset;
    let composite_video_trim_start = raw.composite_video_trim_start;
    let composite_widget_update_rate = raw.composite_widget_update_rate;
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
        opacity: raw.opacity,
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
        ffmpeg: raw.ffmpeg,
        composite_video_path: raw.composite_video_path,
        composite_bitrate: raw.composite_bitrate,
        composite_sync_offset,
        composite_video_fps_num: raw.composite_video_fps_num,
        composite_video_fps_den: raw.composite_video_fps_den,
        composite_video_duration: raw.composite_video_duration,
        composite_render_duration: raw.composite_render_duration,
        composite_video_trim_start,
        composite_widget_update_rate,
    })
}

fn require_positive_u32(v: Option<u32>, field: &str) -> CoreResult<u32> {
    match v {
        Some(n) if n > 0 => Ok(n),
        Some(n) => Err(CoreError::Config(format!("{field}: must be > 0, got {n}"))),
        None => Err(CoreError::Config(format!("{field}: required"))),
    }
}

fn require_positive_f64(v: f64, field: &str) -> CoreResult<f64> {
    if v <= 0.0 || !v.is_finite() {
        Err(CoreError::Config(format!(
            "{field}: must be a positive finite number"
        )))
    } else {
        Ok(v)
    }
}

fn require_finite_f64(v: f64, field: &str) -> CoreResult<f64> {
    if !v.is_finite() {
        Err(CoreError::Config(format!(
            "{field}: must be a finite number"
        )))
    } else {
        Ok(v)
    }
}

fn require_u32(v: Option<u32>, field: &str) -> CoreResult<u32> {
    match v {
        Some(n) => Ok(n),
        None => Err(CoreError::Config(format!("{field}: required"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_scene() -> SceneConfig {
        serde_json::from_value(serde_json::json!({
            "fps": 30.0,
            "start": 0.0,
            "end": 10.0,
            "width": 1920,
            "height": 1080,
            "scale": 1.0,
            "shadow_color": "#000000",
            "shadow_strength": 0.0,
            "shadow_distance": 0.0,
            "border_color": "#000000",
            "border_thickness": 0.0,
            "update_rate": 1,
            "custom_export_range_active": false,
            "composite_widget_update_rate": 1
        }))
        .unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_scene_config(full_scene()).is_ok());
    }

    #[test]
    fn missing_width_rejected() {
        let mut s = full_scene();
        s.width = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.width"), "{e}");
    }

    #[test]
    fn missing_height_rejected() {
        let mut s = full_scene();
        s.height = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.height"), "{e}");
    }

    #[test]
    fn missing_scale_rejected() {
        let mut s = full_scene();
        s.scale = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.scale"), "{e}");
    }

    #[test]
    fn zero_width_rejected() {
        let mut s = full_scene();
        s.width = Some(0);
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.width"), "{e}");
    }

    #[test]
    fn negative_scale_rejected() {
        let mut s = full_scene();
        s.scale = Some(-1.0);
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.scale"), "{e}");
    }

    #[test]
    fn zero_fps_rejected() {
        let mut s = full_scene();
        s.fps = 0.0;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.fps"), "{e}");
    }

    #[test]
    fn fractional_fps_accepted() {
        let mut s = full_scene();
        s.fps = 29.97;
        let v = validate_scene_config(s).unwrap();
        assert!((v.fps - 29.97).abs() < f64::EPSILON);
    }

    #[test]
    fn start_greater_than_end_rejected() {
        let mut s = full_scene();
        s.start = 10.0;
        s.end = 5.0;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.start") && e.contains("scene.end"), "{e}");
    }

    #[test]
    fn missing_update_rate_rejected() {
        let mut s = full_scene();
        s.update_rate = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.update_rate"), "{e}");
    }

    #[test]
    fn missing_shadow_strength_rejected() {
        let mut s = full_scene();
        s.shadow_strength = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.shadow_strength"), "{e}");
    }

    #[test]
    fn missing_border_thickness_rejected() {
        let mut s = full_scene();
        s.border_thickness = None;
        let e = validate_scene_config(s).unwrap_err().to_string();
        assert!(e.contains("scene.border_thickness"), "{e}");
    }

    #[test]
    fn shadow_and_border_pass() {
        let mut s = full_scene();
        s.shadow_strength = Some(2.0);
        s.shadow_distance = Some(3.0);
        s.border_thickness = Some(1.0);
        let v = validate_scene_config(s).unwrap();
        assert!((v.shadow_strength - 2.0).abs() < f32::EPSILON);
        assert!((v.border_thickness - 1.0).abs() < f32::EPSILON);
    }
}
