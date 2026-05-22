//! Activity data contracts used by the renderer.
//!
//! Owns: `ParsedActivity` (frontend-provided JSON shape), `DenseActivityReport`
//!       (frame-aligned interpolated telemetry), `TrimmedActivity` (scene-window
//!       subset), `DebugPayload` (parser debug wrapper), and the type aliases
//!       `NumericSeries`, `TimeSeries`, `CourseSeries`.
//! Does not own: parsing (see [`crate::activity::mod::parse_activity_json`]),
//!       trimming (see [`crate::activity::trim`]), interpolation (see
//!       [`crate::activity::interpolate`] and [`crate::interpolation`]).
//!
//! Allowed dependencies: `serde`, `serde_json`.
//! Forbidden dependencies: `render`, `encode`, `commands`.
//!
//! Related modules: [`crate::activity::interpolate`] (consumes these types for
//!       densification), [`crate::config`] (consumes `RenderDataRequirements` to
//!       decide which series are needed).
//!
//! ## Serde Contract
//! The frontend parser normalizes GPX/FIT input into these shapes before Rust
//! receives it. Optional telemetry values use `Option<T>` so missing sensor
//! samples remain distinguishable from real zero values during trimming,
//! interpolation, and widget rendering.
//!
//! ## Performance
//! Not a hot path — these types are constructed once per render during the
//! parse-and-prepare phase. The `DenseActivityReport` is read heavily during
//! per-frame rendering (O(1) lookup via `frame_index`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Numeric telemetry series aligned with `sample_elapsed_seconds`.
///
/// `None` means the source file had no valid value for that sample. Consumers
/// interpolate through available values and preserve empty vectors for series
/// that a template did not request.
pub type NumericSeries = Vec<Option<f64>>;

/// Timestamp series aligned with `sample_elapsed_seconds`.
///
/// Values are expected to be RFC 3339 strings when present. Invalid timestamps
/// are ignored by interpolation rather than failing the whole render.
pub type TimeSeries = Vec<Option<String>>;

/// Latitude/longitude pairs aligned with `sample_elapsed_seconds`.
///
/// Each coordinate component is optional because some activity formats can
/// provide partial or sparse course data.
pub type CourseSeries = Vec<(Option<f64>, Option<f64>)>;

/// Parsed source activity as passed from the JavaScript parser to Rust.
///
/// The struct is intentionally forward-compatible: unknown fields are retained
/// in [`ParsedActivity::extra`] so newer frontend payloads can travel through
/// older Rust builds without being discarded by serde.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParsedActivity {
    /// Original source filename when known.
    #[serde(default)]
    pub file_name: Option<String>,
    /// Source format label such as `fit` or `gpx`.
    #[serde(default)]
    pub file_format: Option<String>,
    /// Parser-provided metadata preserved for diagnostics and future widgets.
    #[serde(default)]
    pub metadata: Value,
    /// Absolute activity start time in RFC 3339 format when known.
    #[serde(default)]
    pub source_start_time: Option<String>,
    /// Monotonic sample timestamps in seconds from the source activity start.
    #[serde(default)]
    pub sample_elapsed_seconds: Vec<f64>,
    /// Absolute activity distance progress, normalized to `0.0..=1.0`.
    #[serde(default)]
    pub sample_distance_progress: Vec<f64>,
    /// Precomputed frame elapsed seconds from older payloads.
    #[serde(default)]
    pub frame_elapsed_seconds: Vec<f64>,
    /// Precomputed frame timestamps from older payloads.
    #[serde(default)]
    pub frame_timestamps: Vec<Option<String>>,
    /// Precomputed frame distance progress from older payloads.
    #[serde(default)]
    pub frame_distance_progress: Vec<Option<f64>>,
    /// Default trim start reported by the parser, in seconds.
    #[serde(default)]
    pub trim_start_seconds: f64,
    /// Default trim end reported by the parser, in seconds.
    #[serde(default)]
    pub trim_end_seconds: f64,
    /// Original course samples before any scene trim.
    #[serde(default)]
    pub sample_course_points: CourseSeries,
    /// Original elevation samples before any scene trim.
    #[serde(default)]
    pub sample_elevations: NumericSeries,
    /// Course points aligned with sample times and suitable for interpolation.
    #[serde(default)]
    pub course: CourseSeries,
    /// Elevation in meters.
    #[serde(default)]
    pub elevation: NumericSeries,
    /// Speed in meters per second.
    #[serde(default)]
    pub speed: NumericSeries,
    /// Heart rate in beats per minute.
    #[serde(default)]
    pub heartrate: NumericSeries,
    /// Cadence in revolutions or steps per minute depending on source sport.
    #[serde(default)]
    pub cadence: NumericSeries,
    /// Power in watts.
    #[serde(default)]
    pub power: NumericSeries,
    /// Temperature in degrees Celsius.
    #[serde(default)]
    pub temperature: NumericSeries,
    /// Road grade in percent.
    #[serde(default)]
    pub gradient: NumericSeries,
    /// Absolute timestamps aligned with sample times.
    #[serde(default)]
    pub time: TimeSeries,
    /// Unrecognized fields preserved for compatibility with frontend payloads.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Debug wrapper produced by parser diagnostics.
///
/// Some local debug files contain `{ "parsed_activity": ... }` instead of the
/// activity object directly. Keeping this type small makes that compatibility
/// explicit in the parser.
#[derive(Clone, Debug, Deserialize)]
pub struct DebugPayload {
    /// Wrapped parsed activity payload.
    pub parsed_activity: ParsedActivity,
}

/// Fully trimmed and densified activity data ready for frame rendering.
///
/// Every non-empty vector in this report is aligned to `frame_elapsed_seconds`.
/// Empty vectors mean the corresponding series was not required by the active
/// template and should be skipped by render code.
#[derive(Clone, Debug, Serialize)]
pub struct DenseActivityReport {
    /// Number of layout frames generated for the scene at `scene.fps`.
    pub frame_count: usize,
    /// Per-frame elapsed seconds relative to the scene start.
    pub frame_elapsed_seconds: Vec<f64>,
    /// Per-frame absolute activity distance progress, if a plot requested it.
    pub frame_distance_progress: Vec<Option<f64>>,
    /// Densified telemetry vectors used by text values and widgets.
    pub series: DenseSeriesReport,
}

/// Densified telemetry series aligned with a [`DenseActivityReport`].
#[derive(Clone, Debug, Serialize)]
pub struct DenseSeriesReport {
    /// Speed in meters per second.
    pub speed: Vec<Option<f64>>,
    /// Elevation in meters.
    pub elevation: Vec<Option<f64>>,
    /// Gradient in percent.
    pub gradient: Vec<Option<f64>>,
    /// Heart rate in beats per minute.
    pub heartrate: Vec<Option<f64>>,
    /// Cadence in revolutions or steps per minute.
    pub cadence: Vec<Option<f64>>,
    /// Power in watts.
    pub power: Vec<Option<f64>>,
    /// Temperature in degrees Celsius.
    pub temperature: Vec<Option<f64>>,
    /// Course latitude values.
    pub course_lat: Vec<Option<f64>>,
    /// Course longitude values.
    pub course_lon: Vec<Option<f64>>,
    /// Absolute RFC 3339 timestamps.
    pub time: Vec<Option<String>>,
}

/// Activity samples after applying a scene trim but before per-frame densifying.
///
/// The first elapsed value is always `0.0`, and the last is `end - start`.
/// Boundary values are interpolated so downstream interpolation has exact
/// endpoints even when the trim window cuts through source samples.
#[derive(Clone, Debug)]
pub struct TrimmedActivity {
    /// Trim-adjusted absolute start time, if the source start was known.
    pub source_start_time: Option<String>,
    /// Sample times relative to the trim start.
    pub sample_elapsed_seconds: Vec<f64>,
    /// Absolute activity distance progress for each trimmed sample.
    pub sample_distance_progress: Vec<Option<f64>>,
    /// Trimmed course points.
    pub course: CourseSeries,
    /// Trimmed elevation samples in meters.
    pub elevation: NumericSeries,
    /// Trimmed speed samples in meters per second.
    pub speed: NumericSeries,
    /// Trimmed heart rate samples.
    pub heartrate: NumericSeries,
    /// Trimmed cadence samples.
    pub cadence: NumericSeries,
    /// Trimmed power samples.
    pub power: NumericSeries,
    /// Trimmed temperature samples in Celsius.
    pub temperature: NumericSeries,
    /// Trimmed gradient samples in percent.
    pub gradient: NumericSeries,
    /// Trimmed timestamp samples.
    pub time: TimeSeries,
}
