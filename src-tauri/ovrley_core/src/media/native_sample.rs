//! Intermediate telemetry sample shape shared across extraction paths.
//!
//! Telemetry-parser emits tag-map structures that vary by camera vendor
//! (GoPro GPS5, DJI AC004 protobuf, Insta360 time scalars). [`NativeSample`]
//! normalises those vendor-specific shapes into a single representation so
//! smoothing and activity assembly do not need to know which camera produced
//! the data.
//!
//! Owns: [`NativeSample`], [`TelemetrySeriesCounts`].
//! Does not own: extraction, smoothing, time handling, or JSON serialization.

/// Intermediate sample at native telemetry cadence before frontend
/// finalization.
///
/// We keep this separate from the JSON payload so the extraction stage can
/// normalize units and smooth continuous series without committing to the
/// frontend activity model. That lets MP4 share the same final derivation path
/// as FIT/GPX/SRT instead of growing a Rust-only activity builder.
#[derive(Debug, Clone, Default)]
pub struct NativeSample {
    pub timestamp_ms: f64,
    pub timestamp: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub iso: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<f64>,
    pub focal_length: Option<f64>,
    pub ev: Option<f64>,
    pub color_temperature: Option<f64>,
    pub g_force: Option<f64>,
}

impl NativeSample {
    /// True when at least one telemetry domain has data.
    pub fn has_payload(&self) -> bool {
        self.has_gps_payload() || self.has_camera_payload() || self.g_force.is_some()
    }

    /// True when any GPS-derived field is populated.
    pub fn has_gps_payload(&self) -> bool {
        self.timestamp.is_some()
            || self.latitude.is_some()
            || self.longitude.is_some()
            || self.altitude.is_some()
            || self.speed.is_some()
            || self.heading.is_some()
    }

    /// True when any camera-metadata field is populated.
    pub fn has_camera_payload(&self) -> bool {
        self.iso.is_some()
            || self.aperture.is_some()
            || self.shutter_speed.is_some()
            || self.focal_length.is_some()
            || self.ev.is_some()
            || self.color_temperature.is_some()
    }
}

/// Counts of non-null samples per telemetry domain after native extraction.
#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetrySeriesCounts {
    pub gps: usize,
    pub imu: usize,
    pub camera: usize,
}

impl TelemetrySeriesCounts {
    pub fn total(self) -> usize {
        self.gps + self.imu + self.camera
    }
}
