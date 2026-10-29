//! Activity data contracts used by the renderer.
//!
//! Owns: `ParsedActivity` (frontend-provided JSON shape), `DenseActivityReport`
//!       (frame-aligned interpolated telemetry), `TrimmedActivity` (scene-window
//!       subset), `DebugPayload` (parser debug wrapper), and the type aliases
//!       `NumericSeries`, `TimeSeries`, `CourseSeries`.
//! Does not own: parsing (see [`crate::activity::mod::parse_activity_json`]),
//!       trimming (see [`crate::activity::trim`]), interpolation (see
//!       [`crate::activity::interpolate`] and [`ovrley_widgets::interpolation`]).
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

use chrono_tz::Tz;
use ovrley_widgets::model::types::MetricKind;
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
    /// Optional source-provided activity start instant.
    #[serde(default)]
    pub sync_time: Option<String>,
    /// Exact source lap boundaries when sample-aligned labels are unavailable.
    #[serde(default)]
    pub lap_markers: LapMarkers,
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
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub enum LapMarkers {
    #[default]
    None,
    /// Activity-relative elapsed seconds of exact lap boundaries.
    BeaconMarkers(Vec<f64>),
    /// VBO: start/finish lat/lon points.
    TimingMarkers(Vec<TimingMarker>),
}

/// Single lap-timing marker from a VBO `[laptiming]` section.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimingMarker {
    pub kind: TimingMarkerKind,
    pub latitude_a: f64,
    pub longitude_a: f64,
    pub latitude_b: f64,
    pub longitude_b: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum TimingMarkerKind {
    Start,
    Split,
}

ovrley_widgets::input_activity_struct!(
    columns,
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
        pub original_sample_count: usize,
        pub include_original_sample_count_metadata: bool,
        /// Metadata for implicit boundary detection.
        pub lap_markers: LapMarkers,
    }
);

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
    /// Time horizon used by windowed algorithms, including the circular EMA
    /// time constant for heading.
    pub window_seconds: f64,
}

ovrley_widgets::input_activity_struct!(
    samples,
    /// Normalized extraction sample consumed by backend finalization.
    ///
    /// Every field is optional because not all formats expose every sensor. Missing
    /// values remain `None` so derivation can distinguish absent data from real
    /// zeroes.
    #[derive(Clone, Debug, Default, Deserialize, Serialize)]
    pub struct RawSample {
        /// Marks backend-inserted idle samples for diagnostics.
        #[serde(default)]
        pub synthetic_idle: bool,
    }
);

ovrley_widgets::numeric_activity_struct!(
    parsed,
    /// Parsed source activity as passed from the JavaScript parser to Rust.
    ///
    /// The struct is intentionally forward-compatible: unknown fields are retained
    /// in [`ParsedActivity::extra`] so newer frontend payloads can travel through
    /// older Rust builds without being discarded by serde.
    #[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
        /// Canonical gear labels normalized from source numbers or strings.
        #[serde(default)]
        pub gear_position: GearSeries,
        /// Absolute timestamps aligned with sample times.
        #[serde(default)]
        pub time: TimeSeries,
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
        /// Duration of every lap, indexed by 0-based lap number; the activity's end closes the last.
        #[serde(default)]
        pub lap_durations_seconds: Vec<f64>,
        /// Prefix-min of `lap_durations_seconds`; `best_so_far[n]` is the fastest lap among laps `0..=n`.
        #[serde(default)]
        pub lap_durations_best_so_far_seconds: Vec<f64>,
        /// Unrecognized fields preserved for compatibility with frontend payloads.
        #[serde(flatten)]
        pub extra: BTreeMap<String, Value>,
    }
);

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
#[derive(Clone, Debug, Default, Serialize)]
pub struct DenseActivityReport {
    /// Numeric metric bounds from the entire source activity, before trimming.
    pub full_activity_metric_ranges: std::collections::HashMap<MetricKind, (f64, f64)>,
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

ovrley_widgets::numeric_activity_struct!(
    plain,
    /// Densified telemetry series aligned with a [`DenseActivityReport`].
    #[derive(Clone, Debug, Default, Serialize)]
    pub struct DenseSeriesReport {
        /// Canonical gear labels.
        pub gear_position: GearSeries,
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
);

ovrley_widgets::numeric_activity_struct!(
    plain,
    /// Activity samples after applying a scene trim but before per-frame densifying.
    ///
    /// The first elapsed value is always `0.0`, and the last is `end - start`.
    /// Boundary values are interpolated so downstream interpolation has exact
    /// endpoints even when the trim window cuts through source samples.
    #[derive(Clone, Debug)]
    pub struct TrimmedActivity {
        /// Numeric metric bounds from the entire source activity, before trimming.
        pub full_activity_metric_ranges: std::collections::HashMap<MetricKind, (f64, f64)>,
        /// Trim-adjusted sync time for the trimmed window.
        pub sync_time: Option<String>,
        /// Sample times relative to the trim start.
        pub sample_elapsed_seconds: Vec<f64>,
        /// Absolute activity distance progress for each trimmed sample.
        pub sample_distance_progress: Vec<Option<f64>>,
        /// Trimmed course points.
        pub course: CourseSeries,
        /// Full-activity cumulative positive elevation gain in meters.
        pub full_activity_total_ascent: Option<f64>,
        /// Trimmed gear position samples.
        pub gear_position: GearSeries,
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
);
