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

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Numeric telemetry series aligned with `sample_elapsed_seconds`.
///
/// `None` means the source file had no valid value for that sample. Consumers
/// interpolate through available values and preserve empty vectors for series
/// that a template did not request.
pub type NumericSeries = Vec<Option<f64>>;

#[derive(Deserialize)]
#[serde(untagged)]
enum NumericOrBalanceSample {
    Number(f64),
    Balance { value: Option<f64> },
}

fn deserialize_optional_numeric_or_balance_series<'de, D>(
    deserializer: D,
) -> Result<NumericSeries, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<Option<NumericOrBalanceSample>>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .map(|value| match value {
            Some(NumericOrBalanceSample::Number(number)) => Some(number),
            Some(NumericOrBalanceSample::Balance { value }) => value,
            None => None,
        })
        .collect())
}

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
    /// Pace in seconds per kilometer.
    #[serde(default)]
    pub pace: NumericSeries,
    /// G-force in multiples of Earth gravity.
    #[serde(default)]
    pub g_force: NumericSeries,
    /// Air pressure in bar.
    #[serde(default)]
    pub air_pressure: NumericSeries,
    /// Ground contact time in milliseconds.
    #[serde(default)]
    pub ground_contact_time: NumericSeries,
    /// Left/right balance as percent-left.
    #[serde(default, deserialize_with = "deserialize_optional_numeric_or_balance_series")]
    pub left_right_balance: NumericSeries,
    /// Stride length in meters.
    #[serde(default)]
    pub stride_length: NumericSeries,
    /// Stroke rate in strokes per minute.
    #[serde(default)]
    pub stroke_rate: NumericSeries,
    /// Torque in newton-meters.
    #[serde(default)]
    pub torque: NumericSeries,
    /// Vertical speed in meters per second.
    #[serde(default)]
    pub vertical_speed: NumericSeries,
    /// Gear position as a discrete numeric value.
    #[serde(default)]
    pub gear_position: NumericSeries,
    /// Vertical ratio in percent.
    #[serde(default)]
    pub vertical_ratio: NumericSeries,
    /// Vertical oscillation in millimeters.
    #[serde(default)]
    pub vertical_oscillation: NumericSeries,
    /// Core temperature in degrees Celsius.
    #[serde(default)]
    pub core_temperature: NumericSeries,
    /// Road grade in percent.
    #[serde(default)]
    pub gradient: NumericSeries,
    /// Absolute timestamps aligned with sample times.
    #[serde(default)]
    pub time: TimeSeries,
    /// Heading in degrees (0–360).
    #[serde(default)]
    pub heading: NumericSeries,
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
    /// Pace in seconds per kilometer.
    pub pace: Vec<Option<f64>>,
    /// G-force in multiples of Earth gravity.
    pub g_force: Vec<Option<f64>>,
    /// Air pressure in bar.
    pub air_pressure: Vec<Option<f64>>,
    /// Ground contact time in milliseconds.
    pub ground_contact_time: Vec<Option<f64>>,
    /// Left/right balance as percent-left.
    pub left_right_balance: Vec<Option<f64>>,
    /// Stride length in meters.
    pub stride_length: Vec<Option<f64>>,
    /// Stroke rate in strokes per minute.
    pub stroke_rate: Vec<Option<f64>>,
    /// Torque in newton-meters.
    pub torque: Vec<Option<f64>>,
    /// Vertical speed in meters per second.
    pub vertical_speed: Vec<Option<f64>>,
    /// Gear position as a discrete numeric value.
    pub gear_position: Vec<Option<f64>>,
    /// Vertical ratio in percent.
    pub vertical_ratio: Vec<Option<f64>>,
    /// Vertical oscillation in millimeters.
    pub vertical_oscillation: Vec<Option<f64>>,
    /// Core temperature in degrees Celsius.
    pub core_temperature: Vec<Option<f64>>,
    /// Heading in degrees (0–360).
    pub heading: Vec<Option<f64>>,
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
    /// Trimmed pace samples in seconds per kilometer.
    pub pace: NumericSeries,
    /// Trimmed g-force samples.
    pub g_force: NumericSeries,
    /// Trimmed air pressure samples in bar.
    pub air_pressure: NumericSeries,
    /// Trimmed ground contact time samples in milliseconds.
    pub ground_contact_time: NumericSeries,
    /// Trimmed left/right balance samples as percent-left.
    pub left_right_balance: NumericSeries,
    /// Trimmed stride length samples in meters.
    pub stride_length: NumericSeries,
    /// Trimmed stroke rate samples in strokes per minute.
    pub stroke_rate: NumericSeries,
    /// Trimmed torque samples in newton-meters.
    pub torque: NumericSeries,
    /// Trimmed vertical speed samples in meters per second.
    pub vertical_speed: NumericSeries,
    /// Trimmed gear position samples.
    pub gear_position: NumericSeries,
    /// Trimmed vertical ratio samples in percent.
    pub vertical_ratio: NumericSeries,
    /// Trimmed vertical oscillation samples in millimeters.
    pub vertical_oscillation: NumericSeries,
    /// Trimmed core temperature samples in Celsius.
    pub core_temperature: NumericSeries,
    /// Trimmed gradient samples in percent.
    pub gradient: NumericSeries,
    /// Trimmed heading samples in degrees.
    pub heading: NumericSeries,
    /// Trimmed timestamp samples.
    pub time: TimeSeries,
}
