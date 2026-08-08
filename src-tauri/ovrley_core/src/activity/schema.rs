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
//! Allowed dependencies: `chrono-tz`, `serde`, `serde_json`, and activity metric contracts.
//! Forbidden dependencies: `render`, `encode`, `commands`.
//!
//! Related modules: [`crate::activity::interpolate`] (consumes these types for
//!       densification), [`crate::normalize`] (consumes `RenderDataRequirements` to
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

use super::elevation::preferred_elevation_series;
use crate::MetricKind;
use chrono_tz::Tz;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Numeric telemetry series aligned with `sample_elapsed_seconds`.
///
/// `None` means the source file had no valid value for that sample. Consumers
/// apply the metric's interpolation policy and preserve empty vectors for
/// series that a template did not request.
pub type NumericSeries = Vec<Option<f64>>;

/// Source lap labels aligned with `sample_elapsed_seconds`.
///
/// Present values are normalized at extraction: `-1` is the pre-lap and
/// valid source labels are preserved as supplied.
pub type LapNumberSeries = Vec<Option<i64>>;

/// Canonical string gear observations aligned with `sample_elapsed_seconds`.
pub type GearSeries = Vec<Option<String>>;

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

/// Intermediate activity payload produced by format-specific extraction.
///
/// This is the seam between browser/native parsers and shared Rust
/// finalization: extractors normalize units and field names, then the backend
/// applies the common post-processing rules exactly once.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RawActivity {
    /// Source filename used for diagnostics and frontend display.
    pub file_name: String,
    /// Parser format label used to preserve source provenance.
    pub file_format: String,
    /// Format-specific context carried through without backend interpretation.
    #[serde(default)]
    pub metadata: Value,
    /// Normalized samples in source order, before shared finalization.
    #[serde(default)]
    pub raw_samples: Vec<RawSample>,
    /// Parser-selected processing controls for the shared backend path.
    #[serde(default)]
    pub options: RawActivityOptions,
}

/// Shared post-processing options supplied by extraction.
///
/// Options belong to extraction because source quality differs by format; the
/// finalizer stays a deterministic executor of these explicit requests.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RawActivityOptions {
    /// Lets dense subtitle/video-like sources bypass synthetic idle insertion.
    #[serde(default)]
    pub skip_idle_gap_fill: bool,
    /// Phase 1 smoothing requests keyed by final metric id.
    #[serde(default)]
    pub smoothing: BTreeMap<String, SmoothingOption>,
}

/// Metric-scoped policy for direct observations whose gaps are intentional.
#[derive(Clone, Debug, Default)]
pub struct DirectMetricGapPolicy {
    pub speed: bool,
    pub heading: bool,
}

/// Metadata carrier for implicit lap boundaries.
#[derive(Clone, Debug)]
pub enum LapMarkers {
    None,
    /// AiM: elapsed seconds of start/finish crossings.
    BeaconMarkers(Vec<f64>),
    /// VBO: start/finish lat/lon points.
    TimingMarkers(Vec<TimingMarker>),
}

impl Default for LapMarkers {
    fn default() -> Self {
        Self::None
    }
}

/// Single lap-timing marker from a VBO `[laptiming]` section.
#[derive(Clone, Debug)]
pub struct TimingMarker {
    pub kind: TimingMarkerKind,
    pub latitude_a: f64,
    pub longitude_a: f64,
    pub latitude_b: f64,
    pub longitude_b: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimingMarkerKind {
    Start,
    Split,
}

/// Columnar activity input used by the shared finalizer core.
///
/// Browser parsers can still send row-oriented [`RawActivity`] JSON; the
/// finalizer converts that payload to columns at the boundary. MP4 telemetry
/// alignment naturally produces columns and can feed this shape directly
/// without constructing large sparse row objects.
#[derive(Clone, Debug, Default)]
pub struct ActivityColumns {
    pub file_name: String,
    pub file_format: String,
    pub metadata: Value,
    /// Optional source-provided activity start instant, normalized by the finalizer.
    pub sync_time: Option<String>,
    pub options: RawActivityOptions,
    /// Direct metrics whose intentional source gaps must remain available for interpolation.
    pub preserve_direct_metric_gaps: DirectMetricGapPolicy,
    pub timestamp: TimeSeries,
    pub elapsed_seconds: NumericSeries,
    pub latitude: NumericSeries,
    pub longitude: NumericSeries,
    pub elevation: NumericSeries,
    pub barometric_altitude: NumericSeries,
    pub speed: NumericSeries,
    pub heading: NumericSeries,
    pub heartrate: NumericSeries,
    pub cadence: NumericSeries,
    pub power: NumericSeries,
    pub temperature: NumericSeries,
    pub gradient: NumericSeries,
    pub pace: NumericSeries,
    pub distance: NumericSeries,
    pub distance_to_home: NumericSeries,
    pub g_force: NumericSeries,
    pub g_force_x: NumericSeries,
    pub g_force_y: NumericSeries,
    pub g_force_z: NumericSeries,
    pub rpm: NumericSeries,
    pub throttle_position: NumericSeries,
    pub brake_position: NumericSeries,
    pub lean_angle: NumericSeries,
    pub vertical_speed: NumericSeries,
    pub torque: NumericSeries,
    pub stroke_rate: NumericSeries,
    pub stride_length: NumericSeries,
    pub vertical_oscillation: NumericSeries,
    pub ground_contact_time: NumericSeries,
    pub left_right_balance: NumericSeries,
    pub core_temperature: NumericSeries,
    pub air_pressure: NumericSeries,
    pub calories: NumericSeries,
    pub gear_position: GearSeries,
    pub iso: NumericSeries,
    pub aperture: NumericSeries,
    pub shutter_speed: NumericSeries,
    pub focal_length: NumericSeries,
    pub ev: NumericSeries,
    pub color_temperature: NumericSeries,
    pub original_sample_count: usize,
    pub include_original_sample_count_metadata: bool,
    /// Source-provided canonical lap numbers (empty if absent).
    pub lap_number: LapNumberSeries,
    /// Metadata for implicit boundary detection.
    pub lap_markers: LapMarkers,
}

/// Per-metric smoothing request. Phase 0 only carries this through the schema.
///
/// The shape is introduced with RawActivity so frontend parsers can adopt the
/// contract before the smoothing executor is wired in the next migration phase.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SmoothingOption {
    /// Disabled entries remain serializable so parser defaults can be explicit.
    #[serde(default)]
    pub enabled: bool,
    /// Algorithm identifier chosen by the parser for this metric.
    pub method: String,
    /// Time horizon used by windowed algorithms; circular EMA ignores it.
    pub window_seconds: f64,
}

/// Normalized extraction sample consumed by backend finalization.
///
/// Every field is optional because not all formats expose every sensor. Missing
/// values remain `None` so derivation can distinguish absent data from real
/// zeroes.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RawSample {
    /// Absolute source timestamp, normalized later for final output.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Source-relative elapsed seconds when the format provides it.
    #[serde(default)]
    pub elapsed_seconds: Option<f64>,
    /// GPS latitude in decimal degrees.
    #[serde(default)]
    pub latitude: Option<f64>,
    /// GPS longitude in decimal degrees.
    #[serde(default)]
    pub longitude: Option<f64>,
    /// Elevation used by route/elevation/gradient widgets.
    #[serde(default)]
    pub elevation: Option<f64>,
    /// Barometric altitude in meters when the source identifies it explicitly.
    #[serde(default)]
    pub barometric_altitude: Option<f64>,
    /// Direct source speed in meters per second.
    #[serde(default)]
    pub speed: Option<f64>,
    /// Direct source heading in degrees.
    #[serde(default)]
    pub heading: Option<f64>,
    /// Heart rate in beats per minute.
    #[serde(default)]
    pub heartrate: Option<f64>,
    /// Cadence in source-normalized revolutions/steps per minute.
    #[serde(default)]
    pub cadence: Option<f64>,
    /// Power in watts.
    #[serde(default)]
    pub power: Option<f64>,
    /// Ambient/device temperature in Celsius.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Cumulative energy expenditure in kilocalories.
    #[serde(default)]
    pub calories: Option<f64>,
    /// Direct source gradient when available; standard derivation may override.
    #[serde(default)]
    pub gradient: Option<f64>,
    /// Direct source pace in seconds per kilometer.
    #[serde(default)]
    pub pace: Option<f64>,
    /// Cumulative distance in meters.
    #[serde(default)]
    pub distance: Option<f64>,
    /// Dynamic acceleration in multiples of Earth gravity.
    #[serde(default)]
    pub g_force: Option<f64>,
    /// Direct source vertical speed in meters per second.
    #[serde(default)]
    pub vertical_speed: Option<f64>,
    /// Direct source torque in newton-meters.
    #[serde(default)]
    pub torque: Option<f64>,
    /// Stroke rate in strokes per minute.
    #[serde(default)]
    pub stroke_rate: Option<f64>,
    /// Stride length in meters.
    #[serde(default)]
    pub stride_length: Option<f64>,
    /// Vertical oscillation in the source-normalized display unit.
    #[serde(default)]
    pub vertical_oscillation: Option<f64>,
    /// Ground contact time in milliseconds.
    #[serde(default)]
    pub ground_contact_time: Option<f64>,
    /// Left/right balance as a numeric percent-left value.
    #[serde(default)]
    pub left_right_balance: Option<f64>,
    /// Core/body temperature in Celsius.
    #[serde(default)]
    pub core_temperature: Option<f64>,
    /// Air pressure in bar.
    #[serde(default)]
    pub air_pressure: Option<f64>,
    /// Canonical gear label normalized from a source number or string.
    #[serde(default)]
    pub gear_position: Option<String>,
    /// Camera ISO setting.
    #[serde(default)]
    pub iso: Option<f64>,
    /// Camera aperture f-number.
    #[serde(default)]
    pub aperture: Option<f64>,
    /// Camera shutter speed in seconds.
    #[serde(default)]
    pub shutter_speed: Option<f64>,
    /// Camera focal length in millimeters.
    #[serde(default)]
    pub focal_length: Option<f64>,
    /// Camera exposure value.
    #[serde(default)]
    pub ev: Option<f64>,
    /// Camera white-balance color temperature in Kelvin.
    #[serde(default)]
    pub color_temperature: Option<f64>,
    /// Marks backend-inserted idle samples for diagnostics.
    #[serde(default)]
    pub synthetic_idle: bool,
}

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
    /// Parsed IANA timezone from `metadata.timezone`, when GPS metadata provides one.
    #[serde(skip)]
    pub timezone: Option<Tz>,
    /// Canonical absolute timestamp for activity/video time zero.
    #[serde(default)]
    pub sync_time: Option<String>,
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
    /// Cumulative energy expenditure in kilocalories.
    #[serde(default)]
    pub calories: NumericSeries,
    /// Surface distance from the first valid GPS coordinate in meters.
    #[serde(default)]
    pub distance_to_home: NumericSeries,
    /// Cumulative positive elevation gain in meters.
    #[serde(default)]
    pub total_ascent: NumericSeries,
    /// Barometric altitude in meters when available.
    #[serde(default)]
    pub barometric_altitude: NumericSeries,
    /// Speed in meters per second.
    #[serde(default)]
    pub speed: NumericSeries,
    /// Cumulative distance in meters.
    #[serde(default)]
    pub distance: NumericSeries,
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
    /// Lateral or source X acceleration in multiples of Earth gravity.
    #[serde(default)]
    pub g_force_x: NumericSeries,
    /// Longitudinal/inline or source Y acceleration in multiples of Earth gravity.
    #[serde(default)]
    pub g_force_y: NumericSeries,
    /// Vertical or source Z acceleration in multiples of Earth gravity.
    #[serde(default)]
    pub g_force_z: NumericSeries,
    /// Engine speed in revolutions per minute.
    #[serde(default)]
    pub rpm: NumericSeries,
    /// Accelerator or throttle position as percent.
    #[serde(default)]
    pub throttle_position: NumericSeries,
    /// Brake control position as percent.
    #[serde(default)]
    pub brake_position: NumericSeries,
    /// Signed vehicle lean angle in degrees.
    #[serde(default)]
    pub lean_angle: NumericSeries,
    /// Air pressure in bar.
    #[serde(default)]
    pub air_pressure: NumericSeries,
    /// Ground contact time in milliseconds.
    #[serde(default)]
    pub ground_contact_time: NumericSeries,
    /// Left/right balance as percent-left.
    #[serde(
        default,
        deserialize_with = "deserialize_optional_numeric_or_balance_series"
    )]
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
    /// ISO sensitivity.
    #[serde(default)]
    pub iso: NumericSeries,
    /// Aperture f-number.
    #[serde(default)]
    pub aperture: NumericSeries,
    /// Shutter speed in seconds.
    #[serde(default)]
    pub shutter_speed: NumericSeries,
    /// Focal length in millimeters.
    #[serde(default)]
    pub focal_length: NumericSeries,
    /// Exposure value.
    #[serde(default)]
    pub ev: NumericSeries,
    /// Color temperature in Kelvin.
    #[serde(default)]
    pub color_temperature: NumericSeries,
    /// Canonical gear labels normalized from source numbers or strings.
    #[serde(default)]
    pub gear_position: GearSeries,
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
    /// Lap label; explicit positive source labels are preserved. -1 for pre-lap records.
    #[serde(default)]
    pub lap_number: Vec<i64>,
    /// Seconds since the start of the current lap. Null during out-lap.
    #[serde(default)]
    pub lap_time_seconds: NumericSeries,
    /// Exact activity-relative start time of each canonical lap.
    #[serde(default)]
    pub lap_start_elapsed_seconds: Vec<f64>,
    /// Delta of current lap time versus the best completed lap at the same distance.
    #[serde(default)]
    pub delta_to_best_lap_seconds: NumericSeries,
    /// Duration of the fastest completed lap in the whole session.
    #[serde(default)]
    pub best_lap_time_seconds: Option<f64>,
    /// Array of completed-lap durations, indexed by 0-based lap number.
    #[serde(default)]
    pub lap_durations_seconds: Vec<f64>,
    /// Prefix-min of `lap_durations_seconds`; `best_so_far[n]` is the fastest lap among laps `0..=n`.
    #[serde(default)]
    pub lap_durations_best_so_far_seconds: Vec<f64>,
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
    /// Final source-activity distance from the parsed distance series.
    pub full_activity_distance: Option<f64>,
    /// Final source-activity cumulative positive elevation gain.
    pub full_activity_total_ascent: Option<f64>,
    /// Densified telemetry vectors used by text values and widgets.
    pub series: DenseSeriesReport,
}

/// Densified telemetry series aligned with a [`DenseActivityReport`].
#[derive(Clone, Debug, Serialize)]
pub struct DenseSeriesReport {
    /// Speed in meters per second.
    pub speed: Vec<Option<f64>>,
    /// Cumulative distance in meters.
    pub distance: Vec<Option<f64>>,
    /// Elevation in meters.
    pub elevation: Vec<Option<f64>>,
    /// Cumulative energy expenditure in kilocalories.
    pub calories: Vec<Option<f64>>,
    /// Surface distance from the first valid GPS coordinate in meters.
    pub distance_to_home: Vec<Option<f64>>,
    /// Cumulative positive elevation gain in meters.
    pub total_ascent: Vec<Option<f64>>,
    /// Barometric altitude in meters.
    pub barometric_altitude: Vec<Option<f64>>,
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
    /// Source X acceleration in multiples of Earth gravity.
    pub g_force_x: Vec<Option<f64>>,
    /// Source Y acceleration in multiples of Earth gravity.
    pub g_force_y: Vec<Option<f64>>,
    /// Source Z acceleration in multiples of Earth gravity.
    pub g_force_z: Vec<Option<f64>>,
    /// Engine speed in revolutions per minute.
    pub rpm: Vec<Option<f64>>,
    /// Accelerator or throttle position as percent.
    pub throttle_position: Vec<Option<f64>>,
    /// Brake control position as percent.
    pub brake_position: Vec<Option<f64>>,
    /// Signed vehicle lean angle in degrees.
    pub lean_angle: Vec<Option<f64>>,
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
    /// ISO sensitivity.
    pub iso: Vec<Option<f64>>,
    /// Aperture f-number.
    pub aperture: Vec<Option<f64>>,
    /// Shutter speed in seconds.
    pub shutter_speed: Vec<Option<f64>>,
    /// Focal length in millimeters.
    pub focal_length: Vec<Option<f64>>,
    /// Exposure value.
    pub ev: Vec<Option<f64>>,
    /// Color temperature in Kelvin.
    pub color_temperature: Vec<Option<f64>>,
    /// Canonical gear labels.
    pub gear_position: GearSeries,
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
    /// Lap number for each frame, 0-based.
    pub lap_number: Vec<Option<i64>>,
    /// Lap time in seconds for each frame.
    pub lap_time_seconds: Vec<Option<f64>>,
    /// Exact scene-relative start time of each canonical lap.
    pub lap_start_elapsed_seconds: Vec<f64>,
    /// Delta to best lap in seconds for each frame.
    pub delta_to_best_lap_seconds: Vec<Option<f64>>,
    /// Completed-lap metadata retained from the full activity through scene trim.
    pub lap_durations_seconds: Vec<f64>,
    pub lap_durations_best_so_far_seconds: Vec<f64>,
}

impl DenseSeriesReport {
    /// Returns the canonical numeric dense series for a metric.
    ///
    /// `None` means the metric has no numeric dense series. A returned empty
    /// slice therefore continues to distinguish an unavailable metric from a
    /// present series containing missing frame samples.
    pub(crate) fn numeric_series_for(&self, metric: MetricKind) -> Option<&[Option<f64>]> {
        match metric {
            MetricKind::Speed => Some(&self.speed),
            MetricKind::Distance => Some(&self.distance),
            MetricKind::Elevation => Some(&self.elevation),
            MetricKind::Gradient => Some(&self.gradient),
            MetricKind::Heartrate => Some(&self.heartrate),
            MetricKind::Cadence => Some(&self.cadence),
            MetricKind::Power => Some(&self.power),
            MetricKind::Temperature => Some(&self.temperature),
            MetricKind::Calories => Some(&self.calories),
            MetricKind::Pace => Some(&self.pace),
            MetricKind::GForce => Some(&self.g_force),
            MetricKind::AirPressure => Some(&self.air_pressure),
            MetricKind::GroundContactTime => Some(&self.ground_contact_time),
            MetricKind::LeftRightBalance => Some(&self.left_right_balance),
            MetricKind::StrideLength => Some(&self.stride_length),
            MetricKind::StrokeRate => Some(&self.stroke_rate),
            MetricKind::Torque => Some(&self.torque),
            MetricKind::VerticalSpeed => Some(&self.vertical_speed),
            MetricKind::VerticalRatio => Some(&self.vertical_ratio),
            MetricKind::VerticalOscillation => Some(&self.vertical_oscillation),
            MetricKind::CoreTemperature => Some(&self.core_temperature),
            MetricKind::Heading => Some(&self.heading),
            MetricKind::Altitude => Some(preferred_elevation_series(
                &self.barometric_altitude,
                &self.elevation,
            )),
            MetricKind::Iso => Some(&self.iso),
            MetricKind::Aperture => Some(&self.aperture),
            MetricKind::ShutterSpeed => Some(&self.shutter_speed),
            MetricKind::FocalLength => Some(&self.focal_length),
            MetricKind::Ev => Some(&self.ev),
            MetricKind::ColorTemperature => Some(&self.color_temperature),
            MetricKind::Rpm => Some(&self.rpm),
            MetricKind::ThrottlePosition => Some(&self.throttle_position),
            MetricKind::BrakePosition => Some(&self.brake_position),
            MetricKind::LeanAngle => Some(&self.lean_angle),
            MetricKind::DistanceToHome => Some(&self.distance_to_home),
            MetricKind::TotalAscent => Some(&self.total_ascent),
            MetricKind::GearPosition
            | MetricKind::GpsCoordinates
            | MetricKind::Time
            | MetricKind::LapTimer => None,
        }
    }
}

/// Activity samples after applying a scene trim but before per-frame densifying.
///
/// The first elapsed value is always `0.0`, and the last is `end - start`.
/// Boundary values are interpolated so downstream interpolation has exact
/// endpoints even when the trim window cuts through source samples.
#[derive(Clone, Debug)]
pub struct TrimmedActivity {
    /// Trim-adjusted sync time for the trimmed window.
    pub sync_time: Option<String>,
    /// Sample times relative to the trim start.
    pub sample_elapsed_seconds: Vec<f64>,
    /// Absolute activity distance progress for each trimmed sample.
    pub sample_distance_progress: Vec<Option<f64>>,
    /// Trimmed course points.
    pub course: CourseSeries,
    /// Trimmed elevation samples in meters.
    pub elevation: NumericSeries,
    /// Trimmed cumulative energy expenditure in kilocalories.
    pub calories: NumericSeries,
    /// Trimmed surface distance from the first valid GPS coordinate in meters.
    pub distance_to_home: NumericSeries,
    /// Trimmed cumulative positive elevation gain in meters.
    pub total_ascent: NumericSeries,
    /// Full-activity cumulative positive elevation gain in meters.
    pub full_activity_total_ascent: Option<f64>,
    /// Trimmed barometric-altitude samples in meters.
    pub barometric_altitude: NumericSeries,
    /// Trimmed speed samples in meters per second.
    pub speed: NumericSeries,
    /// Trimmed cumulative distance samples in meters.
    pub distance: NumericSeries,
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
    /// Trimmed source X acceleration samples in multiples of Earth gravity.
    pub g_force_x: NumericSeries,
    /// Trimmed source Y acceleration samples in multiples of Earth gravity.
    pub g_force_y: NumericSeries,
    /// Trimmed source Z acceleration samples in multiples of Earth gravity.
    pub g_force_z: NumericSeries,
    /// Trimmed engine speed samples in revolutions per minute.
    pub rpm: NumericSeries,
    /// Trimmed accelerator or throttle position samples as percent.
    pub throttle_position: NumericSeries,
    /// Trimmed brake control position samples as percent.
    pub brake_position: NumericSeries,
    /// Trimmed signed vehicle lean angle samples in degrees.
    pub lean_angle: NumericSeries,
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
    /// Trimmed ISO samples.
    pub iso: NumericSeries,
    /// Trimmed aperture samples.
    pub aperture: NumericSeries,
    /// Trimmed shutter speed samples in seconds.
    pub shutter_speed: NumericSeries,
    /// Trimmed focal length samples in millimeters.
    pub focal_length: NumericSeries,
    /// Trimmed exposure value samples.
    pub ev: NumericSeries,
    /// Trimmed color temperature samples in Kelvin.
    pub color_temperature: NumericSeries,
    /// Trimmed gear position samples.
    pub gear_position: GearSeries,
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
    /// Final source-activity distance from the parsed distance series.
    pub full_activity_distance: Option<f64>,
    /// Trimmed lap number samples, 0-based.
    pub lap_number: Vec<i64>,
    /// Trimmed lap time samples in seconds.
    pub lap_time_seconds: NumericSeries,
    /// Exact scene-relative start time of each canonical lap.
    pub lap_start_elapsed_seconds: Vec<f64>,
    /// Trimmed delta-to-best-lap samples in seconds.
    pub delta_to_best_lap_seconds: NumericSeries,
    /// Completed-lap metadata retained from the full activity through scene trim.
    pub lap_durations_seconds: Vec<f64>,
    pub lap_durations_best_so_far_seconds: Vec<f64>,
}
