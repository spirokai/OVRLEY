//! Port of `store/slices/createRenderSettingsSlice.js`'s state, defaults and validation (plan D15).
//! The player steps its clock at the render fps divided by the widget update rate, and the history
//! records the settings. The setters arrive with scene settings (Phase 5) and the render dialog
//! (Phase 7); until then the defaults apply, as in a fresh React session. Rate control reuses
//! `ovrley_core`'s validated quality.

use ovrley_core::encode::quality::{validate_quality, EncodingQuality, QualityType};

/// `QUALITY_DEFAULTS`: the default CRF of new projects and codec changes.
pub const H264_QUALITY_DEFAULT: u8 = 21;
pub const HEVC_QUALITY_DEFAULT: u8 = 26;

/// One combined file, or one file per video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Combined,
    Separate,
}

/// The overlay over the video, or the overlay alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// `validateRenderSettings`, at store or submission ingress. The enums and the quality
    /// (validated by `ovrley_core` when it was built) leave the numbers and the codec to check.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.fps.is_finite() && self.fps > 0.0) {
            return Err("Render FPS must be a positive finite number".into());
        }
        if self.widget_update_rate == 0 {
            return Err("Widget update rate must be a positive integer".into());
        }
        if self.codec.is_empty() {
            return Err("Render codec is required".into());
        }
        Ok(())
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
        assert!(RenderSettings { codec: String::new(), ..RenderSettings::default() }.validate().is_err());
        assert!(render_quality(QualityType::Quality, 52.0).is_err());
        assert_eq!(render_quality(QualityType::Bitrate, 8.5), Ok(EncodingQuality::Bitrate(8.5)));
    }
}
