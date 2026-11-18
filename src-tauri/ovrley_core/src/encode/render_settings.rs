//! The editor's render settings (`createRenderSettingsSlice`): the store holds them, the player
//! steps its clock with them, and projects save them. Rate control is the validated
//! [`EncodingQuality`].

use super::quality::{validate_quality, EncodingQuality, QualityType};
use serde::{Deserialize, Serialize};

/// `QUALITY_DEFAULTS`: the default CRF of new projects and codec changes.
pub const H264_QUALITY_DEFAULT: u8 = 21;
pub const HEVC_QUALITY_DEFAULT: u8 = 26;

/// One combined file, or one file per video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Combined,
    Separate,
}

/// The overlay over the video, or the overlay alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportMode {
    Transparent,
    Composite,
}

/// `renderSettings`.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSettings {
    pub output_mode: OutputMode,
    pub fps: f64,
    pub widget_update_rate: u32,
    pub export_mode: ExportMode,
    pub codec: String,
    pub quality: EncodingQuality,
}

impl Default for RenderSettings {
    /// `DEFAULT_RENDER_SETTINGS`.
    fn default() -> Self {
        Self {
            output_mode: OutputMode::Combined,
            fps: 30.0,
            widget_update_rate: 1,
            export_mode: ExportMode::Composite,
            codec: "prores_ks".into(),
            quality: EncodingQuality::Quality(H264_QUALITY_DEFAULT),
        }
    }
}

impl RenderSettings {
    /// `validateRenderSettings`. The enums and the quality (validated when it was built) leave
    /// the numbers and the codec to check.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.fps.is_finite() && self.fps > 0.0) {
            return Err("Render FPS must be a positive finite number".into());
        }
        if self.widget_update_rate == 0 {
            return Err("Widget update rate must be a positive integer".into());
        }
        if self.codec.trim().is_empty() {
            return Err("Render codec is required".into());
        }
        Ok(())
    }

    /// `setRenderFpsAndUpdateRate`: both, or neither when the result is invalid.
    pub fn set_fps_and_update_rate(&mut self, fps: f64, widget_update_rate: u32) -> Result<(), String> {
        let next = Self { fps, widget_update_rate, ..self.clone() };
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// `setRenderWidgetUpdateRate`.
    pub fn set_widget_update_rate(&mut self, widget_update_rate: u32) -> Result<(), String> {
        self.set_fps_and_update_rate(self.fps, widget_update_rate)
    }
}

/// `validateRenderQuality`: rate control from the dialog's type and value.
pub fn render_quality(quality_type: QualityType, value: f64) -> Result<EncodingQuality, String> {
    validate_quality(quality_type, value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid_and_bad_settings_fail() {
        assert_eq!(RenderSettings::default().validate(), Ok(()));
        assert!(RenderSettings { fps: 0.0, ..RenderSettings::default() }.validate().is_err());
        assert!(RenderSettings { widget_update_rate: 0, ..RenderSettings::default() }.validate().is_err());
        assert!(RenderSettings { codec: " ".into(), ..RenderSettings::default() }.validate().is_err());
        assert!(render_quality(QualityType::Quality, 52.0).is_err());
        assert_eq!(render_quality(QualityType::Bitrate, 8.5), Ok(EncodingQuality::Bitrate(8.5)));
    }

    #[test]
    fn setters_validate_before_they_change_anything() {
        let mut settings = RenderSettings::default();
        settings.set_fps_and_update_rate(60.0, 2).unwrap();
        assert_eq!((settings.fps, settings.widget_update_rate), (60.0, 2));
        assert!(settings.set_widget_update_rate(0).is_err());
        assert!(settings.set_fps_and_update_rate(f64::NAN, 1).is_err());
        assert_eq!((settings.fps, settings.widget_update_rate), (60.0, 2));
        settings.set_widget_update_rate(6).unwrap();
        assert_eq!(settings.widget_update_rate, 6);
    }
}
